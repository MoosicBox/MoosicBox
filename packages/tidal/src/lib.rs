//! Tidal music streaming service integration.
//!
//! This crate provides a Rust client for interacting with the Tidal music streaming API.
//! It implements the [`MusicApi`] trait to enable fetching artists, albums, tracks, and
//! search results from Tidal, as well as managing user favorites.
//!
//! # Features
//!
//! * OAuth 2.0 device authorization flow for authentication
//! * Fetch and manage favorite artists, albums, and tracks
//! * Search for music content across Tidal's catalog
//! * Retrieve track playback URLs and metadata
//! * Support for different audio quality levels (High, Lossless, Hi-Res Lossless)
//! * Optional database persistence for authentication tokens
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "db")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # use switchy::database::profiles::LibraryDatabase;
//! # let db: LibraryDatabase = panic!("This is example code");
//! use moosicbox_tidal::TidalMusicApi;
//!
//! let api = TidalMusicApi::builder()
//!     .with_db(db)
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
/// HTTP API endpoints for Tidal integration.
///
/// Provides Actix Web route handlers for device authorization, favorites management,
/// track retrieval, and search functionality.
pub mod api;
#[cfg(feature = "db")]
/// Database operations for persisting Tidal authentication credentials.
///
/// Handles storage and retrieval of OAuth tokens and user configuration.
pub mod db;

/// Tidal API data models and type conversions.
///
/// Contains structs representing Tidal artists, albums, tracks, and search results,
/// along with conversions to/from `MoosicBox` common types.
pub mod models;

use std::sync::{Arc, LazyLock};

use itertools::Itertools as _;
use models::{TidalAlbum, TidalArtist, TidalSearchResults, TidalTrack};
#[cfg(feature = "db")]
use switchy::database::{DatabaseError, profiles::LibraryDatabase};

use async_recursion::async_recursion;
use async_trait::async_trait;
use moosicbox_files::get_content_length;
use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType, database::AsModelResult as _, serde_json::ToValue,
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    MusicApi, TrackOrId,
    auth::{ApiAuth, poll::PollAuth},
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
        TrackSource, search::api::ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, AlbumSort, AlbumType, ApiSource, Artist, AudioFormat, PlaybackQuality, Track, id::Id,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use switchy::http::models::Method;
use switchy_async::sync::Mutex;
use url::form_urlencoded;

/// Errors that can occur when interacting with the Tidal API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No user ID is available for the authenticated session.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// Failed to parse JSON data from the API response.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// HTTP request error from the underlying HTTP client.
    #[error(transparent)]
    Http(#[from] switchy::http::Error),
    /// Database operation failed.
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to retrieve Tidal configuration from the database.
    #[cfg(feature = "db")]
    #[error(transparent)]
    TidalConfig(#[from] db::GetTidalConfigError),
    /// No access token is available for authentication.
    #[error("No access token available")]
    NoAccessTokenAvailable,
    /// Request was rejected due to invalid or missing authentication.
    #[error("Unauthorized")]
    Unauthorized,
    /// API request failed with an error message.
    #[error("Request failed (error {0})")]
    RequestFailed(String),
    /// HTTP request failed with a specific status code and message.
    #[error("Request failed (error {0}): {1}")]
    HttpRequestFailed(u16, String),
    /// Maximum number of retry attempts has been exceeded.
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
    /// API response did not include an expected body.
    #[error("No response body")]
    NoResponseBody,
    /// Failed to serialize or deserialize JSON data.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Device type for Tidal API requests.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalDeviceType {
    /// Browser-based device type for web applications.
    Browser,
}

trait ToUrl {
    fn to_url(&self) -> String;
}

enum TidalApiEndpoint {
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

static CLIENT: LazyLock<switchy::http::Client> =
    LazyLock::new(|| switchy::http::Client::builder().build().unwrap());

static TIDAL_AUTH_API_BASE_URL: &str = "https://auth.tidal.com/v1";
static TIDAL_API_BASE_URL: &str = "https://api.tidal.com/v1";

/// The API source identifier for Tidal.
pub static API_SOURCE: LazyLock<ApiSource> =
    LazyLock::new(|| ApiSource::register("Tidal", "Tidal"));

impl ToUrl for TidalApiEndpoint {
    fn to_url(&self) -> String {
        match self {
            Self::DeviceAuthorization => {
                format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/device_authorization")
            }
            Self::AuthorizationToken => format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/token"),
            Self::Artist => format!("{TIDAL_API_BASE_URL}/artists/:artistId"),
            Self::FavoriteArtists | Self::AddFavoriteArtist => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/artists")
            }
            Self::RemoveFavoriteArtist => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/artists/:artistId")
            }
            Self::Album => format!("{TIDAL_API_BASE_URL}/albums/:albumId"),
            Self::FavoriteAlbums => format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/albums"),
            Self::AddFavoriteAlbum => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/albums")
            }
            Self::RemoveFavoriteAlbum => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/albums/:albumId")
            }
            Self::ArtistAlbums => format!("{TIDAL_API_BASE_URL}/artists/:artistId/albums"),
            Self::Track => format!("{TIDAL_API_BASE_URL}/tracks/:trackId"),
            Self::FavoriteTracks => format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/tracks"),
            Self::AddFavoriteTrack => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/tracks")
            }
            Self::RemoveFavoriteTrack => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/tracks/:trackId")
            }
            Self::AlbumTracks => format!("{TIDAL_API_BASE_URL}/albums/:albumId/tracks"),
            Self::TrackUrl => format!("{TIDAL_API_BASE_URL}/tracks/:trackId/urlpostpaywall"),
            Self::TrackPlaybackInfo => format!("{TIDAL_API_BASE_URL}/tracks/:trackId/playbackinfo"),
            Self::Search => format!("{TIDAL_API_BASE_URL}/search/top-hits"),
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

