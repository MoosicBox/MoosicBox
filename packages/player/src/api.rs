//! HTTP API endpoints for playback control.
//!
//! This module provides RESTful API endpoints for controlling playback, including
//! play, pause, stop, seek, and status operations. It uses Actix-web for HTTP handling
//! and integrates with the core playback functionality.
//!
//! # Endpoints
//!
//! * Play operations: `play_track_endpoint`, `play_tracks_endpoint`, `play_album_endpoint`
//! * Playback control: `pause_playback_endpoint`, `resume_playback_endpoint`, `stop_track_endpoint`
//! * Seeking and position: `seek_track_endpoint`, `next_track_endpoint`, `previous_track_endpoint`
//! * Status: `player_status_endpoint`
//! * General updates: `update_playback_endpoint`

#![allow(clippy::needless_for_each)]
#![allow(clippy::future_not_send)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, post,
    web::{self, Json},
};
use moosicbox_music_api::{MusicApi, MusicApis, SourceToMusicApi as _};
use moosicbox_music_models::{
    ApiSource, AudioFormat, PlaybackQuality, Track,
    id::{Id, ParseIntegersError, parse_integer_ranges_to_ids},
};
use moosicbox_profiles::api::ProfileName;
use moosicbox_session::models::PlaybackTarget;
use serde::Deserialize;

use crate::{
    ApiPlaybackStatus, DEFAULT_PLAYBACK_RETRY_OPTIONS, PlaybackHandler, PlaybackStatus, PlayerError,
};

/// Binds all player API endpoints to the given service scope.
///
/// This function registers all playback control endpoints including play, pause,
/// resume, stop, seek, and status endpoints.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(play_track_endpoint)
        .service(play_tracks_endpoint)
        .service(play_album_endpoint)
        .service(pause_playback_endpoint)
        .service(resume_playback_endpoint)
        .service(update_playback_endpoint)
        .service(next_track_endpoint)
        .service(previous_track_endpoint)
        .service(stop_track_endpoint)
        .service(seek_track_endpoint)
        .service(player_status_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Library")),
    paths(
        play_album_endpoint,
        play_track_endpoint,
        play_tracks_endpoint,
        stop_track_endpoint,
        seek_track_endpoint,
        update_playback_endpoint,
        next_track_endpoint,
        pause_playback_endpoint,
        resume_playback_endpoint,
        previous_track_endpoint,
        player_status_endpoint,
    ),
    components(schemas(
        crate::ApiPlayback,
        ApiPlaybackStatus,
        PlaybackStatus,
    ))
)]
/// `OpenAPI` documentation structure for player API endpoints.
pub struct Api;

impl From<PlayerError> for actix_web::Error {
    fn from(err: PlayerError) -> Self {
        match err {
            PlayerError::TrackNotFound(track_id) => {
                ErrorNotFound(format!("Track not found: {track_id}"))
            }
            PlayerError::DatabaseFetch(err) => {
                ErrorInternalServerError(format!("DB error: {err:?}"))
            }
            PlayerError::Http(err) => ErrorInternalServerError(format!("Http: {err:?}")),
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
            PlayerError::UnsupportedFormat(format) => {
                ErrorBadRequest(format!("Unsupported format: {format:?}"))
            }
            PlayerError::PlaybackError(err) => ErrorInternalServerError(err),
            PlayerError::Send(err) => ErrorInternalServerError(err),
            PlayerError::IO(err) => ErrorInternalServerError(err),
            PlayerError::InvalidSession { .. }
            | PlayerError::Join { .. }
            | PlayerError::NoAudioOutputs
            | PlayerError::Cancelled
            | PlayerError::RetryRequested
            | PlayerError::InvalidState
            | PlayerError::InvalidSource
            | PlayerError::MissingSessionId
            | PlayerError::MissingProfile => ErrorInternalServerError(err),
            PlayerError::Acquire(err) => ErrorInternalServerError(err),
            PlayerError::Seek(err) => ErrorInternalServerError(err),
            PlayerError::AudioOutput(err) => ErrorInternalServerError(err),
        }
    }
}

#[cfg(feature = "local")]
static PLAYER_CACHE: std::sync::LazyLock<
    std::sync::Arc<switchy_async::sync::Mutex<std::collections::BTreeMap<String, PlaybackHandler>>>,
> = std::sync::LazyLock::new(|| {
    std::sync::Arc::new(switchy_async::sync::Mutex::new(
        std::collections::BTreeMap::new(),
    ))
});

