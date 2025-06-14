use std::{collections::BTreeMap, num::ParseIntError, str::FromStr, sync::LazyLock};

use hyperchad::{
    renderer::{Content, PartialView, View},
    router::{Container, RouteRequest},
    transformer::html::ParseError,
};
use moosicbox_app_models::{Connection, DownloadSettings, MusicApiSettings, ScanSettings};
use moosicbox_app_native_ui::{
    downloads::DownloadTab,
    formatting::classify_name,
    search::{results_content, results_content_id},
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

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Missing query param: '{0}'")]
    MissingQueryParam(&'static str),
    #[error("Missing connection")]
    MissingConnection,
    #[error("Unsupported method")]
    UnsupportedMethod,
    #[error("Failed to parse markup")]
    ParseMarkup,
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

fn parse_track_sources(value: &str) -> Result<Vec<TrackApiSource>, RouteError> {
    value
        .split(',')
        .filter(|x| !x.is_empty())
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, TryFromStringTrackApiSourceError>>()
        .map_err(|e| RouteError::RouteFailed(e.into()))
}

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

    moosicbox_app_native_ui::albums::albums_list_start(
        &state,
        &albums,
        &filtered_sources,
        sort,
        size,
        search.map_or("", |search| search),
    )
    .into_string()
    .try_into()
    .map_err(|e| {
        moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
        RouteError::ParseMarkup
    })
}

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

    moosicbox_app_native_ui::albums::albums_list(host, &albums, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

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

    moosicbox_app_native_ui::artists::albums_list(host, &albums, source, album_type, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

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

    moosicbox_app_native_ui::audio_zones::audio_zones(&zones, &[])
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

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

    moosicbox_app_native_ui::playback_sessions::playback_sessions(host, &sessions)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

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
            .into_string()
            .try_into()?;

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
            .into_string()
            .try_into()?;

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

        moosicbox_app_native_ui::albums::albums(
            &convert_state(&STATE).await,
            &filtered_sources,
            sort,
        )
        .into_string()
        .try_into()?
    })
}

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
            moosicbox_app_native_ui::artists::artist(&convert_state(&STATE).await, &artist)
                .into_string()
                .try_into()?;

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

        moosicbox_app_native_ui::artists::artists(&convert_state(&STATE).await, &artists)
            .into_string()
            .try_into()?
    })
}

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

    moosicbox_app_native_ui::downloads::downloads(&state, &tasks, active_tab)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

pub async fn settings_route(req: RouteRequest) -> Result<Container, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;

    switchy::unsync::task::spawn({
        let state = state.clone();
        async move {
            let container = settings_music_api_settings_markup(&state).await.unwrap();
            let renderer = RENDERER.get().unwrap();
            renderer
                .render_partial(PartialView {
                    target: "settings-music-api-settings-section".to_string(),
                    container,
                })
                .await?;

            Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
        }
    });

    let connections = STATE.get_connections().await?;
    let current_connection = STATE.get_current_connection().await?;
    let connection_name = STATE.get_connection_name().await?.unwrap_or_default();

    moosicbox_app_native_ui::settings::settings(
        &state,
        &connection_name,
        &connections,
        current_connection.as_ref(),
        &[],
    )
    .into_string()
    .try_into()
    .map_err(|e| {
        moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
        RouteError::ParseMarkup
    })
}

#[derive(Deserialize)]
struct ConnectionName {
    name: String,
}

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

pub async fn settings_music_api_settings_route(req: RouteRequest) -> Result<Content, RouteError> {
    if !matches!(req.method, Method::Get) {
        return Err(RouteError::UnsupportedMethod);
    }

    let state = convert_state(&STATE).await;

    Ok(Content::partial_view(
        "settings-music-api-settings-section",
        settings_music_api_settings_markup(&state).await?,
    ))
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

        return Ok(Content::view(
            moosicbox_app_native_ui::settings::music_api_settings_content(
                &settings,
                AuthState::Initial,
            ),
        ));
    }

    Ok(Content::partial_view(
        format!("settings-scan-error-{}", classify_name(&api_source)),
        moosicbox_app_native_ui::settings::scan_error_message(&api_source, Some("Failed to scan")),
    ))
}

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

        return Ok(Content::view(
            moosicbox_app_native_ui::settings::music_api_settings_content(
                &settings,
                AuthState::Initial,
            ),
        ));
    }

    Ok(Content::partial_view(
        format!("settings-scan-error-{}", classify_name(&api_source)),
        moosicbox_app_native_ui::settings::scan_error_message(
            &api_source,
            Some("Failed to enable scan origin"),
        ),
    ))
}

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

        return Ok(Content::view(
            moosicbox_app_native_ui::settings::music_api_settings_content(&settings, auth_state),
        ));
    }

    Ok(Content::partial_view(
        format!("settings-auth-error-{}", classify_name(&api_source)),
        moosicbox_app_native_ui::settings::auth_error_message(
            &api_source,
            Some("Failed to authenticate"),
        ),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearchRequest {
    query: String,
}

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
                .render_partial(PartialView {
                    target: results_content_id(&api_source),
                    container: markup.into_string().try_into().unwrap(),
                })
                .await
                .unwrap();

            Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
        });
    }

    Ok(())
}

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

    Ok(Content::partial_view(
        "settings-download-settings-section",
        markup,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadsDownloadLocationRequest {
    location: String,
}

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

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadsDefaultDownloadLocationRequest {
    location: String,
}

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

    Ok(Content::partial_view(
        "settings-scan-settings-section",
        markup,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ScanScanPathRequest {
    path: String,
}

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