/// Constructs Tidal API endpoint URLs with optional parameters and query strings.
///
/// # Examples
///
/// ```ignore
/// // Basic endpoint
/// let url = tidal_api_endpoint!(Album);
///
/// // With URL parameters
/// let url = tidal_api_endpoint!(Album, &[(":albumId", "123")]);
///
/// // With URL parameters and query string
/// let url = tidal_api_endpoint!(Album, &[(":albumId", "123")], &[("locale", "en_US")]);
/// ```
#[macro_export]
macro_rules! tidal_api_endpoint {
    ($name:ident $(,)?) => {
        TidalApiEndpoint::$name.to_url()
    };

    ($name:ident, $params:expr) => {
        replace_all(&tidal_api_endpoint!($name), $params)
    };

    ($name:ident, $params:expr, $query:expr) => {
        attach_query_string(&tidal_api_endpoint!($name, $params), $query)
    };
}

/// Initiates the OAuth 2.0 device authorization flow for Tidal.
///
/// Returns a JSON object containing the verification URL and device code that the user
/// needs to authorize the application. Optionally opens the verification URL in the
/// default browser if `open` is `true`.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
pub async fn device_authorization(client_id: String, open: bool) -> Result<Value, Error> {
    let url = tidal_api_endpoint!(DeviceAuthorization);

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
        "deviceCode": device_code,
    }))
}

/// Exchanges a device code for an access token in the OAuth 2.0 flow.
///
/// Polls the Tidal API to check if the user has authorized the device. If successful,
/// returns the access token and optionally persists it to the database if `persist` is `true`.
///
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
    let url = tidal_api_endpoint!(AuthorizationToken);

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

        db::create_tidal_config(
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
        "accessToken": access_token,
        "refreshToken": refresh_token,
    }))
}

struct TidalCredentials {
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
) -> Result<TidalCredentials, Error> {
    #[cfg(feature = "db")]
    {
        Ok(if let Some(access_token) = access_token {
            log::debug!("Using passed access_token");
            Some(Ok(TidalCredentials {
                access_token,
                client_id: None,
                refresh_token: None,
                persist: false,
            }))
        } else {
            log::debug!("Fetching db Tidal config");

            match db::get_tidal_config(db).await {
                Ok(Some(config)) => {
                    log::debug!("Using db Tidal config");
                    Some(Ok(TidalCredentials {
                        access_token: config.access_token,
                        client_id: Some(config.client_id),
                        refresh_token: Some(config.refresh_token),
                        persist: true,
                    }))
                }
                Ok(None) => {
                    log::debug!("No Tidal config available");
                    None
                }
                Err(err) => {
                    log::error!("Failed to get Tidal config: {err:?}");
                    Some(Err(err))
                }
            }
        }
        .ok_or(Error::NoAccessTokenAvailable)??)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(TidalCredentials {
            access_token: access_token.ok_or(Error::NoAccessTokenAvailable)?,
            client_id: None,
            refresh_token: None,
        })
    }
}

async fn validate_credentials(#[cfg(feature = "db")] db: &LibraryDatabase) -> Result<bool, Error> {
    if let Err(e) = favorite_albums(
        #[cfg(feature = "db")]
        db,
        Some(0),
        Some(1),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    {
        log::debug!("validate_credentials: error: {e:?}");
        match e {
            Error::Unauthorized => Ok(false),
            Error::NoUserIdAvailable
            | Error::Parse(..)
            | Error::Http(..)
            | Error::NoAccessTokenAvailable
            | Error::RequestFailed(..)
            | Error::HttpRequestFailed(..)
            | Error::MaxFailedAttempts
            | Error::NoResponseBody
            | Error::Serde(..) => Err(e),
            #[cfg(feature = "db")]
            Error::Database(_) | Error::TidalConfig(_) => Err(e),
        }
    } else {
        log::debug!("validate_credentials: success");
        Ok(true)
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

    let mut request = CLIENT.request(method, url).header(
        switchy::http::Header::Authorization.as_ref(),
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
            Err(switchy::http::Error::Decode) => Ok(None),
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
    let url = tidal_api_endpoint!(AuthorizationToken);

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

        db::create_tidal_config(
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

/// Order field for artist queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalArtistOrder {
    /// Order artists by date added to favorites.
    Date,
}

/// Sort direction for artist queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalArtistOrderDirection {
    /// Ascending order (oldest to newest).
    Asc,
    /// Descending order (newest to oldest).
    Desc,
}

/// Fetches the user's favorite artists from Tidal.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
/// * If no user ID is available
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalArtistOrder>,
    order_direction: Option<TidalArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalArtist, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteArtists,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(TidalArtistOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(TidalArtistOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Adds an artist to the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "artistId": &artist_id })),
    )
    .await?;

    log::trace!("Received add favorite artist response: {value:?}");

    Ok(())
}

/// Removes an artist from the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Order field for album queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalAlbumOrder {
    /// Order albums by date added to favorites.
    Date,
}

/// Sort direction for album queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalAlbumOrderDirection {
    /// Ascending order (oldest to newest).
    Asc,
    /// Descending order (newest to oldest).
    Desc,
}

impl From<AlbumSort> for TidalAlbumOrderDirection {
    fn from(value: AlbumSort) -> Self {
        match value {
            AlbumSort::ArtistAsc
            | AlbumSort::NameAsc
            | AlbumSort::ReleaseDateAsc
            | AlbumSort::DateAddedAsc => Self::Asc,
            AlbumSort::NameDesc
            | AlbumSort::ArtistDesc
            | AlbumSort::ReleaseDateDesc
            | AlbumSort::DateAddedDesc => Self::Desc,
        }
    }
}