#[allow(clippy::unused_async)]
async fn get_player(
    #[allow(unused)] host: Option<&str>,
) -> Result<PlaybackHandler, actix_web::Error> {
    #[cfg(not(feature = "local"))]
    {
        unimplemented!("get_player not supported without `local` feature")
    }
    #[cfg(feature = "local")]
    {
        use crate::{PlayerSource, local::LocalPlayer};
        use moosicbox_audio_output::default_output_factory;

        Ok(PLAYER_CACHE
            .lock()
            .await
            .entry(
                host.as_ref()
                    .map_or_else(|| "local".into(), |h| format!("stream|{h}")),
            )
            .or_insert(if let Some(host) = host {
                let local_player = LocalPlayer::new(
                    PlayerSource::Remote {
                        host: host.to_string(),
                        query: None,
                        headers: None,
                    },
                    Some(super::PlaybackType::Stream),
                )
                .await
                .map_err(ErrorInternalServerError)?
                .with_output(
                    default_output_factory()
                        .await
                        .ok_or_else(|| ErrorInternalServerError("Missing default audio output"))?,
                );

                let playback = local_player.playback.clone();
                let output = local_player.output.clone();

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(output);

                local_player
                    .playback_handler
                    .write()
                    .unwrap()
                    .replace(handler.clone());

                handler
            } else {
                let local_player =
                    LocalPlayer::new(PlayerSource::Local, None)
                        .await
                        .map_err(ErrorInternalServerError)?
                        .with_output(default_output_factory().await.ok_or_else(|| {
                            ErrorInternalServerError("Missing default audio output")
                        })?);

                let playback = local_player.playback.clone();
                let output = local_player.output.clone();

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(output);

                local_player
                    .playback_handler
                    .write()
                    .unwrap()
                    .replace(handler.clone());

                handler
            })
            .clone())
    }
}

