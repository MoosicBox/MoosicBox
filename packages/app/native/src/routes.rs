//! Route handlers for the native application.
//!
//! This module implements all HTTP-like route handlers for the native desktop application.
//! Routes handle navigation, settings management, music browsing, search, downloads, and
//! music API integration. Most routes communicate with a backend server via HTTP requests
//! and render UI updates using the global renderer.

use std::{collections::BTreeMap, num::ParseIntError, str::FromStr, sync::LazyLock};

use hyperchad::{
    renderer::{Content, View},
    router::{Container, RouteRequest},
    transformer::html::ParseError,
};
use moosicbox_app_models::{Connection, DownloadSettings, MusicApiSettings, ScanSettings};
use moosicbox_app_native_ui::{
    downloads::DownloadTab,
    search::results_content,
    settings::{AuthState, download_settings_content, scan_settings_content},
    state::State,
};
use moosicbox_app_state::AppStateError;
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_downloader::api::models::{ApiDownloadLocation, ApiDownloadTask};
use moosicbox_music_api::{SourceToMusicApi as _, profiles::PROFILES};
use moosicbox_music_api_api::models::{ApiMusicApi, AuthValues};
use moosicbox_music_api_models::search::api::ApiSearchResultsResponse;
use moosicbox_music_models::{
    API_SOURCES, AlbumSort, AlbumType, ApiSource, TrackApiSource, TryFromStringTrackApiSourceError,
    api::{ApiAlbum, ApiArtist},
};
use moosicbox_paging::Page;
use moosicbox_scan_models::api::ApiScanPath;
use moosicbox_session_models::ApiSession;
use serde::Deserialize;
use switchy::http::models::Method;

use crate::{PROFILE, RENDERER, STATE, convert_state};

static CLIENT: LazyLock<switchy::http::Client> =
    LazyLock::new(|| switchy::http::Client::builder().build().unwrap());