/// Fetches the user's favorite albums from Tidal.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
/// * If no user ID is available
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalAlbum, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteAlbums,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(TidalAlbumOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(TidalAlbumOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Retrieves all favorite albums from Tidal by paginating through results.
///
/// Automatically handles pagination to fetch all albums, making multiple requests if necessary.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn all_favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<Vec<TidalAlbum>, Error> {
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

/// Adds an album to the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "albumId": &album_id })),
    )
    .await?;

    log::trace!("Received add favorite album response: {value:?}");

    Ok(())
}

/// Removes an album from the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Order field for track queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalTrackOrder {
    /// Order tracks by date added to favorites.
    Date,
}

/// Sort direction for track queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalTrackOrderDirection {
    /// Ascending order (oldest to newest).
    Asc,
    /// Descending order (newest to oldest).
    Desc,
}

/// Fetches the user's favorite tracks from Tidal.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
/// * If no user ID is available
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalTrackOrder>,
    order_direction: Option<TidalTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteTracks,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("order", order.unwrap_or(TidalTrackOrder::Date).as_ref()),
            (
                "orderDirection",
                order_direction
                    .unwrap_or(TidalTrackOrderDirection::Desc)
                    .as_ref(),
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Adds a track to the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_post_request(
        #[cfg(feature = "db")]
        db,
        &url,
        access_token,
        None,
        Some(serde_json::json!({ "trackId": &track_id })),
    )
    .await?;

    log::trace!("Received add favorite track response: {value:?}");

    Ok(())
}

/// Removes a track from the user's favorites on Tidal.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), Error> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(Error::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Album type classification in Tidal.
#[derive(Default, Debug, Serialize, Deserialize, AsRefStr, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum TidalAlbumType {
    /// Full-length studio album (LP).
    #[default]
    Lp,
    /// Extended plays (EPs) and single releases.
    EpsAndSingles,
    /// Compilation albums and collections.
    Compilations,
}

impl From<&str> for TidalAlbumType {
    fn from(value: &str) -> Self {
        match value {
            "EPSANDSINGLES" | "EP" | "SINGLE" => Self::EpsAndSingles,
            "COMPILATIONS" => Self::Compilations,
            _ => Self::Lp,
        }
    }
}

impl From<TidalAlbumType> for AlbumType {
    fn from(value: TidalAlbumType) -> Self {
        match value {
            TidalAlbumType::Lp => Self::Lp,
            TidalAlbumType::Compilations => Self::Compilations,
            TidalAlbumType::EpsAndSingles => Self::EpsAndSingles,
        }
    }
}

impl MissingValue<TidalAlbumType> for &Value {}
impl ToValueType<TidalAlbumType> for &Value {
    fn to_value_type(self) -> Result<TidalAlbumType, ParseError> {
        Ok(self
            .as_str()
            .ok_or_else(|| {
                ParseError::MissingValue(format!(
                    "TidalAlbumType: ({})",
                    serde_json::to_string(self).unwrap_or_default()
                ))
            })?
            .into())
    }
}

