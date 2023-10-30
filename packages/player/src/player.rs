use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, SendError},
        Arc, Mutex, RwLock,
    },
    u16,
};

use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::get_track,
        models::{ToApi, Track},
    },
};
use moosicbox_symphonia_player::{PlaybackError, Progress};
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
    #[error("Track not found: {0}")]
    TrackNotFound(i32),
    #[error("Track not locally stored: {0}")]
    TrackNotLocal(i32),
    #[error("No players playing")]
    NoPlayersPlaying,
}

#[derive(Serialize)]
pub struct PlaybackStatus {
    pub playback_id: usize,
    pub success: bool,
}

pub fn play_track(db: Db, track_id: i32, seek: Option<f64>) -> Result<PlaybackStatus, PlayerError> {
    play_tracks(db, vec![track_id], None, seek)
}

pub fn play_tracks(
    db: Db,
    track_ids: Vec<i32>,
    position: Option<u16>,
    seek: Option<f64>,
) -> Result<PlaybackStatus, PlayerError> {
    let playback_id = thread_rng().gen::<usize>();

    RT.spawn(async move {
        for (i, track_id) in track_ids.iter().enumerate() {
            if position.is_some_and(|position| (i as u16) < position) {
                continue;
            }

            let track = {
                let library = db.library.lock().unwrap();
                get_track(&library, *track_id).map_err(|e| {
                    eprintln!("Failed to fetch track: {e:?}");
                    PlayerError::TrackFetchFailed(*track_id)
                })?
            };

            println!("Got track {track:?}");

            if track.is_none() {
                return Err(PlayerError::TrackNotFound(*track_id));
            }

            let track = track.unwrap();

            if track.file.is_none() {
                return Err(PlayerError::TrackNotLocal(*track_id));
            }

            println!("track {}", track.id);
            start_playback(playback_id, track, seek)?;
        }

        Ok(0)
    });

    Ok(PlaybackStatus {
        success: true,
        playback_id,
    })
}

fn start_playback(
    playback_id: usize,
    track: Track,
    seek: Option<f64>,
) -> Result<Playback, PlayerError> {
    if let Ok(playback) = get_playback(None) {
        debug!("Stopping existing playback {}", playback.id);
        stop(Some(playback.id))?;
    }
    let progress = Arc::new(RwLock::new(Progress { position: 0.0 }));
    let abort = Arc::new(AtomicBool::new(false));

    let playback = Playback {
        id: playback_id,
        track: track.clone(),
        progress: progress.clone(),
        abort: abort.clone(),
    };

    let (tx, rx) = channel();

    info!("Playing track with Symphonia: {}", track.id);
    moosicbox_symphonia_player::run(
        &track.file.unwrap(),
        true,
        true,
        None,
        seek,
        progress,
        abort,
    )?;

    info!("Finished playback for track {}", track.id);

    ACTIVE_PLAYBACKS.lock().unwrap().remove(&playback_id);

    tx.send(())?;

    ACTIVE_PLAYBACK_RECEIVERS
        .lock()
        .unwrap()
        .insert(playback_id, rx);

    ACTIVE_PLAYBACKS
        .lock()
        .unwrap()
        .insert(playback_id, playback.clone());

    Ok(playback)
}

pub fn stop_track(playback_id: Option<usize>) -> Result<PlaybackStatus, PlayerError> {
    let playback = stop(playback_id)?;

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

fn stop(playback_id: Option<usize>) -> Result<Playback, PlayerError> {
    let playback = get_playback(playback_id)?;

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

    Ok(playback)
}

pub fn seek_track(playback_id: Option<usize>, seek: f64) -> Result<PlaybackStatus, PlayerError> {
    let playback = stop(playback_id)?;
    let playback = start_playback(
        playback_id.unwrap_or(thread_rng().gen::<usize>()),
        playback.track.clone(),
        Some(seek),
    )?;

    Ok(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    })
}

#[derive(Clone)]
pub struct Playback {
    pub id: usize,
    pub track: Track,
    pub progress: Arc<RwLock<Progress>>,
    pub abort: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayback {
    pub track_id: i32,
    pub position: f64,
}

impl ToApi<ApiPlayback> for Playback {
    fn to_api(&self) -> ApiPlayback {
        ApiPlayback {
            track_id: self.track.id,
            position: self.progress.clone().read().unwrap().position,
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