/// Errors that can occur during route handling.
#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Missing query param: '{0}'")]
    MissingQueryParam(&'static str),
    #[error("Missing connection")]
    MissingConnection,
    #[error("Unsupported method")]
    UnsupportedMethod,
    #[error("Failed to parse body")]
    ParseBody(#[from] hyperchad::router::ParseError),
    #[error(transparent)]
    StrumParse(#[from] strum::ParseError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Reqwest(#[from] switchy::http::Error),
    #[error("Route failed: {0:?}")]
    RouteFailed(Box<dyn std::error::Error>),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    #[error(transparent)]
    AppState(#[from] AppStateError),
    #[error(transparent)]
    TryFromStringTrackApiSource(#[from] TryFromStringTrackApiSourceError),
}

/// Parses a comma-separated string of track API sources.
///
/// # Errors
///
/// * [`RouteError::RouteFailed`] if any source string is invalid
fn parse_track_sources(value: &str) -> Result<Vec<TrackApiSource>, RouteError> {
    value
        .split(',')
        .filter(|x| !x.is_empty())
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, TryFromStringTrackApiSourceError>>()
        .map_err(|e| RouteError::RouteFailed(e.into()))
}

/// Handles the initial load of the albums list with pagination.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingQueryParam`] if required query parameters are missing
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseInt`] if numeric query parameters are invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn albums_list_start_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(limit) = req.query.get("limit") else {
        return Err(RouteError::MissingQueryParam("limit"));
    };
    let limit = limit.parse::<u32>()?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;
    let offset = if let Some(offset) = req.query.get("offset") {
        offset.parse::<u32>()?
    } else {
        0
    };
    let search = req.query.get("search").filter(|x| !x.is_empty());

    let filtered_sources = parse_track_sources(
        req.query
            .get("sources")
            .map(String::as_str)
            .unwrap_or_default(),
    )?;

    let sort = req
        .query
        .get("sort")
        .map(String::as_str)
        .map(FromStr::from_str)
        .and_then(Result::ok)
        .unwrap_or(AlbumSort::NameAsc);

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .get(&format!(
            "{host}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
            if filtered_sources.is_empty() {
                String::new()
            } else {
                format!(
                    "&sources={}",
                    filtered_sources
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            },
            search.map_or_else(String::new, |search| format!("&search={search}"))
        ))
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_start_route: albums={albums:?}");

    Ok(moosicbox_app_native_ui::albums::albums_list_start(
        &state,
        &albums,
        &filtered_sources,
        sort,
        size,
        search.map_or("", |search| search),
    )
    .into())
}

/// Handles loading additional pages of the albums list.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingQueryParam`] if required query parameters are missing
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseInt`] if numeric query parameters are invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn albums_list_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(offset) = req.query.get("offset") else {
        return Err(RouteError::MissingQueryParam("offset"));
    };
    let offset = offset.parse::<u32>()?;
    let Some(limit) = req.query.get("limit") else {
        return Err(RouteError::MissingQueryParam("limit"));
    };
    let limit = limit.parse::<u32>()?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;

    let search = req.query.get("search").filter(|x| !x.is_empty());

    let filtered_sources = parse_track_sources(
        req.query
            .get("sources")
            .map(String::as_str)
            .unwrap_or_default(),
    )?;

    let sort = req
        .query
        .get("sort")
        .map(String::as_str)
        .map(FromStr::from_str)
        .and_then(Result::ok)
        .unwrap_or(AlbumSort::NameAsc);

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .get(&format!(
            "{host}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
            if filtered_sources.is_empty() {
                String::new()
            } else {
                format!(
                    "&sources={}",
                    filtered_sources
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            },
            search.map_or_else(String::new, |search| format!("&search={search}"))
        ))
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_route: albums={albums:?}");

    Ok(moosicbox_app_native_ui::albums::albums_list(host, &albums, size).into())
}

/// Handles loading an artist's albums list filtered by album type.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingQueryParam`] if required query parameters are missing
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseInt`] if numeric query parameters are invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn artist_albums_list_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(artist_id) = req.query.get("artistId") else {
        return Err(RouteError::MissingQueryParam("artistId"));
    };
    let source: ApiSource = req
        .query
        .get("source")
        .map(TryFrom::try_from)
        .transpose()
        .unwrap()
        .ok_or(RouteError::MissingQueryParam("Missing source query param"))?;
    let album_type: AlbumType = req
        .query
        .get("albumType")
        .map(String::as_str)
        .map(TryFrom::try_from)
        .transpose()?
        .ok_or(RouteError::MissingQueryParam(
            "Missing albumType query param",
        ))?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let url = format!(
        "{host}/menu/albums?moosicboxProfile={PROFILE}&artistId={artist_id}&source={source}&albumType={album_type}",
    );
    let response = CLIENT.get(&url).send().await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_route: albums={albums:?}");

    Ok(
        moosicbox_app_native_ui::artists::albums_list(host, &albums, source, album_type, size)
            .into(),
    )
}

/// Handles loading the audio zones list with their associated sessions.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn audio_zones_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let url = format!("{host}/audio-zone/with-session?moosicboxProfile={PROFILE}",);
    let response = CLIENT.get(&url).send().await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let zones: Page<ApiAudioZoneWithSession> = response.json().await?;

    Ok(moosicbox_app_native_ui::audio_zones::audio_zones(&zones, &[]).into())
}

/// Handles loading the list of active playback sessions.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn playback_sessions_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let url = format!("{host}/session/sessions?moosicboxProfile={PROFILE}",);
    let response = CLIENT.get(&url).send().await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let sessions: Page<ApiSession> = response.json().await?;

    Ok(moosicbox_app_native_ui::playback_sessions::playback_sessions(host, &sessions).into())
}

/// Handles displaying either a specific album or the albums list.
///
/// If an `albumId` query parameter is provided, displays the album details page.
/// Otherwise, displays the paginated albums list.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::MusicApi`] if the music API source is invalid or unavailable
/// * [`RouteError::ParseInt`] if numeric query parameters are invalid
/// * [`RouteError::RouteFailed`] if the server returns an error response or album not found
pub async fn albums_route(req: RouteRequest) -> Result<Container, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    Ok(if let Some(album_id) = req.query.get("albumId") {
        let source: ApiSource = req
            .query
            .get("source")
            .map(TryFrom::try_from)
            .transpose()
            .unwrap()
            .unwrap_or_default();

        let version_source: Option<TrackApiSource> = req
            .query
            .get("versionSource")
            .map(TryFrom::try_from)
            .transpose()?;

        let sample_rate: Option<u32> = req
            .query
            .get("sampleRate")
            .map(|x| x.parse::<u32>())
            .transpose()?;

        let bit_depth: Option<u8> = req
            .query
            .get("bitDepth")
            .map(|x| x.parse::<u8>())
            .transpose()?;

        if req.query.get("full").map(String::as_str) == Some("true") {
            let state = convert_state(&STATE).await;
            let album_id = album_id.into();
            let api = PROFILES.get(PROFILE).unwrap().get(&source).ok_or_else(|| {
                RouteError::MusicApi(moosicbox_music_api::Error::MusicApiNotFound(source.clone()))
            })?;
            let album = api
                .album(&album_id)
                .await?
                .ok_or_else(|| {
                    RouteError::RouteFailed(format!("No album for album_id={album_id}").into())
                })?
                .into();

            log::debug!("album: {album:?}");

            let versions = api
                .album_versions(&album_id, None, None)
                .await?
                .map(Into::into);

            log::debug!("versions: {versions:?}");

            let container: Container = moosicbox_app_native_ui::albums::album_page_content(
                &state,
                &album,
                &versions,
                versions.iter().find(|v| {
                    version_source.as_ref().is_none_or(|x| &v.source == x)
                        && bit_depth.is_none_or(|x| v.bit_depth.is_some_and(|b| b == x))
                        && sample_rate.is_none_or(|x| v.sample_rate.is_some_and(|s| s == x))
                }),
            )
            .into();

            container
        } else {
            let container: Container = moosicbox_app_native_ui::albums::album(
                &convert_state(&STATE).await,
                album_id,
                Some(&source),
                version_source.as_ref(),
                sample_rate,
                bit_depth,
            )
            .into();

            container
        }
    } else {
        let filtered_sources = parse_track_sources(
            req.query
                .get("sources")
                .map(String::as_str)
                .unwrap_or_default(),
        )?;

        let sort = req
            .query
            .get("sort")
            .map(String::as_str)
            .map(FromStr::from_str)
            .and_then(Result::ok)
            .unwrap_or(AlbumSort::NameAsc);

        let search = req
            .query
            .get("search")
            .filter(|x| !x.is_empty())
            .map(String::as_str);

        moosicbox_app_native_ui::albums::albums(
            &convert_state(&STATE).await,
            &filtered_sources,
            sort,
            search,
        )
        .into()
    })
}

