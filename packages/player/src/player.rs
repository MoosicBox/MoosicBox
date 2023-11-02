use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, SendError},
        Arc, Mutex, RwLock,
    },
    u16, usize,
};

use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album_tracks, get_track},
        models::{ToApi, Track},
    },
};
use moosicbox_symphonia_player::{AudioOutputType, PlaybackError, Progress};
use rand::{thread_rng, Rng as _};
use serde::Serialize;
use thiserror::Error;
use tokio::runtime::{self, Runtime};

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
    static ref ACTIVE_PLAYBACKS: Mutex<HashMap<usize, Playback>> = Mutex::new(HashMap::new());
    static ref ACTIVE_PLAYBACK_RECEIVERS: Mutex<HashMap<usize, Receiver<()>>> =
        Mutex::new(HashMap::new());
}

impl From<SendError<()>> for PlayerError {
    fn from(err: SendError<()>) -> Self {
        PlayerError::Send(err)
    }
}

impl From<PlaybackError> for PlayerError {
    fn from(err: PlaybackError) -> Self {
        PlayerError::PlaybackError(err)
    }
}

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error(transparent)]
    Send(SendError<()>),
    #[error(transparent)]
    PlaybackError(moosicbox_symphonia_player::PlaybackError),
    #[error("Track fetch failed: {0}")]
    TrackFetchFailed(i32),
    #[error("Album fetch failed: {0}")]
    AlbumFetchFailed(i32),
    #[error("Track not found: {0}")]
    TrackNotFound(i32),
    #[error("Track not locally stored: {0}")]
    TrackNotLocal(i32),
    #[error("No players playing")]
    NoPlayersPlaying,
    #[error("Position out of bounds: {0}")]
    PositionOutOfBounds(i32),
    #[error("Playback not playing: {0}")]
    PlaybackNotPlaying(usize),
    #[error("Playback already playing: {0}")]
    PlaybackAlreadyPlaying(usize),
}

#[derive(Serialize)]
pub struct PlaybackStatus {
    pub playback_id: usize,
    pub success: bool,
}

pub fn play_album(
    db: Db,
    album_id: i32,
    position: Option<u16>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    let tracks = {
        let library = db.library.lock().unwrap();
        get_album_tracks(&library, album_id).map_err(|e| {
            error!("Failed to fetch album tracks: {e:?}");
            PlayerError::AlbumFetchFailed(album_id)
        })?
    };

    play_tracks(
        Some(db),
        tracks.into_iter().map(TrackOrId::Track).collect(),
        position,
        seek,
    )
}

pub fn play_track(
    db: Option<Db>,
    track: TrackOrId,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    play_tracks(db, vec![track], None, seek)
}

pub fn play_tracks(
    db: Option<Db>,
    tracks: Vec<TrackOrId>,
    position: Option<u16>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    if let Ok(playback) = get_playback(None) {
        debug!("Stopping existing playback {}", playback.id);
        stop(Some(playback.id))?;
    }

    let playback = Playback::new(tracks, position);

    play_playback(db, playback, seek)
}