/// Fetches albums by an artist from Tidal.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<TidalAlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> PagingResult<TidalAlbum, Error> {
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
                .unwrap_or(TidalDeviceType::Browser)
                .as_ref()
                .to_string(),
        ),
    ];

    if let Some(album_type) = album_type {
        match album_type {
            TidalAlbumType::Lp => {}
            _ => {
                query.push(("filter", album_type.as_ref().to_string()));
            }
        }
    }

    let url = tidal_api_endpoint!(
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

/// Fetches tracks from an album on Tidal.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> PagingResult<TidalTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Retrieves album metadata from Tidal by album ID.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalAlbum, Error> {
    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Retrieves artist metadata from Tidal by artist ID.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalArtist, Error> {
    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Retrieves track metadata from Tidal by track ID.
///
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
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalTrack, Error> {
    let url = tidal_api_endpoint!(
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
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

/// Content types that can be searched on Tidal.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SearchType {
    /// Search for artists.
    Artists,
    /// Search for albums.
    Albums,
    /// Search for tracks.
    Tracks,
    /// Search for videos.
    Videos,
    /// Search for playlists.
    Playlists,
    /// Search for user profiles.
    UserProfiles,
}

impl From<SearchType> for TidalSearchType {
    fn from(value: SearchType) -> Self {
        match value {
            SearchType::Artists => Self::Artists,
            SearchType::Albums => Self::Albums,
            SearchType::Tracks => Self::Tracks,
            SearchType::Videos => Self::Videos,
            SearchType::Playlists => Self::Playlists,
            SearchType::UserProfiles => Self::UserProfiles,
        }
    }
}

/// Tidal-specific search type identifiers.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum TidalSearchType {
    /// Search for artists.
    Artists,
    /// Search for albums.
    Albums,
    /// Search for tracks.
    Tracks,
    /// Search for videos.
    Videos,
    /// Search for playlists.
    Playlists,
    /// Search for user profiles.
    UserProfiles,
}

/// Searches for content on Tidal by query string and content types.
///
/// Returns search results containing matching artists, albums, and tracks based on the
/// specified query and search types.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
#[allow(clippy::too_many_arguments)]
pub async fn search(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    query: &str,
    offset: Option<u32>,
    limit: Option<u32>,
    include_contributions: Option<bool>,
    include_did_you_mean: Option<bool>,
    include_user_playlists: Option<bool>,
    supports_user_data: Option<bool>,
    types: Option<Vec<TidalSearchType>>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalSearchResults, Error> {
    static DEFAULT_TYPES: [TidalSearchType; 3] = [
        TidalSearchType::Artists,
        TidalSearchType::Albums,
        TidalSearchType::Tracks,
    ];

    let url = tidal_api_endpoint!(
        Search,
        &[],
        &[
            ("query", query),
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(3).to_string()),
            (
                "includeContributions",
                &include_contributions.unwrap_or(false).to_string()
            ),
            (
                "includeDidYouMean",
                &include_did_you_mean.unwrap_or(false).to_string()
            ),
            (
                "includeUserPlaylists",
                &include_user_playlists.unwrap_or(false).to_string()
            ),
            (
                "supportsUserData",
                &supports_user_data.unwrap_or(false).to_string()
            ),
            (
                "types",
                &types
                    .unwrap_or_else(|| DEFAULT_TYPES.to_vec())
                    .iter()
                    .map(AsRef::as_ref)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (
                "countryCode",
                &country_code.clone().unwrap_or_else(|| "US".into())
            ),
            ("locale", &locale.clone().unwrap_or_else(|| "en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
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

    log::trace!("Received search response: {value:?}");

    Ok(value.as_model()?)
}

/// Audio quality levels supported by Tidal.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TidalAudioQuality {
    /// High quality audio (AAC 320kbps).
    High,
    /// Lossless audio quality (FLAC 16-bit/44.1kHz).
    Lossless,
    /// Hi-Res lossless audio (FLAC up to 24-bit/192kHz or MQA).
    HiResLossless,
}

impl From<TrackAudioQuality> for TidalAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => Self::High,
            TrackAudioQuality::FlacLossless => Self::Lossless,
            TrackAudioQuality::FlacHiRes | TrackAudioQuality::FlacHighestRes => Self::HiResLossless,
        }
    }
}

/// Retrieves the playback URLs for a track at the specified audio quality.
///
/// Returns a list of URLs that can be used to stream or download the track.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn track_file_url(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    audio_quality: TidalAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<Vec<String>, Error> {
    let url = tidal_api_endpoint!(
        TrackUrl,
        &[(":trackId", &track_id.to_string())],
        &[
            ("audioquality", audio_quality.as_ref()),
            ("urlusagemode", "STREAM"),
            ("assetpresentation", "`FULL`")
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

/// Playback information for a Tidal track including audio metadata.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TidalTrackPlaybackInfo {
    /// Album peak amplitude for normalization.
    pub album_peak_amplitude: f64,
    /// Album replay gain in dB.
    pub album_replay_gain: f64,
    /// Asset presentation format (e.g., "`FULL`").
    pub asset_presentation: String,
    /// Audio mode (e.g., "`STEREO`").
    pub audio_mode: String,
    /// Audio quality level (e.g., "`HI_RES_LOSSLESS`").
    pub audio_quality: String,
    /// Bit depth in bits (e.g., 16, 24).
    pub bit_depth: Option<u8>,
    /// Playback manifest data.
    pub manifest: String,
    /// Hash of the manifest for verification.
    pub manifest_hash: String,
    /// MIME type of the manifest (e.g., "application/dash+xml").
    pub manifest_mime_type: String,
    /// Sample rate in Hz (e.g., 44100, 96000).
    pub sample_rate: Option<u32>,
    /// Tidal track ID.
    pub track_id: u64,
    /// Track peak amplitude for normalization.
    pub track_peak_amplitude: f64,
    /// Track replay gain in dB.
    pub track_replay_gain: f64,
}

/// Retrieves detailed playback information for a track including audio metadata.
///
/// Returns information such as replay gain, bit depth, sample rate, and manifest data
/// needed for playback.
///
/// # Errors
///
/// * If the HTTP request failed
/// * If the JSON response failed to parse
/// * If a database error occurred
pub async fn track_playback_info(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    audio_quality: TidalAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<TidalTrackPlaybackInfo, Error> {
    let url = tidal_api_endpoint!(
        TrackPlaybackInfo,
        &[(":trackId", &track_id.to_string())],
        &[
            ("audioquality", audio_quality.as_ref()),
            ("playbackmode", "STREAM"),
            ("assetpresentation", "`FULL`")
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

impl From<ArtistOrder> for TidalArtistOrder {
    fn from(value: ArtistOrder) -> Self {
        match value {
            ArtistOrder::DateAdded => Self::Date,
        }
    }
}

impl From<ArtistOrderDirection> for TidalArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => Self::Asc,
            ArtistOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<AlbumSort> for TidalAlbumOrder {
    fn from(_value: AlbumSort) -> Self {
        Self::Date
    }
}

impl From<AlbumOrder> for TidalAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => Self::Date,
        }
    }
}

impl From<AlbumOrderDirection> for TidalAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => Self::Asc,
            AlbumOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<TrackOrder> for TidalTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => Self::Date,
        }
    }
}

impl From<TrackOrderDirection> for TidalTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => Self::Asc,
            TrackOrderDirection::Descending => Self::Desc,
        }
    }
}

/// Error returned when converting an unsupported album type to `TidalAlbumType`.
#[derive(Debug, thiserror::Error)]
#[error("Unsupported AlbumType")]
pub struct TryFromAlbumTypeError;

impl TryFrom<AlbumType> for TidalAlbumType {
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

/// Errors that can occur when configuring the Tidal API client.
#[derive(Debug, thiserror::Error)]
pub enum TidalConfigError {
    /// Database connection is required but was not provided.
    #[cfg(feature = "db")]
    #[error("Missing Db")]
    MissingDb,
    /// Failed to retrieve Tidal configuration from the database.
    #[cfg(feature = "db")]
    #[error(transparent)]
    GetTidalConfig(#[from] db::GetTidalConfigError),
}

/// Builder for configuring and constructing a [`TidalMusicApi`] instance.
#[derive(Default)]
pub struct TidalMusicApiBuilder {
    #[cfg(feature = "db")]
    db: Option<LibraryDatabase>,
}

impl TidalMusicApiBuilder {
    /// Sets the database connection (builder pattern).
    #[cfg(feature = "db")]
    #[must_use]
    pub fn with_db(mut self, db: LibraryDatabase) -> Self {
        self.db = Some(db);
        self
    }

