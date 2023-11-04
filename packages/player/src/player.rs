use std::{
    collections::HashMap,
    fs::File,
    path::Path,
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
use symphonia::core::{
    io::{MediaSource, MediaSourceStream},
    probe::Hint,
};
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
    let to_playable = ToPlayable::Callback(PlayableTrack::from_track_or_id);

    let tracks = {
        let db = db.clone().expect("No DB set");
        let library = db.library.lock().unwrap();
        tracks
            .iter()
            .map(|track| match track {
                TrackOrId::Id(track_id) => {
                    debug!("Fetching track {track_id}",);
                    let track = get_track(&library, *track_id)
                        .map_err(|e| {
                            error!("Failed to fetch track: {e:?}");
                            PlayerError::TrackFetchFailed(*track_id)
                        })
                        .expect("Failed to fetch track");
                    debug!("Got track {track:?}");
                    TrackOrId::Track(track.expect("Track doesnt exist"))
                }
                TrackOrId::Track(track) => TrackOrId::Track(track.clone()),
            })
            .collect()
    };
    let playback = Playback::new(tracks, position);

    play_playback(playback, to_playable, seek)
}

pub fn play_playback(
    playback: Playback,
    to_playable: ToPlayable,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    if let Ok(playback) = get_playback(Some(playback.id)) {
        debug!("Stopping existing playback {}", playback.id);
        stop(Some(playback.id))?;
    }

    RT.spawn(async move {
        let (tx, rx) = channel();

        let mut seek = seek;

        ACTIVE_PLAYBACK_RECEIVERS
            .lock()
            .unwrap()
            .insert(playback.id, rx);

        for (i, track_or_id) in playback.tracks.iter().enumerate() {
            if (i as u16) < playback.position {
                continue;
            }
            let track_id = match track_or_id {
                TrackOrId::Id(id) => *id,
                TrackOrId::Track(track) => track.id,
            };

            // playback.playing = true;
            let playback = Playback {
                id: playback.id,
                tracks: playback.tracks.clone(),
                playing: true,
                position: i as u16,
                progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
                abort: Arc::new(AtomicBool::new(false)),
            };

            ACTIVE_PLAYBACKS
                .lock()
                .unwrap()
                .insert(playback.id, playback.clone());

            debug!("track {} {seek:?}", track_id);

            let seek = if seek.is_some() { seek.take() } else { None };

            start_playback(&playback, to_playable, seek)?;

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

        Ok::<_, PlayerError>(0)
    });

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

#[derive(Copy, Clone)]
pub enum ToPlayable {
    Callback(fn(&TrackOrId) -> PlayableTrack),
}

fn start_playback(
    playback: &Playback,
    to_playable: ToPlayable,
    seek: Option<f64>,
) -> Result<(), PlayerError> {
    let track = &playback.tracks[playback.position as usize];
    let track_id = match track {
        TrackOrId::Id(id) => *id,
        TrackOrId::Track(track) => track.id,
    };
    info!("Playing track with Symphonia: {}", track_id);

    #[allow(unused)]
    #[cfg(feature = "cpal")]
    let audio_output_type = AudioOutputType::Cpal;
    #[allow(unused)]
    #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
    let audio_output_type = AudioOutputType::PulseAudioSimple;
    #[allow(unused)]
    #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
    let audio_output_type = AudioOutputType::PulseAudioStandard;

    let playable_track = match to_playable {
        ToPlayable::Callback(f) => f(track),
    };
    let mss = MediaSourceStream::new(playable_track.source, Default::default());
    moosicbox_symphonia_player::play_media_source(
        mss,
        &playable_track.hint,
        &audio_output_type,
        true,
        true,
        None,
        seek,
        playback.progress.clone(),
        playback.abort.clone(),
    )?;

    info!("Finished playback for track {}", track_id);

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
    start_playback(
        &playback,
        ToPlayable::Callback(PlayableTrack::from_track_or_id),
        Some(seek),
    )?;

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

pub fn next_track(
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

    update_playback(playback_id, Some(playback.position + 1), seek)
}

pub fn previous_track(
    playback_id: Option<usize>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Playing next track {playback_id:?} seek {seek:?}");
    let playback = get_playback(playback_id)?;

    if playback.position == 0 {
        return Err(PlayerError::PositionOutOfBounds(-1));
    }

    update_playback(playback_id, Some(playback.position - 1), seek)
}

pub fn update_playback(
    playback_id: Option<usize>,
    position: Option<u16>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    info!("Updating playback id {playback_id:?} position {position:?} seek {seek:?}");
    let playback = stop(playback_id)?;

    let to_playable = ToPlayable::Callback(PlayableTrack::from_track_or_id);

    let playback = Playback {
        id: playback.id,
        tracks: playback.tracks.clone(),
        playing: true,
        position: position.unwrap_or(playback.position),
        progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
        abort: Arc::new(AtomicBool::new(false)),
    };

    play_playback(playback, to_playable, seek)
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

pub fn resume_playback(playback_id: Option<usize>) -> Result<PlaybackStatus, PlayerError> {
    info!("Resuming playback id {playback_id:?}");
    let playback = get_playback(playback_id)?;

    let id = playback.id;

    if playback.playing {
        return Err(PlayerError::PlaybackAlreadyPlaying(id));
    }

    let seek = Some(playback.progress.read().unwrap().position);

    let to_playable = ToPlayable::Callback(PlayableTrack::from_track_or_id);

    let playback = Playback {
        id,
        tracks: playback.tracks.clone(),
        playing: true,
        position: playback.position,
        progress: Arc::new(RwLock::new(Progress { position: 0.0 })),
        abort: Arc::new(AtomicBool::new(false)),
    };

    play_playback(playback, to_playable, seek)
}

#[derive(Debug, Clone)]
pub enum TrackOrId {
    Track(Track),
    Id(i32),
}

pub struct PlayableTrack {
    pub track_id: i32,
    pub source: Box<dyn MediaSource>,
    pub hint: Hint,
}

impl PlayableTrack {
    fn from_track_or_id(track_or_id: &TrackOrId) -> PlayableTrack {
        match track_or_id {
            TrackOrId::Id(_id) => unreachable!(),
            TrackOrId::Track(track) => PlayableTrack::from_track(track),
        }
    }

    fn from_track(track: &Track) -> PlayableTrack {
        let mut hint = Hint::new();

        let file = track.file.clone().unwrap();
        let path = Path::new(&file);

        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }

        let source = Box::new(File::open(path).unwrap());

        PlayableTrack {
            track_id: track.id,
            source,
            hint,
        }
    }
}

impl std::fmt::Debug for PlayableTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayableTrack")
            .field("track_id", &self.track_id)
            .field("source", &"{{source}}")
            .finish()
    }
}

trait PlayablePlayback {
    fn track_to_playable(track: TrackOrId) -> PlayableTrack;
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

pub struct FilePlayback {}

impl PlayablePlayback for Playback {
    fn track_to_playable(track: TrackOrId) -> PlayableTrack {
        if let TrackOrId::Track(track) = track {
            PlayableTrack::from_track(&track)
        } else {
            unreachable!()
        }
    }
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
