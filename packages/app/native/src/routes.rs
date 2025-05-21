use std::{num::ParseIntError, str::FromStr, sync::LazyLock};

use hyperchad::{
    renderer::View,
    router::{Container, RouteRequest},
    transformer::html::ParseError,
};
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_music_api::{
    AlbumError, MusicApisError, SourceToMusicApi as _, TracksError, profiles::PROFILES,
};
use moosicbox_music_models::{
    AlbumSort, AlbumType, ApiSource, TrackApiSource,
    api::{ApiAlbum, ApiArtist},
};
use moosicbox_paging::Page;
use moosicbox_session_models::ApiSession;

use crate::{PROFILE, STATE, convert_state};

static CLIENT: LazyLock<switchy_http::Client> =
    LazyLock::new(|| switchy_http::Client::builder().build().unwrap());

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Missing query param: '{0}'")]
    MissingQueryParam(&'static str),
    #[error("Failed to parse markup")]
    ParseMarkup,
    #[error(transparent)]
    StrumParse(#[from] strum::ParseError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Reqwest(#[from] switchy_http::Error),
    #[error("Route failed: {0:?}")]
    RouteFailed(Box<dyn std::error::Error>),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    MusicApis(#[from] MusicApisError),
    #[error(transparent)]
    Album(#[from] AlbumError),
    #[error(transparent)]
    Tracks(#[from] TracksError),
}

fn parse_track_sources(value: &str) -> Result<Vec<TrackApiSource>, RouteError> {
    value
        .split(',')
        .filter(|x| !x.is_empty())
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, strum::ParseError>>()
        .map_err(|e| RouteError::RouteFailed(e.into()))
}

pub async fn albums_list_start_route(req: RouteRequest) -> Result<View, RouteError> {
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

    let response = CLIENT
        .get(&format!(
            "{}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8016"),
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

    let response = CLIENT
        .get(&format!(
            "{}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8016"),
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

    moosicbox_app_native_ui::albums::albums_list(&albums, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

pub async fn artist_albums_list_route(req: RouteRequest) -> Result<View, RouteError> {
    let Some(artist_id) = req.query.get("artistId") else {
        return Err(RouteError::MissingQueryParam("artistId"));
    };
    let source: ApiSource = req
        .query
        .get("source")
        .map(TryFrom::try_from)
        .transpose()?
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
    let url = format!(
        "{}/menu/albums?moosicboxProfile={PROFILE}&artistId={artist_id}&source={source}&albumType={album_type}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8016")
    );
    let response = CLIENT.get(&url).send().await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_route: albums={albums:?}");

    moosicbox_app_native_ui::artists::albums_list(&albums, source, album_type, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

pub async fn audio_zones_route(_req: RouteRequest) -> Result<View, RouteError> {
    let url = format!(
        "{}/audio-zone/with-session?moosicboxProfile={PROFILE}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8016")
    );
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

pub async fn playback_sessions_route(_req: RouteRequest) -> Result<View, RouteError> {
    let url = format!(
        "{}/session/sessions?moosicboxProfile={PROFILE}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8016")
    );
    let response = CLIENT.get(&url).send().await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let sessions: Page<ApiSession> = response.json().await?;

    moosicbox_app_native_ui::playback_sessions::playback_sessions(&sessions)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

pub async fn albums_route(req: RouteRequest) -> Result<Container, RouteError> {
    Ok(if let Some(album_id) = req.query.get("albumId") {
        let source: ApiSource = req
            .query
            .get("source")
            .map(TryFrom::try_from)
            .transpose()?
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
            let api = PROFILES.get(PROFILE).unwrap().get(source)?;
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
                    version_source.is_none_or(|x| v.source == x)
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
                Some(source),
                version_source,
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
    Ok(if let Some(artist_id) = req.query.get("artistId") {
        let source: Option<ApiSource> =
            req.query.get("source").map(TryFrom::try_from).transpose()?;

        let response = CLIENT
            .get(&format!(
                "{}/menu/artist?moosicboxProfile={PROFILE}&artistId={artist_id}{}",
                std::env::var("MOOSICBOX_HOST")
                    .as_deref()
                    .unwrap_or("http://localhost:8016"),
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
                "{}/menu/artists?moosicboxProfile={PROFILE}&offset=0&limit=2000",
                std::env::var("MOOSICBOX_HOST")
                    .as_deref()
                    .unwrap_or("http://localhost:8016")
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
