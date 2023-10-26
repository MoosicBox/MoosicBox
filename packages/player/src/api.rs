use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, SendError},
        Arc, Mutex, RwLock,
    },
};

use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get, post};
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::get_track,
        models::{ToApi, Track},
    },
};
use moosicbox_symphonia_player::Progress;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
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

impl From<SendError<()>> for PlaybackError {
    fn from(err: SendError<()>) -> Self {
        PlaybackError::Send(err)
    }
}

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error(transparent)]
    Send(SendError<()>),
    #[error("Not Found Error: {error:?}")]
    Symphonia { error: String },
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTrackQuery {
    pub track_id: i32,
    pub seek: Option<f64>,
}

#[derive(Serialize)]
pub struct PlaybackStatus {
    pub playback_id: usize,
    pub success: bool,
}

#[post("/player/play")]
pub async fn play_track_endpoint(
    query: web::Query<PlayTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let track = {
        let library = data.db.as_ref().unwrap().library.lock().unwrap();
        get_track(&library, query.track_id).map_err(|e| {
            eprintln!("Failed to fetch track: {e:?}");
            ErrorInternalServerError(format!("Failed to fetch track: {e:?}"))
        })?
    };

    println!("Got track {track:?}");

    if track.is_none() {
        return Err(ErrorInternalServerError("Failed to find track"));
    }

    let track = track.unwrap();

    if track.file.is_none() {
        return Err(ErrorInternalServerError("Track doesn't have a local file"));
    }

    println!("track {}", track.id);
    let playback = start_playback(track, query.seek)?;

    Ok(Json(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    }))
}

fn start_playback(track: Track, seek: Option<f64>) -> Result<Playback> {
    if let Ok(playback) = get_playback(None) {
        debug!("Stopping existing playback {}", playback.id);
        stop(Some(playback.id))?;
    }
    let playback_id = thread_rng().gen::<usize>();
    let progress = Arc::new(RwLock::new(Progress { position: 0.0 }));
    let abort = Arc::new(AtomicBool::new(false));

    let playback = Playback {
        id: playback_id,
        track: track.clone(),
        progress: progress.clone(),
        abort: abort.clone(),
    };

    let (tx, rx) = channel();

    RT.spawn(async move {
        info!("Playing track with Symphonia: {}", track.id);
        let response = moosicbox_symphonia_player::run(
            &track.file.unwrap(),
            true,
            true,
            None,
            seek,
            progress,
            abort,
        )
        .map_err(|e| PlaybackError::Symphonia {
            error: format!("{e:?}"),
        });

        info!("Finished playback for track {}", track.id);

        ACTIVE_PLAYBACKS.lock().unwrap().remove(&playback_id);

        tx.send(())?;

        response
    });

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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopTrackQuery {
    pub playback_id: Option<usize>,
}

#[post("/player/stop")]
pub async fn stop_track_endpoint(
    query: web::Query<StopTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let playback = stop(query.playback_id)?;

    Ok(Json(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    }))
}

fn stop(playback_id: Option<usize>) -> Result<Playback> {
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SeekTrackQuery {
    pub playback_id: Option<usize>,
    pub seek: f64,
}

#[post("/player/seek")]
pub async fn seek_track_endpoint(
    query: web::Query<SeekTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let playback = stop(query.playback_id)?;
    let playback = start_playback(playback.track.clone(), Some(query.seek))?;

    Ok(Json(PlaybackStatus {
        success: true,
        playback_id: playback.id,
    }))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {}

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

#[get("/player/status")]
pub async fn player_status_endpoint(
    _query: web::Query<PlayerStatusQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<ApiPlaybackStatus>> {
    Ok(Json(ApiPlaybackStatus {
        active_playbacks: ACTIVE_PLAYBACKS
            .lock()
            .unwrap()
            .iter()
            .map(|x| x.1)
            .map(|x| x.to_api())
            .collect(),
    }))
}

fn get_playback(playback_id: Option<usize>) -> Result<Playback> {
    trace!("Getting by id {playback_id:?}");
    let active_playbacks = ACTIVE_PLAYBACKS.lock().unwrap();

    match playback_id {
        Some(playback_id) => match active_playbacks.get(&playback_id) {
            Some(playback) => Ok(playback.clone()),
            None => Err(ErrorBadRequest("Playback not playing")),
        },
        None => {
            if active_playbacks.len() == 1 {
                Ok(active_playbacks.values().next().unwrap().clone())
            } else {
                Err(ErrorBadRequest("No players playing"))
            }
        }
    }
}