/// Handles displaying either a specific artist or the artists list.
///
/// If an `artistId` query parameter is provided, displays the artist details page.
/// Otherwise, displays the full artists list.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn artist_route(req: RouteRequest) -> Result<Container, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    Ok(if let Some(artist_id) = req.query.get("artistId") {
        let source: Option<ApiSource> = req
            .query
            .get("source")
            .map(TryFrom::try_from)
            .transpose()
            .unwrap();

        let response = CLIENT
            .get(&format!(
                "{host}/menu/artist?moosicboxProfile={PROFILE}&artistId={artist_id}{}",
                source.map_or_else(String::new, |x| format!("&source={x}")),
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            let message = format!("Error: {} {}", response.status(), response.text().await?);
            log::error!("{message}");
            return Err(RouteError::RouteFailed(message.into()));
        }

        let artist: ApiArtist = response.json().await?;

        log::debug!("artist: {artist:?}");

        let container: Container =
            moosicbox_app_native_ui::artists::artist(&convert_state(&STATE).await, &artist).into();

        container
    } else {
        let response = CLIENT
            .get(&format!(
                "{host}/menu/artists?moosicboxProfile={PROFILE}&offset=0&limit=2000",
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            let message = format!("Error: {} {}", response.status(), response.text().await?);
            log::error!("{message}");
            return Err(RouteError::RouteFailed(message.into()));
        }

        let artists: Vec<ApiArtist> = response.json().await?;

        log::trace!("artists: {artists:?}");

        moosicbox_app_native_ui::artists::artists(&convert_state(&STATE).await, &artists).into()
    })
}