pub fn play_playback(
    db: Option<Db>,
    playback: Playback,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    if let Ok(playback) = get_playback(Some(playback.id)) {
        debug!("Stopping existing playback {}", playback.id);
        stop(Some(playback.id))?;
    }

    RT.spawn(async move {
        let (tx, rx) = channel();

        let mut seek = seek;
        let mut tracks = playback.tracks.clone();
        let track_ids: Vec<_> = tracks
            .iter()
            .map(|t| match t {
                TrackOrId::Id(id) => *id,
                TrackOrId::Track(t) => t.id,
            })
            .collect();

        ACTIVE_PLAYBACK_RECEIVERS
            .lock()
            .unwrap()
            .insert(playback.id, rx);

        for (i, track_id) in track_ids.iter().enumerate() {
            if (i as u16) < playback.position {
                continue;
            }

            debug!("Fetching track {track_id}");
            let track = {
                let db = db.clone().expect("No DB set");
                let library = db.library.lock().unwrap();
                get_track(&library, *track_id).map_err(|e| {
                    error!("Failed to fetch track: {e:?}");
                    PlayerError::TrackFetchFailed(*track_id)
                })?
            };
            debug!("Got track {track:?}");

            if track.is_none() {
                return Err(PlayerError::TrackNotFound(*track_id));
            }

            let track = track.unwrap();

            if track.file.is_none() {
                return Err(PlayerError::TrackNotLocal(*track_id));
            }

            tracks[i] = TrackOrId::Track(track.clone());

            let playback = Playback {
                id: playback.id,
                tracks: tracks.clone(),
                playing: true,
                position: i as u16,
                progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
                abort: Arc::new(AtomicBool::new(false)),
            };

            ACTIVE_PLAYBACKS
                .lock()
                .unwrap()
                .insert(playback.id, playback.clone());

            debug!("track {} {seek:?}", track.id);
            start_playback(&playback, seek.take())?;

            if playback.abort.load(Ordering::SeqCst) {
                break;
            }
        }

        let mut active_playbacks = ACTIVE_PLAYBACKS.lock().unwrap();

        if active_playbacks
            .get(&playback.id)
            .is_some_and(|p| p.playing)
        {
            active_playbacks.remove(&playback.id);
        }

        tx.send(())?;

        ACTIVE_PLAYBACK_RECEIVERS
            .lock()
            .unwrap()
            .remove(&playback.id);

        Ok(0)
    });

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

fn start_playback(playback: &Playback, seek: Option<f64>) -> Result<(), PlayerError> {
    if let TrackOrId::Track(track) = &playback.tracks[playback.position as usize] {
        info!("Playing track with Symphonia: {}", track.id);

        #[allow(unused)]
        #[cfg(feature = "cpal")]
        let audio_output_type = AudioOutputType::Cpal;
        #[allow(unused)]
        #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
        let audio_output_type = AudioOutputType::PulseAudioSimple;
        #[allow(unused)]
        #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
        let audio_output_type = AudioOutputType::PulseAudioStandard;

        moosicbox_symphonia_player::run(
            &track.file.clone().unwrap(),
            &audio_output_type,
            true,
            true,
            None,
            seek,
            playback.progress.clone(),
            playback.abort.clone(),
        )?;

        info!("Finished playback for track {}", track.id);
    } else {
        unreachable!();
    }

    Ok(())
}

pub fn stop_track(playback_id: Option<usize>) -> Result<PlaybackStatus, PlayerError> {
    let playback = stop(playback_id)?;

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

fn stop(playback_id: Option<usize>) -> Result<Playback, PlayerError> {
    info!("Stopping playback for playback_id {playback_id:?}");
    let playback = get_playback(playback_id)?;
    debug!("Stopping playback {playback:?}");

    playback.abort.clone().store(true, Ordering::SeqCst);

    trace!("Waiting for playback completion response");
    if let Some(rx) = ACTIVE_PLAYBACK_RECEIVERS.lock().unwrap().get(&playback.id) {
        if let Err(_err) = rx.recv() {
            error!("Sender correlated with receiver has dropped");
        }
    }
    trace!("Playback successfully stopped");

    Ok(playback)
}

pub fn seek_track(playback_id: Option<usize>, seek: f64) -> Result<PlaybackStatus, PlayerError> {
    let playback = stop(playback_id)?;
    start_playback(&playback, Some(seek))?;

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

pub fn next_track(
    db: Option<Db>,
    playback_id: Option<usize>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Playing next track {playback_id:?} seek {seek:?}");
    let playback = get_playback(playback_id)?;

    if playback.position + 1 >= playback.tracks.len() as u16 {
        return Err(PlayerError::PositionOutOfBounds(
            playback.position as i32 + 1,
        ));
    }

    update_playback(db, playback_id, Some(playback.position + 1), seek)
}

pub fn previous_track(
    db: Option<Db>,
    playback_id: Option<usize>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Playing next track {playback_id:?} seek {seek:?}");
    let playback = get_playback(playback_id)?;

    if playback.position == 0 {
        return Err(PlayerError::PositionOutOfBounds(-1));
    }

    update_playback(db, playback_id, Some(playback.position - 1), seek)
}

pub fn update_playback(
    db: Option<Db>,
    playback_id: Option<usize>,
    position: Option<u16>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Updating playback id {playback_id:?} position {position:?} seek {seek:?}");
    let playback = stop(playback_id)?;
    let playback = Playback {
        id: playback.id,
        tracks: playback.tracks.clone(),
        playing: true,
        position: position.unwrap_or(playback.position),
        progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
        abort: Arc::new(AtomicBool::new(false)),
    };

    play_playback(db, playback, seek)
}

pub fn pause_playback(playback_id: Option<usize>) -> Result<PlaybackStatus, PlayerError> {
    info!("Pausing playback id {playback_id:?}");
    let playback = get_playback(playback_id)?;

    let id = playback.id;

    if !playback.playing {
        return Err(PlayerError::PlaybackNotPlaying(id));
    }

    playback.abort.clone().store(true, Ordering::SeqCst);

    trace!("Waiting for playback completion response");
    if let Err(_err) = ACTIVE_PLAYBACK_RECEIVERS
        .lock()
        .unwrap()
        .get(&playback.id)
        .unwrap()
        .recv()
    {
        error!("Sender correlated with receiver has dropped");
    }
    trace!("Playback successfully stopped");

    let playback = Playback {
        id,
        tracks: playback.tracks.clone(),
        playing: false,
        position: playback.position,
        progress: playback.progress,
        abort: Arc::new(AtomicBool::new(false)),
    };

    let mut active_playbacks = ACTIVE_PLAYBACKS.lock().unwrap();

    active_playbacks.insert(playback.id, playback);

    Ok(PlaybackStatus {
        success: true,
        playback_id: id,
    })
}

pub fn resume_playback(
    db: Option<Db>,
    playback_id: Option<usize>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Resuming playback id {playback_id:?}");
    let playback = get_playback(playback_id)?;

    let id = playback.id;

    if playback.playing {
        return Err(PlayerError::PlaybackAlreadyPlaying(id));
    }

    let seek = Some(playback.progress.read().unwrap().position);

    let playback = Playback {
        id,
        tracks: playback.tracks.clone(),
        playing: true,
        position: playback.position,
        progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
        abort: Arc::new(AtomicBool::new(false)),
    };

    play_playback(db, playback, seek)
}

#[derive(Debug, Clone)]
pub enum TrackOrId {
    Track(Track),
    Id(i32),
}

#[derive(Debug, Clone)]
pub struct Playback {
    pub id: usize,
    pub tracks: Vec<TrackOrId>,
    pub playing: bool,
    pub position: u16,
    pub progress: Arc<RwLock<Progress>>,
    pub abort: Arc<AtomicBool>,
}

impl Playback {
    pub fn new(tracks: Vec<TrackOrId>, position: Option<u16>) -> Playback {
        Playback {
            id: thread_rng().gen::<usize>(),
            tracks,
            playing: true,
            position: position.unwrap_or_default(),
            progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
            abort: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayback {
    pub track_ids: Vec<i32>,
    pub playing: bool,
    pub position: u16,
    pub seek: f64,
}

impl ToApi<ApiPlayback> for Playback {
    fn to_api(&self) -> ApiPlayback {
        ApiPlayback {
            track_ids: self
                .tracks
                .iter()
                .map(|t| match t {
                    TrackOrId::Track(track) => track.id,
                    TrackOrId::Id(id) => *id,
                })
                .collect(),
            playing: self.playing,
            position: self.position,
            seek: self.progress.clone().read().unwrap().position,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlaybackStatus {
    active_playbacks: Vec<ApiPlayback>,
}

pub fn player_status() -> Result<ApiPlaybackStatus, PlayerError> {
    Ok(ApiPlaybackStatus {
        active_playbacks: ACTIVE_PLAYBACKS
            .lock()
            .unwrap()
            .iter()
            .map(|x| x.1)
            .map(|x| x.to_api())
            .collect(),
    })
}

fn get_playback(playback_id: Option<usize>) -> Result<Playback, PlayerError> {
    trace!("Getting by id {playback_id:?}");
    let active_playbacks = ACTIVE_PLAYBACKS.lock().unwrap();

    match playback_id {
        Some(playback_id) => match active_playbacks.get(&playback_id) {
            Some(playback) => Ok(playback.clone()),
            None => Err(PlayerError::NoPlayersPlaying),
        },
        None => {
            if active_playbacks.len() == 1 {
                Ok(active_playbacks.values().next().unwrap().clone())
            } else {
                Err(PlayerError::NoPlayersPlaying)
            }
        }
    }
}
