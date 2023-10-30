use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get, post};
use moosicbox_core::app::AppState;
use serde::Deserialize;

use crate::player::{
    play_track, play_tracks, player_status, seek_track, stop_track, ApiPlaybackStatus,
    PlaybackStatus, PlayerError,
};

impl From<PlayerError> for actix_web::Error {
    fn from(err: PlayerError) -> Self {
        match err {
            PlayerError::TrackNotFound(track_id) => {
                ErrorNotFound(format!("Track not found: {track_id}"))
            }
            PlayerError::TrackNotLocal(track_id) => {
                ErrorBadRequest(format!("Track not stored locally: {track_id}"))
            }
            PlayerError::TrackFetchFailed(track_id) => {
                ErrorInternalServerError(format!("Failed to fetch track: {track_id}"))
            }
            PlayerError::NoPlayersPlaying => ErrorBadRequest(err),
            PlayerError::PlaybackError(err) => ErrorInternalServerError(err),
            PlayerError::Send(err) => ErrorInternalServerError(err),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTrackQuery {
    pub track_id: i32,
    pub seek: Option<f64>,
}

#[post("/player/play/track")]
pub async fn play_track_endpoint(
    query: web::Query<PlayTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(play_track(
        data.db.clone().expect("No DB bound on AppState"),
        query.track_id,
        query.seek,
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTracksQuery {
    pub track_ids: String,
    pub position: Option<u16>,
    pub seek: Option<f64>,
}

#[post("/player/play/tracks")]
pub async fn play_tracks_endpoint(
    query: web::Query<PlayTracksQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(play_tracks(
        data.db.clone().expect("No DB bound on AppState"),
        query
            .track_ids
            .split(",")
            .map(|t| {
                t.parse::<i32>()
                    .map_err(|_| ErrorBadRequest(format!("Could not parse trackId '{t}'")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        query.position,
        query.seek,
    )?))
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
    Ok(Json(stop_track(query.playback_id)?))
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
    Ok(Json(seek_track(query.playback_id, query.seek)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {}

#[get("/player/status")]
pub async fn player_status_endpoint(
    _query: web::Query<PlayerStatusQuery>,
) -> Result<Json<ApiPlaybackStatus>> {
    Ok(Json(player_status()?))
}
