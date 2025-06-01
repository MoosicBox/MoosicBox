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
        }
    }
}

#[cfg(feature = "local")]
static PLAYER_CACHE: std::sync::LazyLock<
    std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, PlaybackHandler>>>,
> = std::sync::LazyLock::new(|| {
    std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()))
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
                let receiver = local_player.receiver.clone();

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(output)
                    .with_receiver(receiver);

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
                let receiver = local_player.receiver.clone();

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(output)
                    .with_receiver(receiver);

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

/// # Errors
///
/// * If failed to get the tracks from the `MusicApi`
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayAlbumQuery {
    pub session_id: u64,
    pub album_id: String,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
    pub source: Option<ApiSource>,
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
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTrackQuery {
    pub session_id: u64,
    pub track_id: i32,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
    pub source: Option<ApiSource>,
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
            .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayTracksQuery {
    pub session_id: u64,
    pub track_ids: String,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub host: Option<String>,
    pub format: Option<AudioFormat>,
    pub source: Option<ApiSource>,
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
            .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopTrackQuery {
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SeekTrackQuery {
    pub seek: f64,
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
    pub session_id: Option<u64>,
    pub audio_zone_id: Option<u64>,
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
                    .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NextTrackQuery {
    pub seek: Option<f64>,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PauseQuery {
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeQuery {
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviousTrackQuery {
    pub seek: Option<f64>,
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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatusQuery {
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