/// Retrieves tracks from the music API by parsing track ID ranges.
///
/// Parses a string containing track IDs or ranges (e.g., "1,3-5,7") and fetches
/// the corresponding tracks from the music API.
///
/// # Errors
///
/// * If failed to get the tracks from the `MusicApi`
/// * If failed to parse the track ID ranges
pub async fn get_track_or_ids_from_track_id_ranges(
    api: &dyn MusicApi,
    track_ids: &str,
) -> Result<Vec<Track>> {
    let track_ids = parse_integer_ranges_to_ids(track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    api.tracks(Some(track_ids.as_ref()), None, None, None, None)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get tracks: {e:?}")))?
        .with_rest_of_items_in_batches()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get tracks: {e:?}")))
}

/// Query parameters for playing an album.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayAlbumQuery {
    /// Session ID to play the album on
    pub session_id: u64,
    /// Album ID to play
    pub album_id: String,
    /// Starting track position in the album
    pub position: Option<u16>,
    /// Seek position in seconds within the track
    pub seek: Option<f64>,
    /// Playback volume (0.0 to 1.0)
    pub volume: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
    /// Audio format for playback
    pub format: Option<AudioFormat>,
    /// API source for the album
    pub source: Option<ApiSource>,
    /// Audio zone ID for multi-room audio
    pub audio_zone_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/play/album",
        description = "Play the given album for the specified host or local player",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("sessionId" = u64, Query, description = "Session ID to play the album on"),
            ("albumId" = String, Query, description = "Album ID to play"),
            ("position" = Option<u16>, Query, description = "Position in the playlist to play from"),
            ("seek" = Option<f64>, Query, description = "Seek position to begin playback from"),
            ("volume" = Option<f64>, Query, description = "Volume level to play at"),
            ("host" = Option<String>, Query, description = "Remote host to fetch track audio from"),
            ("format" = Option<AudioFormat>, Query, description = "Audio format to play the tracks in"),
            ("source" = Option<ApiSource>, Query, description = "API source to fetch the tracks from"),
            ("audioZoneId" = Option<u64>, Query, description = "Audio zone ID to play from"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/play/album")]
pub async fn play_album_endpoint(
    query: web::Query<PlayAlbumQuery>,
    profile: ProfileName,
    music_apis: MusicApis,
) -> Result<Json<PlaybackStatus>> {
    let source = query.source.clone().unwrap_or_else(ApiSource::library);
    let album_id = Id::try_from_str(query.album_id.as_str(), &source)
        .map_err(|e| ErrorBadRequest(format!("Invalid album id: {e:?}")))?;

    get_player(query.host.as_deref())
        .await?
        .play_album(
            &**music_apis
                .get(&source)
                .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
            query.session_id,
            profile.into(),
            &album_id,
            query.position,
            query.seek,
            query.volume,
            PlaybackQuality {
                format: query.format.unwrap_or_default(),
            },
            query
                .audio_zone_id
                .map(|audio_zone_id| PlaybackTarget::AudioZone { audio_zone_id }),
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
        )
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for playing a single track.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTrackQuery {
    /// Session ID to play the track on
    pub session_id: u64,
    /// Track ID to play
    pub track_id: i32,
    /// Seek position in seconds within the track
    pub seek: Option<f64>,
    /// Playback volume (0.0 to 1.0)
    pub volume: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
    /// Audio format for playback
    pub format: Option<AudioFormat>,
    /// API source for the track
    pub source: Option<ApiSource>,
    /// Audio zone ID for multi-room audio
    pub audio_zone_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/play/track",
        description = "Play the given track for the specified host or local player",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("sessionId" = Option<u64>, Query, description = "Session ID to play the album on"),
            ("trackId" = i32, Query, description = "Track ID to play"),
            ("seek" = Option<f64>, Query, description = "Seek position to begin playback from"),
            ("volume" = Option<f64>, Query, description = "Volume level to play at"),
            ("host" = Option<String>, Query, description = "Remote host to fetch track audio from"),
            ("format" = Option<AudioFormat>, Query, description = "Audio format to play the tracks in"),
            ("source" = Option<ApiSource>, Query, description = "API source to fetch the tracks from"),
            ("audioZoneId" = Option<u64>, Query, description = "Audio zone ID to play from"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/play/track")]
pub async fn play_track_endpoint(
    query: web::Query<PlayTrackQuery>,
    music_apis: MusicApis,
    profile: ProfileName,
) -> Result<Json<PlaybackStatus>> {
    let track_id = get_track_or_ids_from_track_id_ranges(
        &**music_apis
            .get(&query.source.clone().unwrap_or_else(ApiSource::library))
            .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
        query.track_id.to_string().as_str(),
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| ErrorBadRequest(format!("Invalid trackId '{}'", query.track_id)))?;

    get_player(query.host.as_deref())
        .await?
        .play_track(
            query.session_id,
            profile.into(),
            track_id,
            query.seek,
            query.volume,
            PlaybackQuality {
                format: query.format.unwrap_or_default(),
            },
            query
                .audio_zone_id
                .map(|audio_zone_id| PlaybackTarget::AudioZone { audio_zone_id }),
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
        )
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for playing multiple tracks.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTracksQuery {
    /// Session ID to play the tracks on
    pub session_id: u64,
    /// Comma-separated list of track IDs to play
    pub track_ids: String,
    /// Starting track position in the playlist
    pub position: Option<u16>,
    /// Seek position in seconds within the starting track
    pub seek: Option<f64>,
    /// Playback volume (0.0 to 1.0)
    pub volume: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
    /// Audio format for playback
    pub format: Option<AudioFormat>,
    /// API source for the tracks
    pub source: Option<ApiSource>,
    /// Audio zone ID for multi-room audio
    pub audio_zone_id: Option<u64>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/play/tracks",
        description = "Play the given tracks for the specified host or local player",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("sessionId" = Option<u64>, Query, description = "Session ID to play the album on"),
            ("trackIds" = String, Query, description = "Comma-separated list of track IDs to play"),
            ("position" = Option<u16>, Query, description = "Position in the list of tracks to play from"),
            ("seek" = Option<f64>, Query, description = "Seek position to begin playback from"),
            ("volume" = Option<f64>, Query, description = "Volume level to play at"),
            ("host" = Option<String>, Query, description = "Remote host to fetch track audio from"),
            ("format" = Option<AudioFormat>, Query, description = "Audio format to play the tracks in"),
            ("source" = Option<ApiSource>, Query, description = "API source to fetch the tracks from"),
            ("audioZoneId" = Option<u64>, Query, description = "Audio zone ID to play from"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/play/tracks")]
pub async fn play_tracks_endpoint(
    query: web::Query<PlayTracksQuery>,
    profile: ProfileName,
    music_apis: MusicApis,
) -> Result<Json<PlaybackStatus>> {
    let track_ids = get_track_or_ids_from_track_id_ranges(
        &**music_apis
            .get(&query.source.clone().unwrap_or_else(ApiSource::library))
            .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
        &query.track_ids,
    )
    .await?;

    get_player(query.host.as_deref())
        .await?
        .play_tracks(
            query.session_id,
            profile.into(),
            track_ids,
            query.position,
            query.seek,
            query.volume,
            PlaybackQuality {
                format: query.format.unwrap_or_default(),
            },
            query
                .audio_zone_id
                .map(|audio_zone_id| PlaybackTarget::AudioZone { audio_zone_id }),
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
        )
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for stopping playback.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopTrackQuery {
    /// Remote host to stop playback on
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/stop",
        description = "Stop the current playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("host" = Option<String>, Query, description = "Remote host to stop playback from"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/stop")]
pub async fn stop_track_endpoint(
    query: web::Query<StopTrackQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .stop(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for seeking playback position.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SeekTrackQuery {
    /// Seek position in seconds
    pub seek: f64,
    /// Remote host to seek playback on
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/seek",
        description = "Seek the current playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("seek" = Option<f64>, Query, description = "Position to seek the playback to"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/seek")]
pub async fn seek_track_endpoint(
    query: web::Query<SeekTrackQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .seek(query.seek, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for updating playback state.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlaybackQuery {
    /// Start playback
    pub play: Option<bool>,
    /// Stop playback
    pub stop: Option<bool>,
    /// Set playing state
    pub playing: Option<bool>,
    /// Track position in playlist
    pub position: Option<u16>,
    /// Seek position in seconds
    pub seek: Option<f64>,
    /// Playback volume (0.0 to 1.0)
    pub volume: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
    /// Comma-separated list of track IDs
    pub track_ids: Option<String>,
    /// Audio format for playback
    pub format: Option<AudioFormat>,
    /// Session ID for playback
    pub session_id: Option<u64>,
    /// Audio zone ID for multi-room audio
    pub audio_zone_id: Option<u64>,
    /// API source for tracks
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/update-playback",
        description = "Update a playback for the player",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("play" = Option<bool>, Query, description = "Trigger playback to begin on this update"),
            ("stop" = Option<bool>, Query, description = "Trigger playback to stop on this update"),
            ("playing" = Option<bool>, Query, description = "Update the 'playing' status on the playback"),
            ("position" = Option<u16>, Query, description = "Update the 'position' status on the playback"),
            ("seek" = Option<f64>, Query, description = "Update the 'seek' status on the playback"),
            ("volume" = Option<f64>, Query, description = "Update the 'volume' status on the playback"),
            ("host" = Option<String>, Query, description = "Remote host to fetch track audio from"),
            ("trackIds" = String, Query, description = "Comma-separated list of track IDs to update the playback with"),
            ("format" = Option<AudioFormat>, Query, description = "Update the 'format' status on the playback"),
            ("sessionId" = Option<u64>, Query, description = "Session ID to update the playback for"),
            ("audioZoneId" = Option<u64>, Query, description = "Audio zone ID to update the playback for"),
            ("source" = Option<ApiSource>, Query, description = "Update the 'source' status on the playback"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = PlaybackStatus,
            )
        )
    )
)]
#[post("/update-playback")]
pub async fn update_playback_endpoint(
    query: web::Query<UpdatePlaybackQuery>,
    profile: ProfileName,
    music_apis: MusicApis,
) -> Result<Json<PlaybackStatus>> {
    let track_ids = if let Some(track_ids) = &query.track_ids {
        Some(
            get_track_or_ids_from_track_id_ranges(
                &**music_apis
                    .get(&query.source.clone().unwrap_or_else(ApiSource::library))
                    .ok_or_else(|| ErrorBadRequest("Invalid source"))?,
                track_ids,
            )
            .await?,
        )
    } else {
        None
    };

    get_player(query.host.as_deref())
        .await?
        .update_playback(
            true,
            query.play,
            query.stop,
            query.playing,
            query.position,
            query.seek,
            query.volume,
            track_ids,
            query.format.map(|format| PlaybackQuality { format }),
            query.session_id,
            Some(profile.into()),
            query
                .audio_zone_id
                .map(|audio_zone_id| PlaybackTarget::AudioZone { audio_zone_id }),
            true,
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
        )
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for skipping to the next track.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NextTrackQuery {
    /// Seek position in seconds for the next track
    pub seek: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/next-track",
        description = "Skip to the next track for the playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("seek" = Option<f64>, Query, description = "Position to seek the next track on the playback to"),
            ("host" = Option<String>, Query, description = "Remote host to skip to the next track on the playback for"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/next-track")]
pub async fn next_track_endpoint(
    query: web::Query<NextTrackQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .next_track(query.seek, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for pausing playback.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
    /// Remote host to pause playback on
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/pause",
        description = "Pause the playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("host" = Option<String>, Query, description = "Remote host to pause the playback for"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/pause")]
pub async fn pause_playback_endpoint(
    query: web::Query<PauseQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .pause(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for resuming playback.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeQuery {
    /// Remote host to resume playback on
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/resume",
        description = "Resume the playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("host" = Option<String>, Query, description = "Remote host to resume the playback for"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/resume")]
pub async fn resume_playback_endpoint(
    query: web::Query<ResumeQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .resume(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for skipping to the previous track.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviousTrackQuery {
    /// Seek position in seconds for the previous track
    pub seek: Option<f64>,
    /// Remote host for playback
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        post,
        path = "/previous-track",
        description = "Skip to the previous track for the playback for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("seek" = Option<f64>, Query, description = "Position to seek the previous track on the playback to"),
            ("host" = Option<String>, Query, description = "Remote host to skip to the previous track on the playback for"),
        ),
        responses(
            (
                status = 200,
                description = "Success message",
                body = Value,
            )
        )
    )
)]
#[post("/previous-track")]
pub async fn previous_track_endpoint(
    query: web::Query<PreviousTrackQuery>,
) -> Result<Json<PlaybackStatus>> {
    get_player(query.host.as_deref())
        .await?
        .previous_track(query.seek, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;

    Ok(Json(PlaybackStatus { success: true }))
}

/// Query parameters for getting player status.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {
    /// Remote host to get status from
    pub host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Player"],
        get,
        path = "/status",
        description = "Get the playback status for the specified host",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("host" = Option<String>, Query, description = "Remote host to get playback status for"),
        ),
        responses(
            (
                status = 200,
                description = "Status for the playback",
                body = ApiPlaybackStatus,
            )
        )
    )
)]
#[get("/status")]
pub async fn player_status_endpoint(
    query: web::Query<PlayerStatusQuery>,
) -> Result<Json<ApiPlaybackStatus>> {
    Ok(Json(
        get_player(query.host.as_deref())
            .await?
            .player
            .player_status()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_player_error_to_actix_error_track_not_found() {
        let error = PlayerError::TrackNotFound(42.into());
        let actix_error: actix_web::Error = error.into();

        // Should be a 404 Not Found error
        assert_eq!(actix_error.as_response_error().status_code(), 404);
        assert!(actix_error.to_string().contains("Track not found"));
        assert!(actix_error.to_string().contains("42"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_track_not_local() {
        let error = PlayerError::TrackNotLocal(123.into());
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("not stored locally"));
        assert!(actix_error.to_string().contains("123"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_track_fetch_failed() {
        let error = PlayerError::TrackFetchFailed("remote-track-456".to_string());
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
        assert!(actix_error.to_string().contains("Failed to fetch track"));
        assert!(actix_error.to_string().contains("remote-track-456"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_album_fetch_failed() {
        let error = PlayerError::AlbumFetchFailed(789.into());
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
        assert!(actix_error.to_string().contains("Failed to fetch album"));
        assert!(actix_error.to_string().contains("789"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_no_players_playing() {
        let error = PlayerError::NoPlayersPlaying;
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_position_out_of_bounds() {
        let error = PlayerError::PositionOutOfBounds(99);
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("Position out of bounds"));
        assert!(actix_error.to_string().contains("99"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_playback_not_playing() {
        let error = PlayerError::PlaybackNotPlaying(12345);
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("Playback not playing"));
        assert!(actix_error.to_string().contains("12345"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_playback_already_playing() {
        let error = PlayerError::PlaybackAlreadyPlaying(67890);
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("Playback already playing"));
        assert!(actix_error.to_string().contains("67890"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_invalid_playback_type() {
        let error = PlayerError::InvalidPlaybackType;
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("Invalid Playback Type"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_unsupported_format() {
        let error = PlayerError::UnsupportedFormat(moosicbox_music_models::AudioFormat::Source);
        let actix_error: actix_web::Error = error.into();

        // Should be a 400 Bad Request error
        assert_eq!(actix_error.as_response_error().status_code(), 400);
        assert!(actix_error.to_string().contains("Unsupported format"));
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error = PlayerError::IO(io_error);
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_invalid_session() {
        let error = PlayerError::InvalidSession {
            session_id: 999,
            message: "test message".to_string(),
        };
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_seek_error() {
        let error = PlayerError::Seek("seek position invalid".to_string());
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_missing_session_id() {
        let error = PlayerError::MissingSessionId;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_missing_profile() {
        let error = PlayerError::MissingProfile;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_invalid_state() {
        let error = PlayerError::InvalidState;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_invalid_source() {
        let error = PlayerError::InvalidSource;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_cancelled() {
        let error = PlayerError::Cancelled;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_retry_requested() {
        let error = PlayerError::RetryRequested;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }

    #[test_log::test]
    fn test_player_error_to_actix_error_no_audio_outputs() {
        let error = PlayerError::NoAudioOutputs;
        let actix_error: actix_web::Error = error.into();

        // Should be a 500 Internal Server Error
        assert_eq!(actix_error.as_response_error().status_code(), 500);
    }
}