    /// Sets the database connection (mutable reference pattern).
    #[cfg(feature = "db")]
    pub fn db(&mut self, db: LibraryDatabase) -> &mut Self {
        self.db = Some(db);
        self
    }

    /// Constructs a [`TidalMusicApi`] instance from the builder configuration.
    ///
    /// # Errors
    ///
    /// * If the `db` is missing
    #[allow(clippy::unused_async)]
    pub async fn build(self) -> Result<TidalMusicApi, TidalConfigError> {
        #[cfg(feature = "db")]
        let db = self.db.ok_or(TidalConfigError::MissingDb)?;

        #[cfg(not(feature = "db"))]
        let logged_in = false;
        #[cfg(feature = "db")]
        let logged_in = crate::db::get_tidal_config(&db)
            .await
            .is_ok_and(|x| x.is_some());

        let auth = ApiAuth::builder()
            .with_logged_in(logged_in)
            .with_auth(PollAuth::new())
            .with_validate_credentials({
                #[cfg(feature = "db")]
                let db = db.clone();
                move || {
                    #[cfg(feature = "db")]
                    let db = db.clone();
                    async move {
                        validate_credentials(
                            #[cfg(feature = "db")]
                            &db,
                        )
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
                    }
                }
            })
            .build();

        switchy::unsync::task::spawn({
            let auth = auth.clone();
            async move {
                if let Err(e) = auth.validate_credentials().await {
                    moosicbox_assert::die_or_error!("Failed to validate credentials: {e:?}");
                }
            }
        });

        Ok(TidalMusicApi {
            #[cfg(feature = "db")]
            db,
            auth,
        })
    }
}

/// Implementation of the [`MusicApi`] trait for Tidal streaming service.
pub struct TidalMusicApi {
    #[cfg(feature = "db")]
    db: LibraryDatabase,
    auth: ApiAuth,
}

impl TidalMusicApi {
    /// Creates a new builder for configuring a `TidalMusicApi` instance.
    #[must_use]
    pub fn builder() -> TidalMusicApiBuilder {
        TidalMusicApiBuilder::default()
    }
}

#[async_trait]
impl MusicApi for TidalMusicApi {
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

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        let artist = crate::artist(
            #[cfg(feature = "db")]
            &self.db,
            &artist.id,
            None,
            None,
            None,
            None,
        )
        .await?;

        Ok(artist
            .picture_url(size.into())
            .map(|url| ImageCoverSource::RemoteUrl { url, headers: None }))
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
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
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
                    TidalAlbumType::Lp,
                    TidalAlbumType::EpsAndSingles,
                    TidalAlbumType::Compilations,
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