/// Handles displaying the downloads page with current or historical download tasks.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseInt`] if numeric query parameters are invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn downloads_route(req: RouteRequest) -> Result<Container, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let offset = req
        .query
        .get("offset")
        .map(|x| x.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    let limit = req
        .query
        .get("limit")
        .map(|x| x.parse::<u32>())
        .transpose()?
        .unwrap_or(30);
    let active_tab = req
        .query
        .get("tab")
        .map(String::as_str)
        .map(DownloadTab::from_str)
        .transpose()?
        .unwrap_or(DownloadTab::Current);

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let tasks_response = match active_tab {
        DownloadTab::Current => {
            CLIENT
                .get(&format!(
                    "{host}/downloader/download-tasks?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}&state=PENDING,PAUSED,STARTED",
                ))
                .send()
                .await?
        }
        DownloadTab::History => {
            CLIENT
                .get(&format!(
                    "{host}/downloader/download-tasks?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}&state=CANCELLED,FINISHED,ERROR",
                ))
                .send()
                .await?
        }
    };

    if !tasks_response.status().is_success() {
        let message = format!(
            "Error: {} {}",
            tasks_response.status(),
            tasks_response.text().await?
        );
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let tasks: Page<ApiDownloadTask> = tasks_response.json().await?;

    log::trace!("downloads_route: active_tab={active_tab} tasks={tasks:?}");

    Ok(moosicbox_app_native_ui::downloads::downloads(&state, &tasks, active_tab).into())
}

/// Handles displaying the settings page with connections and configurations.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::AppState`] if fails to retrieve connections from the app state
pub async fn settings_route(req: RouteRequest) -> Result<Container, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;

    switchy::unsync::task::spawn({
        let state = state.clone();
        async move {
            let mut container = settings_music_api_settings_markup(&state).await.unwrap();
            container.str_id = Some("settings-music-api-settings-section".to_string());
            let renderer = RENDERER.get().unwrap();
            renderer
                .render(View::builder().with_fragment(container).build())
                .await?;

            Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
        }
    });

    let connections = STATE.get_connections().await?;
    let current_connection = STATE.get_current_connection().await?;
    let connection_name = STATE.get_connection_name().await?.unwrap_or_default();

    Ok(moosicbox_app_native_ui::settings::settings(
        &state,
        &connection_name,
        &connections,
        current_connection.as_ref(),
        &[],
    )
    .into())
}

#[derive(Deserialize)]
struct ConnectionName {
    name: String,
}

/// Handles updating the connection name.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::RouteFailed`] if fails to parse the form data
/// * [`RouteError::AppState`] if fails to update the connection name in the app state
pub async fn settings_connection_name_route(req: RouteRequest) -> Result<(), RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    log::debug!("settings_connection_name_route: req={req:?}");
    let ConnectionName { name } = req
        .parse_form::<ConnectionName>()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        .map_err(RouteError::RouteFailed)?;

    STATE.update_connection_name(name).await?;

    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ConnectionUpdate {
    name: String,
    api_url: String,
}

/// Handles managing server connections (delete or update).
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not DELETE or PATCH
/// * [`RouteError::MissingQueryParam`] if the `name` query parameter is missing
/// * [`RouteError::RouteFailed`] if fails to parse the form data (for PATCH)
/// * [`RouteError::AppState`] if fails to update connections in the app state
pub async fn settings_connections_route(req: RouteRequest) -> Result<View, RouteError> {
    match req.method {
        Method::Delete => {
            let Some(name) = req.query.get("name") else {
                return Err(RouteError::MissingQueryParam("name"));
            };

            let connections = STATE.delete_connection(name).await?;

            let current_connection = STATE.get_current_connection().await?;

            Ok(moosicbox_app_native_ui::settings::connections_content(
                &connections,
                current_connection.as_ref(),
            )
            .into())
        }
        Method::Patch => {
            let name = req
                .query
                .get("name")
                .ok_or_else(|| RouteError::MissingQueryParam("name"))?;

            let update = req
                .parse_form::<ConnectionUpdate>()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                .map_err(RouteError::RouteFailed)?;

            let connections = STATE
                .update_connection(
                    name,
                    Connection {
                        name: update.name,
                        api_url: update.api_url,
                    },
                )
                .await?;

            let current_connection = STATE.get_current_connection().await?;

            Ok(moosicbox_app_native_ui::settings::connections_content(
                &connections,
                current_connection.as_ref(),
            )
            .into())
        }
        Method::Get
        | Method::Post
        | Method::Put
        | Method::Head
        | Method::Options
        | Method::Trace
        | Method::Connect => Err(RouteError::UnsupportedMethod),
    }
}

