//! `YouTube` Music API client for `MoosicBox`.
//!
//! This crate provides a Rust client for interacting with the `YouTube` Music API,
//! enabling music streaming, library management, and playback features.
//!
//! # Features
//!
//! * OAuth device authorization flow for `YouTube` Music
//! * Artist, album, and track browsing and search
//! * Favorite/library management (add/remove artists, albums, tracks)
//! * Track streaming with configurable audio quality
//! * Integration with `MoosicBox`'s music API abstraction layer
//!
//! # Optional Features
//!
//! * `api` - Enables Actix-web HTTP API endpoints
//! * `db` - Enables database storage for `YouTube` Music credentials and configuration
//! * `scan` - Enables library scanning functionality
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "db")]
//! # {
//! use moosicbox_yt::YtMusicApi;
//! # use switchy_database::profiles::LibraryDatabase;
//!
//! # async fn example(db: LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! // Create a YouTube Music API client
//! let api = YtMusicApi::builder()
//!     .with_db(db)
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! # }

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    fmt::Display,
    str::FromStr as _,
    sync::{Arc, LazyLock},
};

use async_recursion::async_recursion;
use async_trait::async_trait;
use models::{YtAlbum, YtArtist, YtSearchResults, YtTrack};
use moosicbox_files::get_content_length;
use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType, database::AsModelResult as _, serde_json::ToValue,
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    MusicApi, TrackOrId,
    auth::ApiAuth,
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
        search::api::ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, AlbumSort, AlbumType, ApiSource, Artist, AudioFormat, PlaybackQuality, Track, id::Id,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::Mutex;
use url::form_urlencoded;

#[cfg(feature = "db")]
use switchy_database::DatabaseError;
#[cfg(feature = "db")]
use switchy_database::profiles::LibraryDatabase;

/// Actix-web HTTP API endpoints for `YouTube` Music operations.
///
/// Provides REST endpoints for device authorization, track streaming, library management,
/// and search functionality.
#[cfg(feature = "api")]
pub mod api;

/// Database operations for `YouTube` Music configuration and credentials.
///
/// Handles persistent storage of OAuth tokens and `YouTube` Music user configuration.
#[cfg(feature = "db")]
pub mod db;

