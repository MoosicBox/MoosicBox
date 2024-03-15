use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{
    app::AppState,
    sqlite::models::ApiSource,
    types::{AudioFormat, PlaybackQuality},
};
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::{
    get_track_or_ids_from_track_id_ranges,
    player::{
        get_session_playlist_id_from_session_id, ApiPlaybackStatus, PlaybackRetryOptions,
        PlaybackStatus, Player, PlayerError, PlayerSource,
    },
};

impl From<PlayerError> for actix_web::Error {
    fn from(err: PlayerError) -> Self {
        match err {
            PlayerError::TrackNotFound(track_id) => {
                ErrorNotFound(format!("Track not found: {track_id}"))
            }
            PlayerError::Db(err) => ErrorInternalServerError(format!("DB error: {err:?}")),
            PlayerError::Reqwest(err) => ErrorInternalServerError(format!("Reqwest: {err:?}")),
            PlayerError::Parse(err) => ErrorInternalServerError(format!("Parse: {err:?}")),
            PlayerError::TrackNotLocal(track_id) => {
                ErrorBadRequest(format!("Track not stored locally: {track_id}"))
            }
            PlayerError::TrackFetchFailed(track_id) => {
                ErrorInternalServerError(format!("Failed to fetch track: {track_id}"))
            }
            PlayerError::AlbumFetchFailed(album_id) => {
                ErrorInternalServerError(format!("Failed to fetch album: {album_id}"))
            }
            PlayerError::NoPlayersPlaying => ErrorBadRequest(err),
            PlayerError::PositionOutOfBounds(position) => {
                ErrorBadRequest(format!("Position out of bounds: {position}"))
            }
            PlayerError::PlaybackNotPlaying(id) => {
                ErrorBadRequest(format!("Playback not playing: {id}"))
            }
            PlayerError::PlaybackAlreadyPlaying(id) => {
                ErrorBadRequest(format!("Playback already playing: {id}"))
            }
            PlayerError::InvalidPlaybackType => {
                ErrorBadRequest("Invalid Playback Type".to_string())
            }
            PlayerError::PlaybackError(err) => ErrorInternalServerError(err),
            PlayerError::Send(err) => ErrorInternalServerError(err),
        }
    }
}

static PLAYER_CACHE: Lazy<Arc<Mutex<HashMap<String, Player>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

fn get_player(host: Option<&str>) -> Player {
    PLAYER_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(match &host {
            Some(h) => format!("stream|{h}"),
            None => "local".into(),
        })
        .or_insert(if let Some(host) = host {
            Player::new(
                PlayerSource::Remote {
                    host: host.to_string(),
                    query: None,
                    headers: None,
                },
                Some(super::player::PlaybackType::Stream),
            )
        } else {
            Player::new(PlayerSource::Local, None)
        })
        .clone()
}

const DEFAULT_PLAYBACK_RETRY_OPTIONS: PlaybackRetryOptions = PlaybackRetryOptions {
    max_retry_count: 10,
    retry_delay: std::time::Duration::from_millis(1000),
};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayAlbumQuery {
    pub session_id: Option<usize>,
    pub album_id: i32,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
}

#[post("/player/play/album")]
pub async fn play_album_endpoint(
    query: web::Query<PlayAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(
        get_player(query.host.as_deref())
            .play_album(
                &data.database,
                query.session_id,
                query.album_id,
                query.position,
                query.seek,
                query.volume,
                PlaybackQuality {
                    format: query.format.unwrap_or_default(),
                },
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await?,
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTrackQuery {
    pub session_id: Option<usize>,
    pub track_id: i32,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
    pub source: Option<ApiSource>,
}

#[post("/player/play/track")]
pub async fn play_track_endpoint(
    query: web::Query<PlayTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let track_id = get_track_or_ids_from_track_id_ranges(
        &data.database,
        query.track_id.to_string().as_str(),
        query.source,
        query.host.as_deref(),
    )
    .await?
    .into_iter()
    .next()
    .ok_or(ErrorBadRequest(format!(
        "Invalid trackId '{}'",
        query.track_id
    )))?;

    Ok(Json(
        get_player(query.host.as_deref())
            .play_track(
                &data.database,
                query.session_id,
                track_id,
                query.seek,
                query.volume,
                PlaybackQuality {
                    format: query.format.unwrap_or_default(),
                },
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await?,
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTracksQuery {
    pub session_id: Option<usize>,
    pub track_ids: String,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
    pub source: Option<ApiSource>,
}

#[post("/player/play/tracks")]
pub async fn play_tracks_endpoint(
    query: web::Query<PlayTracksQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let track_ids = get_track_or_ids_from_track_id_ranges(
        &data.database,
        &query.track_ids,
        query.source,
        query.host.as_deref(),
    )
    .await?;

    Ok(Json(
        get_player(query.host.as_deref())
            .play_tracks(
                &data.database,
                query.session_id,
                track_ids,
                query.position,
                query.seek,
                query.volume,
                PlaybackQuality {
                    format: query.format.unwrap_or_default(),
                },
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await?,
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopTrackQuery {
    pub host: Option<String>,
}

#[post("/player/stop")]
pub async fn stop_track_endpoint(
    query: web::Query<StopTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).stop_track()?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SeekTrackQuery {
    pub seek: f64,
    pub host: Option<String>,
}

#[post("/player/seek")]
pub async fn seek_track_endpoint(
    query: web::Query<SeekTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).seek_track(
        query.seek,
        Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlaybackQuery {
    pub play: Option<bool>,
    pub stop: Option<bool>,
    pub playing: Option<bool>,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub track_ids: Option<String>,
    pub format: Option<AudioFormat>,
    pub session_id: Option<usize>,
    pub source: Option<ApiSource>,
}

#[post("/player/update-playback")]
pub async fn update_playback_endpoint(
    query: web::Query<UpdatePlaybackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    let track_ids = if let Some(track_ids) = &query.track_ids {
        Some(
            get_track_or_ids_from_track_id_ranges(
                &data.database,
                &track_ids,
                query.source,
                query.host.as_deref(),
            )
            .await?,
        )
    } else {
        None
    };

    Ok(Json(get_player(query.host.as_deref()).update_playback(
        query.play,
        query.stop,
        query.playing,
        query.position,
        query.seek,
        query.volume,
        track_ids,
        query.format.map(|format| PlaybackQuality { format }),
        query.session_id,
        get_session_playlist_id_from_session_id(&data.database, query.session_id).await?,
        Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NextTrackQuery {
    pub seek: Option<f64>,
    pub host: Option<String>,
}

#[post("/player/next-track")]
pub async fn next_track_endpoint(
    query: web::Query<NextTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).next_track(
        query.seek,
        Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
    pub host: Option<String>,
}

#[post("/player/pause")]
pub async fn pause_playback_endpoint(
    query: web::Query<PauseQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).pause_playback()?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeQuery {
    pub host: Option<String>,
}

#[post("/player/resume")]
pub async fn resume_playback_endpoint(
    query: web::Query<ResumeQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(
        get_player(query.host.as_deref()).resume_playback(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))?,
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviousTrackQuery {
    pub seek: Option<f64>,
    pub host: Option<String>,
}

#[post("/player/previous-track")]
pub async fn previous_track_endpoint(
    query: web::Query<PreviousTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).previous_track(
        query.seek,
        Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {
    pub host: Option<String>,
}

#[get("/player/status")]
pub async fn player_status_endpoint(
    query: web::Query<PlayerStatusQuery>,
) -> Result<Json<ApiPlaybackStatus>> {
    Ok(Json(get_player(query.host.as_deref()).player_status()?))
}