/// Handles creating a new server connection with a default name.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::AppState`] if fails to add the connection to the app state
pub async fn settings_new_connection_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let connections = STATE.get_connections().await?;
    let mut name = "New connection".to_string();
    let mut i = 2;

    while connections.iter().any(|x| x.name == name) {
        name = format!("New connection {i}");
        i += 1;
    }

    let connections = STATE
        .add_connection(Connection {
            name,
            api_url: String::new(),
        })
        .await?;

    let current_connection = STATE.get_current_connection().await?;

    Ok(moosicbox_app_native_ui::settings::connections_content(
        &connections,
        current_connection.as_ref(),
    )
    .into())
}

/// Handles selecting a server connection as the current active connection.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingQueryParam`] if the `name` query parameter is missing
/// * [`RouteError::MissingConnection`] if no connection with the specified name exists
/// * [`RouteError::AppState`] if fails to set the current connection in the app state
pub async fn settings_select_connection_route(req: RouteRequest) -> Result<View, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(name) = req.query.get("name") else {
        return Err(RouteError::MissingQueryParam("name"));
    };

    let connections = STATE.get_connections().await?;

    let connection = connections
        .iter()
        .find(|x| &x.name == name)
        .cloned()
        .ok_or(RouteError::MissingConnection)?;

    STATE.set_current_connection(connection.clone()).await?;

    Ok(
        moosicbox_app_native_ui::settings::connections_content(&connections, Some(&connection))
            .into(),
    )
}

/// Handles loading the music API settings section.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
pub async fn settings_music_api_settings_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;

    Ok(Content::builder()
        .with_fragment(settings_music_api_settings_markup(&state).await?)
        .build())
}

async fn settings_music_api_settings_markup(state: &State) -> Result<Container, RouteError> {
    let mut music_api_settings: Vec<MusicApiSettings> = vec![];

    if let Some(connection) = &state.connection {
        let host = &connection.api_url;

        let music_apis: Page<ApiMusicApi> = CLIENT
            .get(&format!("{host}/music-api?moosicboxProfile={PROFILE}"))
            .send()
            .await
            .inspect(|x| {
                if !x.status().is_success() {
                    log::error!("Error fetching music_apis: status={}", x.status());
                }
            })?
            .json()
            .await
            .inspect_err(|e| log::error!("Error parsing music_apis response body: {e}"))
            .unwrap_or_else(|_| Page::empty());

        let music_apis = music_apis.into_items();

        music_api_settings.extend(music_apis.into_iter().map(Into::into));
    } else {
        log::debug!("No connection");
    }

    Ok(moosicbox_app_native_ui::settings::music_api_settings_section(&music_api_settings).into())
}

/// Handles triggering a music API scan operation.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingQueryParam`] if the `apiSource` query parameter is missing
/// * [`RouteError::RouteFailed`] if the API source is invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::AppState`] if fails to get the current connection
pub async fn music_api_scan_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(api_source) = req.query.get("apiSource") else {
        return Err(RouteError::MissingQueryParam("apiSource"));
    };
    let api_source = ApiSource::from_str(api_source)
        .inspect_err(|e| {
            moosicbox_assert::die_or_error!("Invalid apiSource: {e:?}");
        })
        .map_err(|e| RouteError::RouteFailed(e.into()))?;

    if let Some(connection) = &STATE.get_current_connection().await? {
        let host = &connection.api_url;

        let music_api: ApiMusicApi = CLIENT
            .post(&format!(
                "{host}/music-api/scan?apiSource={api_source}&moosicboxProfile={PROFILE}"
            ))
            .send()
            .await
            .inspect(|x| {
                if !x.status().is_success() {
                    log::error!("Error scanning music_api: status={}", x.status());
                }
            })?
            .json()
            .await?;

        let settings = music_api.into();

        return Ok(Content::builder()
            .with_primary(
                moosicbox_app_native_ui::settings::music_api_settings_content(
                    &settings,
                    AuthState::Initial,
                ),
            )
            .build());
    }

    Ok(Content::builder()
        .with_fragment(moosicbox_app_native_ui::settings::scan_error_message(
            &api_source,
            Some("Failed to scan"),
        ))
        .build())
}

