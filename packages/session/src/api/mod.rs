use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_audio_zone::models::{ApiAudioZone, ApiPlayer};
use moosicbox_core::sqlite::models::ToApi as _;
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::models::{ApiSession, ApiSessionPlaylist, ApiSessionPlaylistTrack, RegisterPlayer};

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(session_playlist_endpoint)
        .service(session_playlist_tracks_endpoint)
        .service(session_audio_zone_endpoint)
        .service(session_playing_endpoint)
        .service(session_endpoint)
        .service(sessions_endpoint)
        .service(register_players_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Session")),
    paths(
        session_playlist_endpoint,
        session_playlist_tracks_endpoint,
        session_audio_zone_endpoint,
        session_playing_endpoint,
        session_endpoint,
        sessions_endpoint,
        register_players_endpoint,
    ),
    components(schemas(
        ApiSessionPlaylist,
        ApiSessionPlaylistTrack,
        ApiPlayer,
        ApiSession,
        RegisterPlayer,
    ))
)]
pub struct Api;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionPlaylistTracks {
    session_playlist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/session-playlist-tracks",
        description = "Get a list of tracks associated with a session playlist",
        params(
            ("sessionPlaylistId" = u64, Query, description = "Session playlist ID to fetch tracks for"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of tracks for the session playlist",
                body = Value,
            )
        )
    )
)]
#[route("/session-playlist-tracks", method = "GET")]
pub async fn session_playlist_tracks_endpoint(
    query: web::Query<GetSessionPlaylistTracks>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiSessionPlaylistTrack>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let outputs = crate::get_session_playlist_tracks(&**data.database, query.session_playlist_id)
        .await
        .map_err(ErrorInternalServerError)?;
    let total = outputs.len() as u32;
    let outputs = outputs
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.to_api())
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: outputs,
        offset,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionPlaylist {
    session_playlist_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/session-playlist",
        description = "Get a session playlist by ID",
        params(
            ("sessionPlaylistId" = u64, Query, description = "Session playlist ID to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "The session playlist, if it exists",
                body = Option<ApiSessionPlaylist>,
            )
        )
    )
)]
#[route("/session-playlist", method = "GET")]
pub async fn session_playlist_endpoint(
    query: web::Query<GetSessionPlaylist>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Option<ApiSessionPlaylist>>> {
    let playlist = crate::get_session_playlist(&**data.database, query.session_playlist_id)
        .await?
        .map(|x| x.to_api());

    Ok(Json(playlist))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionActivePlayers {
    session_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/session-audio-zone",
        description = "Get a session audio zone by session ID",
        params(
            ("sessionId" = u64, Query, description = "Session ID to fetch the audio zone for"),
        ),
        responses(
            (
                status = 200,
                description = "The session's active audio zone",
                body = Value,
            )
        )
    )
)]
#[route("/session-audio-zone", method = "GET")]
pub async fn session_audio_zone_endpoint(
    query: web::Query<GetSessionActivePlayers>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Option<ApiAudioZone>>> {
    let zone = crate::get_session_audio_zone(&**data.database, query.session_id)
        .await?
        .map(|x| x.into());

    Ok(Json(zone))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionPlaying {
    session_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/session-playing",
        description = "Get if the session is currently playing",
        params(
            ("sessionId" = u64, Query, description = "Session ID to fetch active players for"),
        ),
        responses(
            (
                status = 200,
                description = "Whether the session is playing or not",
                body = Option<bool>,
            )
        )
    )
)]
#[route("/session-playing", method = "GET")]
pub async fn session_playing_endpoint(
    query: web::Query<GetSessionPlaying>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Option<bool>>> {
    let playing = crate::get_session_playing(&**data.database, query.session_id).await?;

    Ok(Json(playing))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSession {
    session_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/session",
        description = "Get the session by ID",
        params(
            ("sessionId" = u64, Query, description = "Session ID to fetch"),
        ),
        responses(
            (
                status = 200,
                description = "The session",
                body = Option<ApiSession>,
            )
        )
    )
)]
#[route("/session", method = "GET")]
pub async fn session_endpoint(
    query: web::Query<GetSession>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Option<ApiSession>>> {
    let session = crate::get_session(&**data.database, query.session_id)
        .await?
        .map(|x| x.to_api());

    Ok(Json(session))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessions {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        get,
        path = "/sessions",
        description = "Get all sessions",
        params(
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "The session",
                body = Value,
            )
        )
    )
)]
#[route("/sessions", method = "GET")]
pub async fn sessions_endpoint(
    query: web::Query<GetSessions>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiSession>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let sessions = crate::get_sessions(&**data.database).await?;
    let total = sessions.len() as u32;
    let sessions = sessions
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.to_api())
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: sessions,
        offset,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayers {
    connection_id: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Session"],
        post,
        path = "/register-players",
        description = "Register the players to a connection",
        request_body = Vec<RegisterPlayer>,
        params(
            ("connectionId" = Option<u32>, Query, description = "The ID of the connection to register the players to"),
        ),
        responses(
            (
                status = 200,
                description = "The successfully registered players",
                body = Vec<ApiPlayer>,
            )
        )
    )
)]
#[route("/register-players", method = "POST")]
pub async fn register_players_endpoint(
    players: web::Json<Vec<RegisterPlayer>>,
    query: web::Query<RegisterPlayers>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Vec<ApiPlayer>>> {
    let registered = crate::create_players(&**data.database, &query.connection_id, &players)
        .await?
        .into_iter()
        .map(|x| x.to_api())
        .collect::<Vec<_>>();

    Ok(Json(registered))
}