    async fn album_cover_source(
        &self,
        album: &Album,
        size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        let album = crate::album(
            #[cfg(feature = "db")]
            &self.db,
            &album.id,
            None,
            None,
            None,
            None,
        )
        .await?;

        Ok(album
            .cover_url(size.into())
            .map(|url| ImageCoverSource::RemoteUrl { url, headers: None }))
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        let Some(track_ids) = track_ids else {
            return Ok(favorite_tracks(
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
            .inner_into());
        };

        let offset = offset.unwrap_or(0) as usize;
        let offset = if offset > track_ids.len() {
            track_ids.len()
        } else {
            offset
        };
        let limit = limit.unwrap_or(30) as usize;
        let limit = if limit > track_ids.len() {
            track_ids.len()
        } else {
            limit
        };

        let track_ids = &track_ids[offset..limit];
        let mut all_tracks = vec![];

        let chunks = track_ids
            .iter()
            .chunks(10)
            .into_iter()
            .map(|x| x.into_iter().collect_vec())
            .collect_vec();

        for chunk in chunks {
            let mut tracks = vec![];

            for track_id in chunk {
                tracks.push(track(
                    #[cfg(feature = "db")]
                    &self.db,
                    track_id,
                    None,
                    None,
                    None,
                    None,
                ));
            }

            let tracks = futures::future::join_all(tracks).await;
            let tracks = tracks.into_iter().collect::<Result<Vec<_>, _>>();
            let tracks = match tracks {
                Ok(tracks) => tracks,
                Err(e) => {
                    moosicbox_assert::die_or_err!(
                        moosicbox_music_api::Error::Other(Box::new(e)),
                        "Failed to fetch track: {e:?}",
                    );
                }
            };
            all_tracks.extend(tracks);
        }

        Ok(PagingResponse {
            page: Page::WithTotal {
                items: all_tracks.into_iter().map(Into::into).collect(),
                offset: u32::try_from(offset).unwrap(),
                limit: u32::try_from(limit).unwrap(),
                total: u32::try_from(track_ids.len()).unwrap(),
            },
            fetch: Arc::new(Mutex::new(Box::new(move |_offset, _count| {
                Box::pin(async move { unimplemented!("Fetching tracks is not implemented") })
            }))),
        })
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
            TidalAudioQuality::High,
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
        let results = search(
            #[cfg(feature = "db")]
            &self.db,
            query,
            offset,
            limit,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await?;

        Ok(results.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_replace_all_single_replacement() {
        let input = "https://api.tidal.com/v1/artists/:artistId";
        let result = replace_all(input, &[(":artistId", "12345")]);
        assert_eq!(result, "https://api.tidal.com/v1/artists/12345");
    }

    #[test_log::test]
    fn test_replace_all_multiple_replacements() {
        let input = "https://api.tidal.com/v1/users/:userId/favorites/artists/:artistId";
        let result = replace_all(input, &[(":userId", "999"), (":artistId", "12345")]);
        assert_eq!(
            result,
            "https://api.tidal.com/v1/users/999/favorites/artists/12345"
        );
    }

    #[test_log::test]
    fn test_replace_all_no_replacements() {
        let input = "https://api.tidal.com/v1/search";
        let result = replace_all(input, &[]);
        assert_eq!(result, "https://api.tidal.com/v1/search");
    }

    #[test_log::test]
    fn test_replace_all_pattern_not_found() {
        let input = "https://api.tidal.com/v1/search";
        let result = replace_all(input, &[(":artistId", "12345")]);
        assert_eq!(result, "https://api.tidal.com/v1/search");
    }

    #[test_log::test]
    fn test_attach_query_string_single_param() {
        let result = attach_query_string("https://api.tidal.com/v1/search", &[("query", "test")]);
        assert_eq!(result, "https://api.tidal.com/v1/search?query=test");
    }

    #[test_log::test]
    fn test_attach_query_string_multiple_params() {
        let result = attach_query_string(
            "https://api.tidal.com/v1/search",
            &[("query", "test"), ("limit", "10"), ("offset", "0")],
        );
        assert_eq!(
            result,
            "https://api.tidal.com/v1/search?query=test&limit=10&offset=0"
        );
    }

    #[test_log::test]
    fn test_attach_query_string_url_encoding() {
        let result = attach_query_string(
            "https://api.tidal.com/v1/search",
            &[("query", "test artist")],
        );
        assert_eq!(result, "https://api.tidal.com/v1/search?query=test+artist");
    }

    #[test_log::test]
    fn test_attach_query_string_special_characters() {
        let result = attach_query_string(
            "https://api.tidal.com/v1/search",
            &[("query", "test&special=chars")],
        );
        assert_eq!(
            result,
            "https://api.tidal.com/v1/search?query=test%26special%3Dchars"
        );
    }

    #[test_log::test]
    fn test_attach_query_string_empty_params() {
        let result = attach_query_string("https://api.tidal.com/v1/search", &[]);
        assert_eq!(result, "https://api.tidal.com/v1/search?");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_basic() {
        let url = tidal_api_endpoint!(Album);
        assert_eq!(url, "https://api.tidal.com/v1/albums/:albumId");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_with_params() {
        let url = tidal_api_endpoint!(Album, &[(":albumId", "123456")]);
        assert_eq!(url, "https://api.tidal.com/v1/albums/123456");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_with_query() {
        let url = tidal_api_endpoint!(Album, &[(":albumId", "123456")], &[("locale", "en_US")]);
        assert_eq!(url, "https://api.tidal.com/v1/albums/123456?locale=en_US");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_search() {
        let url = tidal_api_endpoint!(Search);
        assert_eq!(url, "https://api.tidal.com/v1/search/top-hits");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_favorite_artists() {
        let url = tidal_api_endpoint!(FavoriteArtists, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/artists");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_remove_favorite_artist() {
        let url = tidal_api_endpoint!(
            RemoveFavoriteArtist,
            &[(":userId", "999"), (":artistId", "12345")]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/users/999/favorites/artists/12345"
        );
    }

    #[test_log::test]
    fn test_album_type_from_tidal_album_type() {
        assert_eq!(AlbumType::from(TidalAlbumType::Lp), AlbumType::Lp);
        assert_eq!(
            AlbumType::from(TidalAlbumType::EpsAndSingles),
            AlbumType::EpsAndSingles
        );
        assert_eq!(
            AlbumType::from(TidalAlbumType::Compilations),
            AlbumType::Compilations
        );
    }

    #[test_log::test]
    fn test_search_type_from_tidal_search_type() {
        assert_eq!(
            TidalSearchType::from(SearchType::Artists),
            TidalSearchType::Artists
        );
        assert_eq!(
            TidalSearchType::from(SearchType::Albums),
            TidalSearchType::Albums
        );
        assert_eq!(
            TidalSearchType::from(SearchType::Tracks),
            TidalSearchType::Tracks
        );
        assert_eq!(
            TidalSearchType::from(SearchType::Videos),
            TidalSearchType::Videos
        );
        assert_eq!(
            TidalSearchType::from(SearchType::Playlists),
            TidalSearchType::Playlists
        );
        assert_eq!(
            TidalSearchType::from(SearchType::UserProfiles),
            TidalSearchType::UserProfiles
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_epsandsingles() {
        assert_eq!(
            TidalAlbumType::from("EPSANDSINGLES"),
            TidalAlbumType::EpsAndSingles
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_ep() {
        assert_eq!(TidalAlbumType::from("EP"), TidalAlbumType::EpsAndSingles);
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_single() {
        assert_eq!(
            TidalAlbumType::from("SINGLE"),
            TidalAlbumType::EpsAndSingles
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_compilations() {
        assert_eq!(
            TidalAlbumType::from("COMPILATIONS"),
            TidalAlbumType::Compilations
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_lp() {
        assert_eq!(TidalAlbumType::from("LP"), TidalAlbumType::Lp);
    }

    #[test_log::test]
    fn test_tidal_album_type_from_str_unknown_defaults_to_lp() {
        assert_eq!(TidalAlbumType::from("UNKNOWN"), TidalAlbumType::Lp);
        assert_eq!(TidalAlbumType::from("ALBUM"), TidalAlbumType::Lp);
        assert_eq!(TidalAlbumType::from(""), TidalAlbumType::Lp);
    }

    #[test_log::test]
    fn test_tidal_album_order_direction_from_album_sort_ascending() {
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::ArtistAsc),
            TidalAlbumOrderDirection::Asc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::NameAsc),
            TidalAlbumOrderDirection::Asc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::ReleaseDateAsc),
            TidalAlbumOrderDirection::Asc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::DateAddedAsc),
            TidalAlbumOrderDirection::Asc
        );
    }

    #[test_log::test]
    fn test_tidal_album_order_direction_from_album_sort_descending() {
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::NameDesc),
            TidalAlbumOrderDirection::Desc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::ArtistDesc),
            TidalAlbumOrderDirection::Desc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::ReleaseDateDesc),
            TidalAlbumOrderDirection::Desc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumSort::DateAddedDesc),
            TidalAlbumOrderDirection::Desc
        );
    }

    #[test_log::test]
    fn test_tidal_artist_order_from_artist_order() {
        assert_eq!(
            TidalArtistOrder::from(ArtistOrder::DateAdded),
            TidalArtistOrder::Date
        );
    }

    #[test_log::test]
    fn test_tidal_artist_order_direction_from_artist_order_direction() {
        assert_eq!(
            TidalArtistOrderDirection::from(ArtistOrderDirection::Ascending),
            TidalArtistOrderDirection::Asc
        );
        assert_eq!(
            TidalArtistOrderDirection::from(ArtistOrderDirection::Descending),
            TidalArtistOrderDirection::Desc
        );
    }

    #[test_log::test]
    fn test_tidal_album_order_from_album_sort() {
        assert_eq!(
            TidalAlbumOrder::from(AlbumSort::NameAsc),
            TidalAlbumOrder::Date
        );
        assert_eq!(
            TidalAlbumOrder::from(AlbumSort::DateAddedDesc),
            TidalAlbumOrder::Date
        );
    }

    #[test_log::test]
    fn test_tidal_album_order_from_album_order() {
        assert_eq!(
            TidalAlbumOrder::from(AlbumOrder::DateAdded),
            TidalAlbumOrder::Date
        );
    }

    #[test_log::test]
    fn test_tidal_album_order_direction_from_album_order_direction() {
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumOrderDirection::Ascending),
            TidalAlbumOrderDirection::Asc
        );
        assert_eq!(
            TidalAlbumOrderDirection::from(AlbumOrderDirection::Descending),
            TidalAlbumOrderDirection::Desc
        );
    }

    #[test_log::test]
    fn test_tidal_track_order_from_track_order() {
        assert_eq!(
            TidalTrackOrder::from(TrackOrder::DateAdded),
            TidalTrackOrder::Date
        );
    }

    #[test_log::test]
    fn test_tidal_track_order_direction_from_track_order_direction() {
        assert_eq!(
            TidalTrackOrderDirection::from(TrackOrderDirection::Ascending),
            TidalTrackOrderDirection::Asc
        );
        assert_eq!(
            TidalTrackOrderDirection::from(TrackOrderDirection::Descending),
            TidalTrackOrderDirection::Desc
        );
    }

    #[test_log::test]
    fn test_tidal_audio_quality_from_track_audio_quality() {
        assert_eq!(
            TidalAudioQuality::from(TrackAudioQuality::Low),
            TidalAudioQuality::High
        );
        assert_eq!(
            TidalAudioQuality::from(TrackAudioQuality::FlacLossless),
            TidalAudioQuality::Lossless
        );
        assert_eq!(
            TidalAudioQuality::from(TrackAudioQuality::FlacHiRes),
            TidalAudioQuality::HiResLossless
        );
        assert_eq!(
            TidalAudioQuality::from(TrackAudioQuality::FlacHighestRes),
            TidalAudioQuality::HiResLossless
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_try_from_album_type_success() {
        assert_eq!(
            TidalAlbumType::try_from(AlbumType::Lp).unwrap(),
            TidalAlbumType::Lp
        );
        assert_eq!(
            TidalAlbumType::try_from(AlbumType::Compilations).unwrap(),
            TidalAlbumType::Compilations
        );
        assert_eq!(
            TidalAlbumType::try_from(AlbumType::EpsAndSingles).unwrap(),
            TidalAlbumType::EpsAndSingles
        );
    }

    #[test_log::test]
    fn test_tidal_album_type_try_from_album_type_unsupported() {
        assert!(TidalAlbumType::try_from(AlbumType::Live).is_err());
        assert!(TidalAlbumType::try_from(AlbumType::Other).is_err());
        assert!(TidalAlbumType::try_from(AlbumType::Download).is_err());
    }

    // ToValueType<TidalAlbumType> tests for JSON parsing
    #[test_log::test]
    fn test_tidal_album_type_to_value_type_lp() {
        let json_value = serde_json::json!("LP");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::Lp);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_epsandsingles() {
        let json_value = serde_json::json!("EPSANDSINGLES");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::EpsAndSingles);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_ep() {
        let json_value = serde_json::json!("EP");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::EpsAndSingles);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_single() {
        let json_value = serde_json::json!("SINGLE");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::EpsAndSingles);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_compilations() {
        let json_value = serde_json::json!("COMPILATIONS");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::Compilations);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_unknown_defaults_to_lp() {
        let json_value = serde_json::json!("UNKNOWN");
        let result: TidalAlbumType = (&json_value).to_value_type().unwrap();
        assert_eq!(result, TidalAlbumType::Lp);
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_null_returns_error() {
        let json_value = serde_json::json!(null);
        let result: Result<TidalAlbumType, _> = (&json_value).to_value_type();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_number_returns_error() {
        let json_value = serde_json::json!(123);
        let result: Result<TidalAlbumType, _> = (&json_value).to_value_type();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_object_returns_error() {
        let json_value = serde_json::json!({"type": "LP"});
        let result: Result<TidalAlbumType, _> = (&json_value).to_value_type();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_tidal_album_type_to_value_type_array_returns_error() {
        let json_value = serde_json::json!(["LP"]);
        let result: Result<TidalAlbumType, _> = (&json_value).to_value_type();
        assert!(result.is_err());
    }

    // TidalApiEndpoint to_url tests for all endpoints
    #[test_log::test]
    fn test_tidal_api_endpoint_device_authorization() {
        let url = tidal_api_endpoint!(DeviceAuthorization);
        assert_eq!(url, "https://auth.tidal.com/v1/oauth2/device_authorization");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_authorization_token() {
        let url = tidal_api_endpoint!(AuthorizationToken);
        assert_eq!(url, "https://auth.tidal.com/v1/oauth2/token");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_artist() {
        let url = tidal_api_endpoint!(Artist);
        assert_eq!(url, "https://api.tidal.com/v1/artists/:artistId");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_artist_with_id() {
        let url = tidal_api_endpoint!(Artist, &[(":artistId", "12345")]);
        assert_eq!(url, "https://api.tidal.com/v1/artists/12345");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_add_favorite_artist() {
        let url = tidal_api_endpoint!(AddFavoriteArtist, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/artists");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_favorite_albums() {
        let url = tidal_api_endpoint!(FavoriteAlbums, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/albums");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_add_favorite_album() {
        let url = tidal_api_endpoint!(AddFavoriteAlbum, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/albums");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_remove_favorite_album() {
        let url = tidal_api_endpoint!(
            RemoveFavoriteAlbum,
            &[(":userId", "999"), (":albumId", "54321")]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/users/999/favorites/albums/54321"
        );
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_artist_albums() {
        let url = tidal_api_endpoint!(ArtistAlbums, &[(":artistId", "12345")]);
        assert_eq!(url, "https://api.tidal.com/v1/artists/12345/albums");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_track() {
        let url = tidal_api_endpoint!(Track);
        assert_eq!(url, "https://api.tidal.com/v1/tracks/:trackId");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_track_with_id() {
        let url = tidal_api_endpoint!(Track, &[(":trackId", "99999")]);
        assert_eq!(url, "https://api.tidal.com/v1/tracks/99999");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_favorite_tracks() {
        let url = tidal_api_endpoint!(FavoriteTracks, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/tracks");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_add_favorite_track() {
        let url = tidal_api_endpoint!(AddFavoriteTrack, &[(":userId", "999")]);
        assert_eq!(url, "https://api.tidal.com/v1/users/999/favorites/tracks");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_remove_favorite_track() {
        let url = tidal_api_endpoint!(
            RemoveFavoriteTrack,
            &[(":userId", "999"), (":trackId", "88888")]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/users/999/favorites/tracks/88888"
        );
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_album_tracks() {
        let url = tidal_api_endpoint!(AlbumTracks, &[(":albumId", "54321")]);
        assert_eq!(url, "https://api.tidal.com/v1/albums/54321/tracks");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_track_url() {
        let url = tidal_api_endpoint!(TrackUrl, &[(":trackId", "99999")]);
        assert_eq!(url, "https://api.tidal.com/v1/tracks/99999/urlpostpaywall");
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_track_playback_info() {
        let url = tidal_api_endpoint!(TrackPlaybackInfo, &[(":trackId", "99999")]);
        assert_eq!(url, "https://api.tidal.com/v1/tracks/99999/playbackinfo");
    }

    // Error conversion tests
    #[test_log::test]
    fn test_error_to_music_api_error() {
        let error = Error::NoUserIdAvailable;
        let music_api_error: moosicbox_music_api::Error = error.into();
        // Verify it's wrapped in Other variant
        match music_api_error {
            moosicbox_music_api::Error::Other(_) => {}
            _ => panic!("Expected Error::Other variant"),
        }
    }

    #[test_log::test]
    fn test_try_from_album_type_error_to_music_api_error() {
        let error = TryFromAlbumTypeError;
        let music_api_error: moosicbox_music_api::Error = error.into();
        // Verify it's wrapped in Other variant
        match music_api_error {
            moosicbox_music_api::Error::Other(_) => {}
            _ => panic!("Expected Error::Other variant"),
        }
    }

    // Combined endpoint tests with params and query strings
    #[test_log::test]
    fn test_tidal_api_endpoint_track_url_with_query() {
        let url = tidal_api_endpoint!(
            TrackUrl,
            &[(":trackId", "12345")],
            &[
                ("audioquality", "HI_RES_LOSSLESS"),
                ("urlusagemode", "STREAM")
            ]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/tracks/12345/urlpostpaywall?audioquality=HI_RES_LOSSLESS&urlusagemode=STREAM"
        );
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_search_with_query() {
        let url = tidal_api_endpoint!(
            Search,
            &[],
            &[
                ("query", "test"),
                ("limit", "10"),
                ("types", "ARTISTS,ALBUMS")
            ]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/search/top-hits?query=test&limit=10&types=ARTISTS%2CALBUMS"
        );
    }

    #[test_log::test]
    fn test_tidal_api_endpoint_favorite_artists_with_query() {
        let url = tidal_api_endpoint!(
            FavoriteArtists,
            &[(":userId", "12345")],
            &[
                ("offset", "0"),
                ("limit", "100"),
                ("order", "DATE"),
                ("orderDirection", "DESC")
            ]
        );
        assert_eq!(
            url,
            "https://api.tidal.com/v1/users/12345/favorites/artists?offset=0&limit=100&order=DATE&orderDirection=DESC"
        );
    }
}