/// Handles enabling scan origins for a music API.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingQueryParam`] if the `apiSource` query parameter is missing
/// * [`RouteError::RouteFailed`] if the API source is invalid
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::AppState`] if fails to get the current connection
pub async fn music_api_enable_scan_origin_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(api_source) = req.query.get("apiSource") else {
        return Err(RouteError::MissingQueryParam("apiSource"));
    };
    let api_source = ApiSource::from_str(api_source)
        .inspect_err(|e| {
            moosicbox_assert::die_or_error!("Invalid apiSource: {e:?}");
        })
        .map_err(|e| RouteError::RouteFailed(e.into()))?;

    if let Some(connection) = &STATE.get_current_connection().await? {
        let host = &connection.api_url;

        let music_api: ApiMusicApi = CLIENT
            .post(&format!(
                "{host}/music-api/scan-origins?moosicboxProfile={PROFILE}&apiSource={api_source}",
            ))
            .send()
            .await?
            .json()
            .await?;

        log::debug!("music_api_enable_scan_origin_route: music_api={music_api:?}");

        let settings = music_api.into();

        return Ok(Content::builder()
            .with_primary(
                moosicbox_app_native_ui::settings::music_api_settings_content(
                    &settings,
                    AuthState::Initial,
                ),
            )
            .build());
    }

    Ok(Content::builder()
        .with_fragment(moosicbox_app_native_ui::settings::scan_error_message(
            &api_source,
            Some("Failed to enable scan origin"),
        ))
        .build())
}

/// Handles music API authentication with credentials or device flow polling.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingQueryParam`] if the `apiSource` query parameter is missing
/// * [`RouteError::RouteFailed`] if the API source is invalid
/// * [`RouteError::ParseBody`] if fails to parse the authentication form data
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::AppState`] if fails to get the current connection
pub async fn music_api_auth_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let auth_values = req.parse_form::<AuthValues>()?;

    log::debug!("music_api_auth_route: auth_type={auth_values:#?}");
    let Some(api_source) = req.query.get("apiSource") else {
        return Err(RouteError::MissingQueryParam("apiSource"));
    };
    let api_source = ApiSource::from_str(api_source)
        .inspect_err(|e| {
            moosicbox_assert::die_or_error!("Invalid apiSource: {e:?}");
        })
        .map_err(|e| RouteError::RouteFailed(e.into()))?;

    if let Some(connection) = &STATE.get_current_connection().await? {
        let host = &connection.api_url;

        let music_api: ApiMusicApi = CLIENT
            .post(&format!(
                "{host}/music-api/auth?apiSource={api_source}&moosicboxProfile={PROFILE}"
            ))
            .form(&auth_values)
            .send()
            .await
            .inspect(|x| {
                if !x.status().is_success() {
                    log::error!("Error authenticating music_api: status={}", x.status());
                }
            })?
            .json()
            .await?;

        let settings = music_api.into();

        let auth_state = match auth_values {
            AuthValues::UsernamePassword { .. } => AuthState::Initial,
            AuthValues::Poll => AuthState::Polling,
        };

        return Ok(Content::builder()
            .with_primary(
                moosicbox_app_native_ui::settings::music_api_settings_content(
                    &settings, auth_state,
                ),
            )
            .build());
    }

    Ok(Content::builder()
        .with_fragment(moosicbox_app_native_ui::settings::auth_error_message(
            &api_source,
            Some("Failed to authenticate"),
        ))
        .build())
}

/// Request body for search operations.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearchRequest {
    /// The search query string.
    pub query: String,
}

/// Handles music search across all registered API sources.
///
/// Spawns concurrent search tasks for each music API source and updates the UI
/// as results arrive.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseBody`] if fails to parse the search form data
///
/// # Panics
///
/// * If the HTTP request to the server fails in a spawned task
/// * If fails to parse the server response in a spawned task
/// * If fails to render the search results in a spawned task
pub async fn search_route(req: RouteRequest) -> Result<(), RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let request = req.parse_form::<SearchRequest>()?;
    let query = &request.query;

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let api_sources = API_SOURCES
        .read()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    for api_source in api_sources {
        let host = host.clone();
        let query = query.clone();

        switchy::unsync::task::spawn(async move {
            let response = CLIENT
                .get(&format!(
                    "{host}/music-api/search?moosicboxProfile={PROFILE}&query={query}&apiSource={api_source}"
                ))
                .send()
                .await
                .unwrap();

            if !response.status().is_success() {
                let message = format!(
                    "Error: {} {}",
                    response.status(),
                    response.text().await.unwrap()
                );
                panic!("Route failed: {message}");
            }

            let results: BTreeMap<ApiSource, ApiSearchResultsResponse> =
                response.json().await.unwrap();
            let results = results.get(&api_source).unwrap().clone();
            let markup = results_content(&host, &api_source, &results.results);

            let renderer = RENDERER.get().unwrap();
            renderer
                .render(View::builder().with_fragment(markup).build())
                .await
                .unwrap();

            Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
        });
    }

    Ok(())
}