/// Data models for `YouTube` Music entities.
///
/// Contains types for artists, albums, tracks, search results, and playback information.
pub mod models;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[cfg(feature = "db")]
    #[error(transparent)]
    YtConfig(#[from] db::GetYtConfigError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Request failed (error {0})")]
    RequestFailed(String),
    #[error("Request failed (error {0}): {1}")]
    HttpRequestFailed(u16, String),
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
    #[error("No response body")]
    NoResponseBody,
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Empty response")]
    EmptyResponse,
    #[error(transparent)]
    Config(#[from] YtConfigError),
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtDeviceType {
    Browser,
}

trait ToUrl {
    fn to_url(&self) -> String;
}

enum YtApiEndpoint {
    DeviceAuthorization,
    AuthorizationToken,
    Artist,
    FavoriteArtists,
    AddFavoriteArtist,
    RemoveFavoriteArtist,
    Album,
    FavoriteAlbums,
    AddFavoriteAlbum,
    RemoveFavoriteAlbum,
    ArtistAlbums,
    Track,
    FavoriteTracks,
    AddFavoriteTrack,
    RemoveFavoriteTrack,
    AlbumTracks,
    TrackUrl,
    TrackPlaybackInfo,
    Search,
}

static CLIENT: LazyLock<switchy_http::Client> =
    LazyLock::new(|| switchy_http::Client::builder().build().unwrap());

static YT_API_BASE_URL: &str = "https://music.youtube.com/youtubei/v1";

pub static API_SOURCE: LazyLock<ApiSource> =
    LazyLock::new(|| ApiSource::register("Yt", "YouTube Music"));

impl ToUrl for YtApiEndpoint {
    fn to_url(&self) -> String {
        #[allow(clippy::match_same_arms)]
        match self {
            Self::DeviceAuthorization => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::AuthorizationToken => format!("{YT_API_BASE_URL}/"),
            Self::Artist => format!("{YT_API_BASE_URL}/"),
            Self::FavoriteArtists => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::AddFavoriteArtist => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::RemoveFavoriteArtist => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::Album => format!("{YT_API_BASE_URL}/"),
            Self::FavoriteAlbums => format!("{YT_API_BASE_URL}/"),
            Self::AddFavoriteAlbum => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::RemoveFavoriteAlbum => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::ArtistAlbums => format!("{YT_API_BASE_URL}/"),
            Self::Track => format!("{YT_API_BASE_URL}/"),
            Self::FavoriteTracks => format!("{YT_API_BASE_URL}/"),
            Self::AddFavoriteTrack => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::RemoveFavoriteTrack => {
                format!("{YT_API_BASE_URL}/")
            }
            Self::AlbumTracks => format!("{YT_API_BASE_URL}/"),
            Self::TrackUrl => format!("{YT_API_BASE_URL}/"),
            Self::TrackPlaybackInfo => format!("{YT_API_BASE_URL}/"),
            Self::Search => format!("{YT_API_BASE_URL}/music/get_search_suggestions"),
        }
    }
}

fn replace_all(value: &str, params: &[(&str, &str)]) -> String {
    let mut string = value.to_string();

    for (key, value) in params {
        string = string.replace(key, value);
    }

    string
}

fn attach_query_string(value: &str, query: &[(&str, &str)]) -> String {
    let mut query_string = form_urlencoded::Serializer::new(String::new());

    for (key, value) in query {
        query_string.append_pair(key, value);
    }

    format!("{}?{}", value, &query_string.finish())
}

#[macro_export]
macro_rules! yt_api_endpoint {
    ($name:ident $(,)?) => {
        YtApiEndpoint::$name.to_url()
    };

    ($name:ident, $params:expr) => {
        replace_all(&yt_api_endpoint!($name), $params)
    };

    ($name:ident, $params:expr, $query:expr) => {
        attach_query_string(&yt_api_endpoint!($name, $params), $query)
    };
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
pub async fn device_authorization(client_id: String, open: bool) -> Result<Value, Error> {
    let url = yt_api_endpoint!(DeviceAuthorization);

    let params = serde_json::json!({
        "client_id": &client_id,
        "scope": "r_usr w_usr w_sub",
    });

    let value: Value = CLIENT.post(&url).form(&params).send().await?.json().await?;

    let verification_uri_complete = value.to_value::<&str>("verificationUriComplete")?;
    let device_code = value.to_value::<&str>("deviceCode")?;

    let url = format!("https://{verification_uri_complete}");

    if open {
        match open::that(&url) {
            Ok(()) => {
                log::debug!("Opened url in default browser");
            }
            Err(err) => {
                log::error!("Failed to open url in default web browser: {err:?}");
            }
        }
    }

    Ok(serde_json::json!({
        "url": url,
        "device_code": device_code,
    }))
}

#[allow(unused)]
async fn request(url: &str) -> Result<Value, Error> {
    request_inner(Method::Get, url, None, None, 1)
        .await?
        .ok_or_else(|| Error::NoResponseBody)
}

async fn post_request(
    url: &str,
    body: Option<Value>,
    form: Option<Value>,
) -> Result<Option<Value>, Error> {
    request_inner(Method::Post, url, body, form, 1).await
}

#[allow(unused)]
async fn delete_request(url: &str) -> Result<Option<Value>, Error> {
    request_inner(Method::Delete, url, None, None, 1).await
}

#[async_recursion]
async fn request_inner(
    method: Method,
    url: &str,
    body: Option<Value>,
    form: Option<Value>,
    attempt: u8,
) -> Result<Option<Value>, Error> {
    if attempt > 3 {
        log::error!("Max failed attempts for request reached");
        return Err(Error::MaxFailedAttempts);
    }

    log::debug!("Making request to {url}");

    let mut request = match method {
        Method::Get => CLIENT.get(url),
        Method::Post => CLIENT.post(url),
        Method::Delete => CLIENT.delete(url),
    };

    if let Some(form) = &form {
        request = request.form(form);
    }
    if let Some(body) = &body {
        request = request.json(body);
    }

    log::debug!("Sending authenticated {method} request to {url}");
    let response = request.send().await?;

    let status: u16 = response.status().into();

    log::debug!("Received authenticated request response status: {status}");

    match status {
        401 => {
            log::debug!("Received unauthorized response");
            Err(Error::Unauthorized)
        }
        400..=599 => Err(Error::HttpRequestFailed(
            status,
            response.text().await.unwrap_or_default(),
        )),
        _ => match response.json::<Value>().await {
            Ok(value) => Ok(Some(value)),
            Err(switchy_http::Error::Decode) => Ok(None),
            Err(e) => Err(Error::Http(e)),
        },
    }
}

/// # Panics
///
/// * If failed to serialize user `Value` to string
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn device_authorization_token(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, Error> {
    let url = yt_api_endpoint!(AuthorizationToken);

    let params = serde_json::json!({
        "client_id": &client_id,
        "client_secret": &client_secret,
        "device_code": &device_code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "scope": "r_usr w_usr w_sub",
    });

    let value: Value = CLIENT.post(&url).form(&params).send().await?.json().await?;

    log::trace!("Received value {value:?}");

    let access_token = value.to_value::<&str>("access_token")?;
    let refresh_token = value.to_value::<&str>("refresh_token")?;

    #[cfg(feature = "db")]
    if persist.unwrap_or(false) {
        let client_name = value.to_value("clientName")?;
        let expires_in = value.to_value("expires_in")?;
        let scope = value.to_value("scope")?;
        let token_type = value.to_value("token_type")?;
        let user = serde_json::to_string(value.to_value::<&Value>("user")?).unwrap();
        let user_id = value.to_value("user_id")?;

        db::create_yt_config(
            db,
            &client_id,
            access_token,
            refresh_token,
            client_name,
            expires_in,
            scope,
            token_type,
            &user,
            user_id,
        )
        .await?;
    }

    Ok(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
    }))
}

struct YtCredentials {
    access_token: String,
    client_id: Option<String>,
    refresh_token: Option<String>,
    #[cfg(feature = "db")]
    persist: bool,
}

#[allow(clippy::unused_async)]
async fn fetch_credentials(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    access_token: Option<String>,
) -> Result<YtCredentials, Error> {
    #[cfg(feature = "db")]
    {
        Ok(if let Some(access_token) = access_token {
            log::debug!("Using passed access_token");
            Some(Ok(YtCredentials {
                access_token,
                client_id: None,
                refresh_token: None,
                persist: false,
            }))
        } else {
            log::debug!("Fetching db Yt config");

            match db::get_yt_config(db).await {
                Ok(Some(config)) => {
                    log::debug!("Using db Yt config");
                    Some(Ok(YtCredentials {
                        access_token: config.access_token,
                        client_id: Some(config.client_id),
                        refresh_token: Some(config.refresh_token),
                        persist: true,
                    }))
                }
                Ok(None) => {
                    log::debug!("No Yt config available");
                    None
                }
                Err(err) => {
                    log::error!("Failed to get Yt config: {err:?}");
                    Some(Err(err))
                }
            }
        }
        .ok_or(Error::NoAccessTokenAvailable)??)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(YtCredentials {
            access_token: access_token.ok_or(Error::NoAccessTokenAvailable)?,
            client_id: None,
            refresh_token: None,
        })
    }
}

async fn authenticated_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    access_token: Option<String>,
) -> Result<Value, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Get,
        url,
        access_token,
        None,
        None,
        1,
    )
    .await?
    .ok_or_else(|| Error::NoResponseBody)
}

