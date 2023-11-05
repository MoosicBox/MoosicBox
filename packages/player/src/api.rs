use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    web::{self, Json},
    Result,
};
use lambda_web::actix_web::{self, get, post};
use moosicbox_core::app::AppState;
use once_cell::sync::Lazy;
use serde::Deserialize;
use thiserror::Error;

use crate::player::{ApiPlaybackStatus, PlaybackStatus, Player, PlayerError, TrackOrId};

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
            PlayerError::InvalidPlaybackType => ErrorBadRequest(format!("Invalid Playback Type")),
            PlayerError::PlaybackError(err) => ErrorInternalServerError(err),
            PlayerError::Send(err) => ErrorInternalServerError(err),
        }
    }
}

static PLAYER: Lazy<Player> = Lazy::new(|| Player::new(None));

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayAlbumQuery {
    pub album_id: i32,
    pub position: Option<u16>,
    pub seek: Option<f64>,
}

#[post("/player/play/album")]
pub async fn play_album_endpoint(
    query: web::Query<PlayAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.play_album(
        data.db.clone().expect("No DB bound on AppState"),
        query.album_id,
        query.position,
        query.seek,
    )?))
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
    Ok(Json(PLAYER.play_track(
        Some(data.db.clone().expect("No DB bound on AppState")),
        TrackOrId::Id(query.track_id),
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

#[derive(Debug, Error)]
pub enum ParseTrackIdsError {
    #[error("Could not parse trackId: {0}")]
    ParseId(String),
    #[error("Could not parse trackId: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

fn parse_track_id_sequences(track_ids: &str) -> std::result::Result<Vec<i32>, ParseTrackIdsError> {
    track_ids
        .split(',')
        .map(|id| {
            id.parse::<i32>()
                .map_err(|_| ParseTrackIdsError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

fn parse_track_id_ranges(
    track_id_ranges: &str,
) -> std::result::Result<Vec<i32>, ParseTrackIdsError> {
    let ranges = track_id_ranges.split('-').collect::<Vec<_>>();

    if ranges.len() == 1 {
        parse_track_id_sequences(ranges[0])
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseTrackIdsError::UnmatchedRange(track_id_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_track_id_sequences(ranges[i])?;
            let mut start_id = start[start.len() - 1] + 1;
            let mut end = parse_track_id_sequences(ranges[i + 1])?;
            let end_id = end[0];

            if end_id - start_id > 100000 {
                return Err(ParseTrackIdsError::RangeTooLarge(format!(
                    "{}-{}",
                    start_id - 1,
                    end_id,
                )));
            }

            ids.append(&mut start);

            while start_id < end_id {
                ids.push(start_id);
                start_id += 1;
            }

            ids.append(&mut end);

            i += 2;
        }

        Ok(ids)
    }
}

#[post("/player/play/tracks")]
pub async fn play_tracks_endpoint(
    query: web::Query<PlayTracksQuery>,
    data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(
        PLAYER.play_tracks(
            Some(data.db.clone().expect("No DB bound on AppState")),
            parse_track_id_ranges(&query.track_ids)
                .map_err(|e| match e {
                    ParseTrackIdsError::ParseId(id) => {
                        ErrorBadRequest(format!("Could not parse trackId '{id}'"))
                    }
                    ParseTrackIdsError::UnmatchedRange(range) => {
                        ErrorBadRequest(format!("Unmatched range '{range}'"))
                    }
                    ParseTrackIdsError::RangeTooLarge(range) => {
                        ErrorBadRequest(format!("Range too large '{range}'"))
                    }
                })?
                .iter()
                .map(|id| TrackOrId::Id(*id))
                .collect(),
            query.position,
            query.seek,
        )?,
    ))
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
    Ok(Json(PLAYER.stop_track(query.playback_id)?))
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
    Ok(Json(PLAYER.seek_track(query.playback_id, query.seek)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlaybackQuery {
    pub playback_id: Option<usize>,
    pub position: Option<u16>,
    pub seek: Option<f64>,
}

#[post("/player/update-playback")]
pub async fn update_playback_endpoint(
    query: web::Query<UpdatePlaybackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.update_playback(
        query.playback_id,
        query.position,
        query.seek,
    )?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NextTrackQuery {
    pub playback_id: Option<usize>,
    pub seek: Option<f64>,
}

#[post("/player/next-track")]
pub async fn next_track_endpoint(
    query: web::Query<NextTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.next_track(query.playback_id, query.seek)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
    pub playback_id: Option<usize>,
}

#[post("/player/pause")]
pub async fn pause_playback_endpoint(
    query: web::Query<PauseQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.pause_playback(query.playback_id)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeQuery {
    pub playback_id: Option<usize>,
}

#[post("/player/resume")]
pub async fn resume_playback_endpoint(
    query: web::Query<ResumeQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.resume_playback(query.playback_id)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviousTrackQuery {
    pub playback_id: Option<usize>,
    pub seek: Option<f64>,
}

#[post("/player/previous-track")]
pub async fn previous_track_endpoint(
    query: web::Query<PreviousTrackQuery>,
    _data: web::Data<AppState>,
) -> Result<Json<PlaybackStatus>> {
    Ok(Json(PLAYER.previous_track(query.playback_id, query.seek)?))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {}

#[get("/player/status")]
pub async fn player_status_endpoint(
    _query: web::Query<PlayerStatusQuery>,
) -> Result<Json<ApiPlaybackStatus>> {
    Ok(Json(PLAYER.player_status()?))
}