/// Handles initiating an album download.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::MissingQueryParam`] if required query parameters are missing
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::RouteFailed`] if the API source is invalid or download fails
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
pub async fn download(req: RouteRequest) -> Result<(), RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(source) = req.query.get("source") else {
        return Err(RouteError::MissingQueryParam("source"));
    };
    let source = ApiSource::from_str(source)
        .inspect_err(|e| {
            moosicbox_assert::die_or_error!("Invalid source: {e:?}");
        })
        .map_err(|e| RouteError::RouteFailed(e.into()))?;

    let Some(album_id) = req.query.get("albumId") else {
        return Err(RouteError::MissingQueryParam("albumId"));
    };

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .post(&format!("{host}/downloader/download"))
        .header("moosicbox-profile", PROFILE)
        .query_param("albumId", album_id)
        .query_param("source", source.as_ref())
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    Ok(())
}

/// Handles adding or removing an album from the library.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST or DELETE
/// * [`RouteError::MissingQueryParam`] if required query parameters are missing
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::RouteFailed`] if the API source is invalid or operation fails
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::Parse`] if fails to create the success response view
pub async fn library_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Post | Method::Delete) {
        return Err(RouteError::UnsupportedMethod);
    }

    let Some(source) = req.query.get("source") else {
        return Err(RouteError::MissingQueryParam("source"));
    };
    let source = ApiSource::from_str(source)
        .inspect_err(|e| {
            moosicbox_assert::die_or_error!("Invalid source: {e:?}");
        })
        .map_err(|e| RouteError::RouteFailed(e.into()))?;

    let Some(album_id) = req.query.get("albumId") else {
        return Err(RouteError::MissingQueryParam("albumId"));
    };

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .request(req.method, &format!("{host}/menu/album"))
        .header("moosicbox-profile", PROFILE)
        .query_param("albumId", album_id)
        .query_param("source", source.as_ref())
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    Ok(Content::try_view("Success!")?)
}

/// Handles loading the download settings section.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn settings_download_settings_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .get(&format!("{host}/downloader/download-locations"))
        .header("moosicbox-profile", PROFILE)
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let locations: Page<ApiDownloadLocation> = response.json().await?;

    let locations = locations.into_items();
    let locations = locations
        .into_iter()
        .map(|x| (x.id, x.path))
        .collect::<Vec<_>>();

    let settings = DownloadSettings {
        download_locations: locations,
        default_download_location: STATE.get_default_download_location(),
    };

    let markup = download_settings_content(&settings);

    Ok(Content::builder().with_fragment(markup).build())
}

/// Request body for download location operations.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadsDownloadLocationRequest {
    /// The download location path.
    pub location: String,
}

/// Handles deleting a download location.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not DELETE
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseBody`] if fails to parse the form data
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
/// * [`RouteError::Parse`] if fails to create the success response view
pub async fn settings_downloads_download_location_route(
    req: RouteRequest,
) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Delete) {
        return Err(RouteError::UnsupportedMethod);
    }

    let request = req.parse_form::<DownloadsDownloadLocationRequest>()?;
    let location = &request.location;

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .delete(&format!("{host}/downloader/download-locations"))
        .header("moosicbox-profile", PROFILE)
        .query_param("path", location)
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    Ok(Content::try_view("Success!")?)
}

/// Request body for setting the default download location.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadsDefaultDownloadLocationRequest {
    /// The default download location path.
    pub location: String,
}