async fn authenticated_post_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Value>,
) -> Result<Option<Value>, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Post,
        url,
        access_token,
        body,
        form,
        1,
    )
    .await
}

async fn authenticated_delete_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    access_token: Option<String>,
) -> Result<Option<Value>, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Delete,
        url,
        access_token,
        None,
        None,
        1,
    )
    .await
}

#[derive(Copy, Debug, EnumString, AsRefStr, PartialEq, Eq, Clone)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
enum Method {
    Get,
    Post,
    Delete,
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[async_recursion]
async fn authenticated_request_inner(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    method: Method,
    url: &str,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Value>,
    attempt: u8,
) -> Result<Option<Value>, Error> {
    if attempt > 3 {
        log::error!("Max failed attempts for reauthentication reached");
        return Err(Error::MaxFailedAttempts);
    }

    log::debug!("Making authenticated request to {url}");

    let credentials = fetch_credentials(
        #[cfg(feature = "db")]
        db,
        access_token,
    )
    .await?;

    let mut request = match method {
        Method::Get => CLIENT.get(url),
        Method::Post => CLIENT.post(url),
        Method::Delete => CLIENT.delete(url),
    }
    .header(
        switchy_http::Header::Authorization.as_ref(),
        &format!("Bearer {}", credentials.access_token),
    );

    if let Some(form) = &form {
        request = request.form(form);
    }
    if let Some(body) = &body {
        request = request.json(body);
    }

    log::debug!("Sending authenticated {method} request to {url}");
    let response = request.send().await?;

    let status: u16 = response.status().into();

    log::debug!("Received authenticated request response status: {status}");

    match status {
        401 => {
            log::debug!("Received unauthorized response");
            if let (Some(client_id), Some(refresh_token)) =
                (&credentials.client_id, &credentials.refresh_token)
            {
                return authenticated_request_inner(
                    #[cfg(feature = "db")]
                    db,
                    method,
                    url,
                    Some(
                        refetch_access_token(
                            #[cfg(feature = "db")]
                            db,
                            client_id,
                            refresh_token,
                            #[cfg(feature = "db")]
                            credentials.persist,
                        )
                        .await?,
                    ),
                    body,
                    form,
                    attempt + 1,
                )
                .await;
            }

            log::debug!("No client_id or refresh_token available. Unauthorized");
            Err(Error::Unauthorized)
        }
        400..=599 => Err(Error::HttpRequestFailed(
            status,
            response.text().await.unwrap_or_default(),
        )),
        _ => match response.json::<Value>().await {
            Ok(value) => Ok(Some(value)),
            Err(switchy_http::Error::Decode) => Ok(None),
            Err(e) => Err(Error::Http(e)),
        },
    }
}

async fn refetch_access_token(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    client_id: &str,
    refresh_token: &str,
    #[cfg(feature = "db")] persist: bool,
) -> Result<String, Error> {
    log::debug!("Refetching access token");
    let url = yt_api_endpoint!(AuthorizationToken);

    let params = serde_json::json!({
        "client_id": &client_id,
        "refresh_token": &refresh_token,
        "grant_type": "refresh_token",
        "scope": "r_usr w_usr w_sub",
    });

    let value: Value = CLIENT.post(&url).form(&params).send().await?.json().await?;

    let access_token = value.to_value::<&str>("access_token")?;

    #[cfg(feature = "db")]
    if persist {
        let client_name = value.to_value("clientName")?;
        let expires_in = value.to_value("expires_in")?;
        let scope = value.to_value("scope")?;
        let token_type = value.to_value("token_type")?;
        let user = serde_json::to_string(value.to_value::<&Value>("user")?).unwrap();
        let user_id = value.to_value("user_id")?;

        db::create_yt_config(
            db,
            client_id,
            access_token,
            refresh_token,
            client_name,
            expires_in,
            scope,
            token_type,
            &user,
            user_id,
        )
        .await?;
    }

    Ok(access_token.to_string())
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtArtistOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtArtistOrderDirection {
    Asc,
    Desc,
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtArtistOrder>,
    order_direction: Option<YtArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtArtist, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        FavoriteArtists,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(YtArtistOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(YtArtistOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite artists response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| Error::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::RequestFailed(format!("{e:?}: {value:?}")))?;

    let total = value.to_value("totalNumberOfItems")?;

    #[cfg(feature = "db")]
    let db = db.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let country_code = country_code.clone();
            let locale = locale.clone();
            let access_token = access_token.clone();

            Box::pin(async move {
                favorite_artists(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    order,
                    order_direction,
                    country_code,
                    locale,
                    device_type,
                    access_token,
                    Some(user_id),
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteArtist,
        &[(":userId", &user_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "artistId": artist_id })),
    )
    .await?;

    log::trace!("Received add favorite artist response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        RemoveFavoriteArtist,
        &[
            (":userId", &user_id.to_string()),
            (":artistId", &artist_id.to_string())
        ],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_delete_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite artist response: {value:?}");

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtAlbumOrder {
    Date,
}

impl From<AlbumSort> for YtAlbumOrder {
    fn from(_value: AlbumSort) -> Self {
        Self::Date
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtAlbumOrderDirection {
    Asc,
    Desc,
}

impl From<AlbumSort> for YtAlbumOrderDirection {
    fn from(value: AlbumSort) -> Self {
        match value {
            AlbumSort::ArtistAsc
            | AlbumSort::NameAsc
            | AlbumSort::ReleaseDateAsc
            | AlbumSort::DateAddedAsc => Self::Asc,
            AlbumSort::ArtistDesc
            | AlbumSort::NameDesc
            | AlbumSort::ReleaseDateDesc
            | AlbumSort::DateAddedDesc => Self::Desc,
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtAlbumOrder>,
    order_direction: Option<YtAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtAlbum, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        FavoriteAlbums,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(YtAlbumOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(YtAlbumOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite albums response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| Error::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::RequestFailed(format!("{e:?}: {value:?}")))?;

    let total = value.to_value("totalNumberOfItems")?;

    #[cfg(feature = "db")]
    let db = db.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let country_code = country_code.clone();
            let locale = locale.clone();
            let access_token = access_token.clone();

            Box::pin(async move {
                favorite_albums(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    order,
                    order_direction,
                    country_code,
                    locale,
                    device_type,
                    access_token,
                    Some(user_id),
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn all_favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    order: Option<YtAlbumOrder>,
    order_direction: Option<YtAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<Vec<YtAlbum>, Error> {
    let mut all_albums = vec![];

    let mut offset = 0;
    let limit = 100;

    loop {
        let albums = favorite_albums(
            #[cfg(feature = "db")]
            db,
            Some(offset),
            Some(limit),
            order,
            order_direction,
            country_code.clone(),
            locale.clone(),
            device_type,
            access_token.clone(),
            user_id,
        )
        .await?;

        all_albums.extend_from_slice(&albums);

        if albums.is_empty() || all_albums.len() == usize::from(albums.has_more()) {
            break;
        }

        offset += limit;
    }

    Ok(all_albums)
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteAlbum,
        &[(":userId", &user_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "albumId": album_id })),
    )
    .await?;

    log::trace!("Received add favorite album response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        RemoveFavoriteAlbum,
        &[
            (":userId", &user_id.to_string()),
            (":albumId", &album_id.to_string())
        ],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_delete_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite album response: {value:?}");

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtTrackOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtTrackOrderDirection {
    Asc,
    Desc,
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<YtTrackOrder>,
    order_direction: Option<YtTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        FavoriteTracks,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(YtTrackOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(YtTrackOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite tracks response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| Error::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::RequestFailed(format!("{e:?}: {value:?}")))?;

    let total = value.to_value("totalNumberOfItems")?;

    #[cfg(feature = "db")]
    let db = db.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let country_code = country_code.clone();
            let locale = locale.clone();
            let access_token = access_token.clone();

            Box::pin(async move {
                favorite_tracks(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    order,
                    order_direction,
                    country_code,
                    locale,
                    device_type,
                    access_token,
                    Some(user_id),
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteTrack,
        &[(":userId", &user_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "trackId": track_id })),
    )
    .await?;

    log::trace!("Received add favorite track response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        RemoveFavoriteTrack,
        &[
            (":userId", &user_id.to_string()),
            (":trackId", &track_id.to_string())
        ],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_delete_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite track response: {value:?}");

    Ok(())
}

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone, PartialEq, Eq,
)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum YtAlbumType {
    #[default]
    Lp,
    EpsAndSingles,
    Compilations,
}

impl From<YtAlbumType> for AlbumType {
    fn from(value: YtAlbumType) -> Self {
        match value {
            YtAlbumType::Lp => Self::Lp,
            YtAlbumType::Compilations => Self::Compilations,
            YtAlbumType::EpsAndSingles => Self::EpsAndSingles,
        }
    }
}

impl MissingValue<YtAlbumType> for &Value {}
impl ToValueType<YtAlbumType> for &Value {
    fn to_value_type(self) -> Result<YtAlbumType, ParseError> {
        YtAlbumType::from_str(self.as_str().ok_or_else(|| {
            ParseError::MissingValue(format!(
                "YtAlbumType: ({})",
                serde_json::to_string(self).unwrap_or_default()
            ))
        })?)
        .map_err(|e| ParseError::ConvertType(format!("YtAlbumType: {e:?}")))
    }
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<YtAlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> PagingResult<YtAlbum, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let mut query: Vec<(&str, String)> = vec![
        ("offset", offset.to_string()),
        ("limit", limit.to_string()),
        (
            "countryCode",
            country_code.clone().unwrap_or_else(|| "US".into()),
        ),
        ("locale", locale.clone().unwrap_or_else(|| "en_US".into())),
        (
            "deviceType",
            device_type
                .unwrap_or(YtDeviceType::Browser)
                .as_ref()
                .to_string(),
        ),
    ];

    if let Some(album_type) = album_type {
        match album_type {
            YtAlbumType::Lp => {}
            _ => {
                query.push(("filter", album_type.as_ref().to_string()));
            }
        }
    }

    let url = yt_api_endpoint!(
        ArtistAlbums,
        &[(":artistId", &artist_id.to_string())],
        &query
            .iter()
            .map(|q| (q.0, q.1.as_str()))
            .collect::<Vec<_>>()
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| Error::RequestFailed(format!("{value:?}")))?;

    let total = value.to_value("totalNumberOfItems")?;

    #[cfg(feature = "db")]
    let db = db.clone();
    let artist_id = artist_id.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let country_code = country_code.clone();
            let artist_id = artist_id.clone();
            let locale = locale.clone();
            let access_token = access_token.clone();

            Box::pin(async move {
                artist_albums(
                    #[cfg(feature = "db")]
                    &db,
                    &artist_id,
                    Some(offset),
                    Some(limit),
                    album_type,
                    country_code,
                    locale,
                    device_type,
                    access_token,
                )
                .await
            })
        }))),
    })
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> PagingResult<YtTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = yt_api_endpoint!(
        AlbumTracks,
        &[(":albumId", &album_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received album tracks response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| Error::RequestFailed(format!("{value:?}")))?;

    let total = value.to_value("totalNumberOfItems")?;

    #[cfg(feature = "db")]
    let db = db.clone();
    let album_id = album_id.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let album_id = album_id.clone();
            let country_code = country_code.clone();
            let locale = locale.clone();
            let access_token = access_token.clone();

            Box::pin(async move {
                album_tracks(
                    #[cfg(feature = "db")]
                    &db,
                    &album_id,
                    Some(offset),
                    Some(limit),
                    country_code,
                    locale,
                    device_type,
                    access_token,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtAlbum, Error> {
    let url = yt_api_endpoint!(
        Album,
        &[(":albumId", &album_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    Ok(value.as_model()?)
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtArtist, Error> {
    let url = yt_api_endpoint!(
        Artist,
        &[(":artistId", &artist_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received artist response: {value:?}");

    Ok(value.as_model()?)
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtTrack, Error> {
    let url = yt_api_endpoint!(
        Track,
        &[(":trackId", &track_id.to_string())],
        &[
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received track response: {value:?}");

    Ok(value.as_model()?)
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn search(
    query: &str,
    _offset: Option<u32>,
    _limit: Option<u32>,
) -> Result<YtSearchResults, Error> {
    let url = yt_api_endpoint!(Search, &[], &[("prettyPrint", &false.to_string()),]);

    let date = switchy_time::datetime_local_now();

    let value = post_request(
        &url,
        Some(serde_json::json!({
            "input": query,
            "context": {
                "client": {
                    "hl": "en",
                    "gl": "US",
                    "clientName": "WEB_REMIX",
                    "clientVersion": format!("1.{}.00.01", date.format("%Y%m%d"))
                }
            }
        })),
        None,
    )
    .await?
    .ok_or(Error::EmptyResponse)?;

    log::trace!("Received search response: {value:?}");

    Ok(value.as_model()?)
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum YtAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

impl From<TrackAudioQuality> for YtAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => Self::High,
            TrackAudioQuality::FlacLossless => Self::Lossless,
            TrackAudioQuality::FlacHiRes | TrackAudioQuality::FlacHighestRes => Self::HiResLossless,
        }
    }
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn track_file_url(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    audio_quality: YtAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<Vec<String>, Error> {
    let url = yt_api_endpoint!(
        TrackUrl,
        &[(":trackId", &track_id.to_string())],
        &[
            ("audioquality", audio_quality.as_ref()),
            ("urlusagemode", "STREAM"),
            ("assetpresentation", "FULL")
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received track file url response: {value:?}");

    Ok(value.to_value("urls")?)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct YtTrackPlaybackInfo {
    pub album_peak_amplitude: f64,
    pub album_replay_gain: f64,
    pub asset_presentation: String,
    pub audio_mode: String,
    pub audio_quality: String,
    pub bit_depth: Option<u8>,
    pub manifest: String,
    pub manifest_hash: String,
    pub manifest_mime_type: String,
    pub sample_rate: Option<u32>,
    pub track_id: u64,
    pub track_peak_amplitude: f64,
    pub track_replay_gain: f64,
}

/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn track_playback_info(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    audio_quality: YtAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<YtTrackPlaybackInfo, Error> {
    let url = yt_api_endpoint!(
        TrackPlaybackInfo,
        &[(":trackId", &track_id.to_string())],
        &[
            ("audioquality", audio_quality.as_ref()),
            ("playbackmode", "STREAM"),
            ("assetpresentation", "FULL")
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received track playback info response: {value:?}");

    Ok(serde_json::from_value(value)?)
}

impl From<ArtistOrder> for YtArtistOrder {
    fn from(value: ArtistOrder) -> Self {
        match value {
            ArtistOrder::DateAdded => Self::Date,
        }
    }
}

impl From<ArtistOrderDirection> for YtArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => Self::Asc,
            ArtistOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<AlbumOrder> for YtAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => Self::Date,
        }
    }
}

impl From<AlbumOrderDirection> for YtAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => Self::Asc,
            AlbumOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<TrackOrder> for YtTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => Self::Date,
        }
    }
}

impl From<TrackOrderDirection> for YtTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => Self::Asc,
            TrackOrderDirection::Descending => Self::Desc,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Unsupported AlbumType")]
pub struct TryFromAlbumTypeError;

impl TryFrom<AlbumType> for YtAlbumType {
    type Error = TryFromAlbumTypeError;

    fn try_from(value: AlbumType) -> Result<Self, Self::Error> {
        match value {
            AlbumType::Lp => Ok(Self::Lp),
            AlbumType::Compilations => Ok(Self::Compilations),
            AlbumType::EpsAndSingles => Ok(Self::EpsAndSingles),
            _ => Err(TryFromAlbumTypeError),
        }
    }
}

impl From<Error> for moosicbox_music_api::Error {
    fn from(err: Error) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<TryFromAlbumTypeError> for moosicbox_music_api::Error {
    fn from(err: TryFromAlbumTypeError) -> Self {
        Self::Other(Box::new(err))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum YtConfigError {
    #[cfg(feature = "db")]
    #[error("Missing Db")]
    MissingDb,
    #[cfg(feature = "db")]
    #[error(transparent)]
    GetYtConfig(#[from] crate::db::GetYtConfigError),
}

#[derive(Default)]
pub struct YtMusicApiBuilder {
    #[cfg(feature = "db")]
    db: Option<LibraryDatabase>,
}

impl YtMusicApiBuilder {
    #[cfg(feature = "db")]
    #[must_use]
    pub fn with_db(mut self, db: LibraryDatabase) -> Self {
        self.db = Some(db);
        self
    }

    #[cfg(feature = "db")]
    pub fn db(&mut self, db: LibraryDatabase) -> &mut Self {
        self.db = Some(db);
        self
    }

    /// # Errors
    ///
    /// * If the `db` is missing
    #[allow(clippy::unused_async)]
    pub async fn build(self) -> Result<YtMusicApi, YtConfigError> {
        #[cfg(feature = "db")]
        let db = self.db.ok_or(YtConfigError::MissingDb)?;

        #[cfg(not(feature = "db"))]
        let logged_in = false;
        #[cfg(feature = "db")]
        let logged_in = crate::db::get_yt_config(&db)
            .await
            .is_ok_and(|x| x.is_some());

        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(logged_in)
            .build();

        Ok(YtMusicApi {
            #[cfg(feature = "db")]
            db,
            auth,
        })
    }
}

pub struct YtMusicApi {
    #[cfg(feature = "db")]
    db: LibraryDatabase,
    auth: ApiAuth,
}

impl YtMusicApi {
    #[must_use]
    pub fn builder() -> YtMusicApiBuilder {
        YtMusicApiBuilder::default()
    }
}

#[async_trait]
impl MusicApi for YtMusicApi {
    fn source(&self) -> &ApiSource {
        &API_SOURCE
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, moosicbox_music_api::Error> {
        Ok(favorite_artists(
            #[cfg(feature = "db")]
            &self.db,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .inner_into())
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, moosicbox_music_api::Error> {
        Ok(
            match artist(
                #[cfg(feature = "db")]
                &self.db,
                artist_id,
                None,
                None,
                None,
                None,
            )
            .await
            {
                Ok(artist) => Some(artist.into()),
                Err(e) => {
                    if let Error::HttpRequestFailed(status, _) = &e
                        && *status == 404
                    {
                        return Ok(None);
                    }

                    return Err(e.into());
                }
            },
        )
    }

    async fn add_artist(&self, artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(add_favorite_artist(
            #[cfg(feature = "db")]
            &self.db,
            artist_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(remove_favorite_artist(
            #[cfg(feature = "db")]
            &self.db,
            artist_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
        Ok(favorite_albums(
            #[cfg(feature = "db")]
            &self.db,
            request.page.as_ref().map(|x| x.offset),
            request.page.as_ref().map(|x| x.limit),
            request.sort.as_ref().map(|x| (*x).into()),
            request.sort.as_ref().map(|x| (*x).into()),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?)
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, moosicbox_music_api::Error> {
        Ok(Some(
            album(
                #[cfg(feature = "db")]
                &self.db,
                album_id,
                None,
                None,
                None,
                None,
            )
            .await?
            .try_into()
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?,
        ))
    }

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);

        if limit == 0 || offset > 0 {
            return Ok(PagingResponse::empty());
        }

        let tracks = album_tracks(
            #[cfg(feature = "db")]
            &self.db,
            album_id,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .with_rest_of_items_in_batches()
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

        let items = vec![AlbumVersion {
            tracks,
            format: Some(AudioFormat::Flac),
            bit_depth: None,
            sample_rate: None,
            channels: Some(2),
            source: API_SOURCE.clone().into(),
        }];

        Ok(PagingResponse::new(
            Page::WithTotal {
                items,
                offset,
                limit,
                total: 1,
            },
            |_, _| Box::pin(async move { Ok(PagingResponse::empty()) }),
        ))
    }

    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        Ok(if let Some(album_type) = album_type {
            artist_albums(
                #[cfg(feature = "db")]
                &self.db,
                artist_id,
                Some(offset),
                Some(limit),
                Some(album_type.try_into()?),
                None,
                None,
                None,
                None,
            )
            .await?
            .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        } else {
            let pages = futures::future::join_all(
                vec![
                    YtAlbumType::Lp,
                    YtAlbumType::EpsAndSingles,
                    YtAlbumType::Compilations,
                ]
                .into_iter()
                .map(|album_type| {
                    artist_albums(
                        #[cfg(feature = "db")]
                        &self.db,
                        artist_id,
                        Some(offset),
                        Some(limit),
                        Some(album_type),
                        None,
                        None,
                        None,
                        None,
                    )
                }),
            )
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

            let total = pages.iter().map(|page| page.total().unwrap()).sum();

            #[cfg(feature = "db")]
            let db = self.db.clone();
            let artist_id = artist_id.clone();

            PagingResponse {
                page: Page::WithTotal {
                    items: pages
                        .into_iter()
                        .flat_map(moosicbox_paging::PagingResponse::into_items)
                        .collect::<Vec<_>>(),
                    offset,
                    limit,
                    total,
                },
                fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
                    #[cfg(feature = "db")]
                    let db = db.clone();
                    let artist_id = artist_id.clone();

                    Box::pin(async move {
                        artist_albums(
                            #[cfg(feature = "db")]
                            &db,
                            &artist_id,
                            Some(offset),
                            Some(limit),
                            None,
                            None,
                            None,
                            None,
                            None,
                        )
                        .await
                    })
                }))),
            }
            .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        })
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(add_favorite_album(
            #[cfg(feature = "db")]
            &self.db,
            album_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_album(&self, album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(remove_favorite_album(
            #[cfg(feature = "db")]
            &self.db,
            album_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        moosicbox_assert::assert_or_unimplemented!(
            track_ids.is_none(),
            "Fetching specific tracks by id is not implemented yet"
        );

        Ok(favorite_tracks(
            #[cfg(feature = "db")]
            &self.db,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .inner_into())
    }

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        Ok(album_tracks(
            #[cfg(feature = "db")]
            &self.db,
            album_id,
            offset,
            limit,
            None,
            None,
            None,
            None,
        )
        .await?
        .inner_into())
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, moosicbox_music_api::Error> {
        Ok(Some(
            track(
                #[cfg(feature = "db")]
                &self.db,
                track_id,
                None,
                None,
                None,
                None,
            )
            .await?
            .into(),
        ))
    }

    async fn add_track(&self, track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(add_favorite_track(
            #[cfg(feature = "db")]
            &self.db,
            track_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_track(&self, track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(remove_favorite_track(
            #[cfg(feature = "db")]
            &self.db,
            track_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn track_source(
        &self,
        track: TrackOrId,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, moosicbox_music_api::Error> {
        let url = track_file_url(
            #[cfg(feature = "db")]
            &self.db,
            quality.into(),
            track.id(),
            None,
        )
        .await?
        .first()
        .map(ToString::to_string);

        let Some(url) = url else {
            return Ok(None);
        };

        Ok(track
            .track(self)
            .await?
            .map(|track| TrackSource::RemoteUrl {
                url,
                format: track.format.unwrap_or(AudioFormat::Source),
                track_id: Some(track.id.clone()),
                source: track.track_source,
                headers: None,
            }))
    }

    async fn track_size(
        &self,
        track: TrackOrId,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, moosicbox_music_api::Error> {
        let Some(url) = track_file_url(
            #[cfg(feature = "db")]
            &self.db,
            YtAudioQuality::High,
            track.id(),
            None,
        )
        .await?
        .into_iter()
        .next() else {
            return Ok(None);
        };

        Ok(get_content_length(&url, None, None)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?)
    }

    fn supports_scan(&self) -> bool {
        cfg!(feature = "scan")
    }

    #[cfg(feature = "scan")]
    async fn enable_scan(&self) -> Result<(), moosicbox_music_api::Error> {
        moosicbox_music_api_helpers::scan::enable_scan(self, &self.db).await
    }

    #[cfg(feature = "scan")]
    async fn scan_enabled(&self) -> Result<bool, moosicbox_music_api::Error> {
        moosicbox_music_api_helpers::scan::scan_enabled(self, &self.db).await
    }

    #[cfg(feature = "scan")]
    async fn scan(&self) -> Result<(), moosicbox_music_api::Error> {
        moosicbox_music_api_helpers::scan::scan(self, &self.db).await
    }

    fn auth(&self) -> Option<&ApiAuth> {
        Some(&self.auth)
    }

    fn supports_search(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<ApiSearchResultsResponse, moosicbox_music_api::Error> {
        let results = search(query, offset, limit).await?;

        Ok(results.into())
    }
}