/// Handles setting the default download location.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not POST
/// * [`RouteError::ParseBody`] if fails to parse the form data
/// * [`RouteError::AppState`] if fails to update the default download location
/// * [`RouteError::Parse`] if fails to create the success response view
pub async fn settings_downloads_default_download_location_route(
    req: RouteRequest,
) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Post) {
        return Err(RouteError::UnsupportedMethod);
    }

    let request = req.parse_form::<DownloadsDefaultDownloadLocationRequest>()?;
    let location = request.location;

    STATE.set_default_download_location(location).await?;

    Ok(Content::try_view("Success!")?)
}

/// Handles loading the scan settings section.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not GET
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
pub async fn settings_scan_settings_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .get(&format!("{host}/scan/scan-paths"))
        .header("moosicbox-profile", PROFILE)
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let paths: Vec<ApiScanPath> = response.json().await?;
    let paths = paths.into_iter().map(|x| x.path).collect::<Vec<_>>();

    let settings = ScanSettings { scan_paths: paths };

    let markup = scan_settings_content(&settings);

    Ok(Content::builder().with_fragment(markup).build())
}

/// Request body for scan path operations.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ScanScanPathRequest {
    /// The scan path to delete.
    pub path: String,
}

/// Handles deleting a scan path.
///
/// # Errors
///
/// * [`RouteError::UnsupportedMethod`] if the request method is not DELETE
/// * [`RouteError::MissingConnection`] if no server connection is configured
/// * [`RouteError::ParseBody`] if fails to parse the form data
/// * [`RouteError::Reqwest`] if the HTTP request to the server fails
/// * [`RouteError::RouteFailed`] if the server returns an error response
/// * [`RouteError::Parse`] if fails to create the success response view
pub async fn settings_scan_scan_path_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Delete) {
        return Err(RouteError::UnsupportedMethod);
    }

    let request = req.parse_form::<ScanScanPathRequest>()?;
    let path = &request.path;

    let state = convert_state(&STATE).await;
    let Some(connection) = &state.connection else {
        return Err(RouteError::MissingConnection);
    };
    let host = &connection.api_url;

    let response = CLIENT
        .delete(&format!("{host}/scan/scan-paths"))
        .header("moosicbox-profile", PROFILE)
        .query_param("path", path)
        .send()
        .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    Ok(Content::try_view("Success!")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_track_sources_empty_string() {
        let result = parse_track_sources("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Vec::<TrackApiSource>::new());
    }

    #[test]
    fn test_parse_track_sources_single_valid_source() {
        let result = parse_track_sources("LIBRARY");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], TrackApiSource::Library);
    }

    #[test]
    fn test_parse_track_sources_multiple_valid_sources() {
        let result = parse_track_sources("LIBRARY,TIDAL,QOBUZ");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0], TrackApiSource::Library);
        assert_eq!(sources[1], TrackApiSource::Tidal);
        assert_eq!(sources[2], TrackApiSource::Qobuz);
    }

    #[test]
    fn test_parse_track_sources_with_empty_segments() {
        // Test that empty segments between commas are filtered out
        let result = parse_track_sources("LIBRARY,,TIDAL");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0], TrackApiSource::Library);
        assert_eq!(sources[1], TrackApiSource::Tidal);
    }

    #[test]
    fn test_parse_track_sources_trailing_comma() {
        let result = parse_track_sources("LIBRARY,TIDAL,");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0], TrackApiSource::Library);
        assert_eq!(sources[1], TrackApiSource::Tidal);
    }

    #[test]
    fn test_parse_track_sources_invalid_source() {
        let result = parse_track_sources("LIBRARY,INVALID_SOURCE");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RouteError::RouteFailed(_)));
    }

    #[test]
    fn test_parse_track_sources_case_sensitive() {
        // Test that lowercase source names are invalid
        let result = parse_track_sources("library");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_track_sources_yt_source() {
        let result = parse_track_sources("YT");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], TrackApiSource::Yt);
    }

    #[test]
    fn test_parse_track_sources_all_supported_sources() {
        // Test parsing all supported sources together
        let result = parse_track_sources("LIBRARY,TIDAL,QOBUZ,YT");
        assert!(result.is_ok());
        let sources = result.unwrap();
        assert_eq!(sources.len(), 4);
    }

    #[test]
    fn test_parse_track_sources_whitespace_not_trimmed() {
        // Test that whitespace is not trimmed (should fail)
        let result = parse_track_sources(" LIBRARY ");
        assert!(result.is_err());
    }
}
