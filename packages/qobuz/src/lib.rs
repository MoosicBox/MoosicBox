//! `MoosicBox` Qobuz integration providing client library for the Qobuz music streaming API.
//!
//! This crate implements the [`MusicApi`] trait for Qobuz, enabling access to high-resolution
//! music streaming, user favorites, library management, and search functionality.
//!
//! # Features
//!
//! * `api` - HTTP API endpoints for Qobuz operations (requires actix-web)
//! * `db` - Database persistence for authentication tokens and configuration
//! * `openapi` - `OpenAPI` documentation support
//! * `scan` - Library scanning capabilities
//!
//! # Basic Usage
//!
//! ```rust,no_run
//! # use moosicbox_qobuz::QobuzMusicApi;
//! # #[cfg(feature = "db")]
//! # {
//! # use switchy::database::profiles::LibraryDatabase;
//! # async fn example(db: LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! // Build a Qobuz client with database support
//! let client = QobuzMusicApi::builder()
//!     .with_db(db)
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! # Authentication
//!
//! Use [`user_login`] to authenticate with username and password credentials,
//! which returns an access token for subsequent API calls.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    str::Utf8Error,
    sync::{Arc, LazyLock},
};

use async_recursion::async_recursion;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use itertools::Itertools;
use models::{QobuzAlbum, QobuzArtist, QobuzRelease, QobuzSearchResults, QobuzTrack};
use moosicbox_files::get_content_length;
use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    serde_json::{ToNestedValue, ToValue},
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    MusicApi, TrackOrId,
    auth::{ApiAuth, username_password::UsernamePasswordAuth},
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
        TrackSource, search::api::ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, AlbumType, ApiSource, Artist, AudioFormat, PlaybackQuality, Track, id::Id,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use switchy::http::models::{Method, StatusCode};
use tokio::sync::Mutex;
use url::form_urlencoded;

use crate::models::QobuzImage;

#[cfg(feature = "db")]
use moosicbox_json_utils::database::DatabaseFetchError;
#[cfg(feature = "db")]
use switchy::database::{DatabaseError, profiles::LibraryDatabase};

/// HTTP API endpoints for Qobuz operations.
///
/// Provides actix-web route handlers for album, artist, track, and search endpoints.
/// Requires the `api` feature to be enabled.
#[cfg(feature = "api")]
pub mod api;

/// Database operations for persisting Qobuz authentication and configuration.
///
/// Provides functions to store and retrieve access tokens, app configurations, and user settings.
/// Requires the `db` feature to be enabled.
#[cfg(feature = "db")]
pub mod db;

/// Data models for Qobuz API responses and internal representations.
///
/// Contains types for albums, artists, tracks, images, genres, and search results,
/// along with conversions to standard `MoosicBox` music models.
pub mod models;

/// Errors that can occur when interacting with the Qobuz API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No user ID available for the operation.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// JSON parsing error.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// HTTP request error.
    #[error(transparent)]
    Http(#[from] switchy::http::Error),
    /// Database error (requires `db` feature).
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Database fetch error (requires `db` feature).
    #[cfg(feature = "db")]
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// No access token available for authentication.
    #[error("No access token available")]
    NoAccessTokenAvailable,
    /// No App ID found in the Qobuz bundle output.
    #[error("No App ID found in output")]
    NoAppId,
    /// No seed and timezone found in the Qobuz bundle output.
    #[error("No seed and timezone found in output")]
    NoSeedAndTimezone,
    /// No info and extras found in the Qobuz bundle output.
    #[error("No info and extras found in output")]
    NoInfoAndExtras,
    /// No matching info for timezone in the Qobuz bundle.
    #[error("No matching info for timezone")]
    NoMatchingInfoForTimezone,
    /// UTF-8 string conversion error.
    #[error(transparent)]
    Utf8(#[from] Utf8Error),
    /// Failed to fetch the Qobuz app ID from the bundle.
    #[error("Failed to fetch app id")]
    FailedToFetchAppId,
    /// No app secret available for signing requests.
    #[error("No app secret available")]
    NoAppSecretAvailable,
    /// Authentication failed (401 Unauthorized).
    #[error("Unauthorized")]
    Unauthorized,
    /// Generic request failure with error message.
    #[error("Request failed (error {0})")]
    RequestFailed(String),
    /// HTTP request failed with status code and message.
    #[error("Request failed (error {0}): {1}")]
    HttpRequestFailed(u16, String),
    /// Maximum number of retry attempts exceeded.
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
    /// HTTP response had no body.
    #[error("No response body")]
    NoResponseBody,
    /// JSON serialization/deserialization error.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Base64 decoding error.
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
    /// Qobuz configuration error.
    #[error(transparent)]
    Config(#[from] QobuzConfigError),
}

static AUTH_HEADER_NAME: &str = "x-user-auth-token";
static APP_ID_HEADER_NAME: &str = "x-app-id";

/// Device type for Qobuz API requests.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzDeviceType {
    /// Browser-based client.
    Browser,
}

trait ToUrl {
    fn to_url(&self) -> String;
}

static QOBUZ_PLAY_API_BASE_URL: &str = "https://play.qobuz.com";
static QOBUZ_API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

static CLIENT: LazyLock<switchy::http::Client> =
    LazyLock::new(|| switchy::http::Client::builder().build().unwrap());

pub static API_SOURCE: LazyLock<ApiSource> =
    LazyLock::new(|| ApiSource::register("Qobuz", "Qobuz"));

/// Formats a title with an optional version string appended.
///
/// If a version is provided, it will be appended to the title with a dash separator.
#[must_use]
pub fn format_title(title: &str, version: Option<&str>) -> String {
    version.as_ref().map_or_else(
        || title.to_string(),
        |version| format!("{title} - {version}"),
    )
}

#[derive(Clone)]
struct QobuzCredentials {
    access_token: String,
    app_id: Option<String>,
    username: Option<String>,
    #[cfg(feature = "db")]
    persist: bool,
}

#[allow(clippy::unused_async)]
async fn fetch_credentials(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<QobuzCredentials, Error> {
    #[cfg(feature = "db")]
    {
        Ok(if let Some(access_token) = access_token {
            log::debug!("Using passed access_token");
            Some(Ok(QobuzCredentials {
                access_token,
                app_id: None,
                username: None,
                persist: false,
            }))
        } else {
            log::debug!("Fetching db Qobuz config");

            match db::get_qobuz_config(db).await {
                Ok(Some(config)) => {
                    log::debug!("Using db Qobuz config");
                    log::debug!("Fetching db Qobuz app config");
                    match db::get_qobuz_app_config(db).await {
                        Ok(Some(app_config)) => {
                            log::debug!("Using db Qobuz app config");
                            Some(Ok(QobuzCredentials {
                                access_token: config.access_token,
                                app_id: app_id.or(Some(app_config.app_id)),
                                username: Some(config.user_email),
                                persist: true,
                            }))
                        }
                        Ok(None) => {
                            log::debug!("No Qobuz app config available");
                            None
                        }
                        Err(err) => {
                            log::error!("Failed to get Qobuz app config: {err:?}");
                            Some(Err(err))
                        }
                    }
                }
                Ok(None) => {
                    log::debug!("No Qobuz config available");
                    None
                }
                Err(err) => {
                    log::error!("Failed to get Qobuz app config: {err:?}");
                    Some(Err(err))
                }
            }
        }
        .ok_or(Error::NoAccessTokenAvailable)??)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(QobuzCredentials {
            access_token: access_token.ok_or(Error::NoAccessTokenAvailable)?,
            app_id,
            username: None,
        })
    }
}

async fn authenticated_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<Value, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Get,
        url,
        app_id,
        access_token,
        None,
        None,
        1,
    )
    .await?
    .ok_or_else(|| Error::NoResponseBody)
}

#[allow(unused)]
async fn authenticated_post_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Value>,
) -> Result<Option<Value>, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Post,
        url,
        app_id,
        access_token,
        body,
        form,
        1,
    )
    .await
}

#[allow(unused)]
async fn authenticated_delete_request(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<Option<Value>, Error> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Delete,
        url,
        app_id,
        access_token,
        None,
        None,
        1,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
async fn authenticated_request_inner(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    method: Method,
    url: &str,
    app_id: Option<String>,
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
        app_id,
        access_token,
    )
    .await?;

    let Some(app_id) = &credentials.app_id else {
        log::debug!("No app_id available");
        return Err(Error::Unauthorized);
    };

    let mut request = CLIENT
        .request(method, url)
        .header(APP_ID_HEADER_NAME, app_id)
        .header(AUTH_HEADER_NAME, &credentials.access_token);

    if let Some(form) = &form {
        request = request.form(form);
    }
    if let Some(body) = &body {
        request = request.json(body);
    }

    let response = request.send().await?;

    let status: u16 = response.status().into();

    log::debug!("Received authenticated request response status: {status}");

    match status {
        401 => {
            log::debug!("Received unauthorized response");

            let Some(username) = credentials.username else {
                return Err(Error::Unauthorized);
            };

            return authenticated_request_inner(
                #[cfg(feature = "db")]
                db,
                method,
                url,
                Some(app_id.clone()),
                Some(
                    refetch_access_token(
                        #[cfg(feature = "db")]
                        db,
                        app_id,
                        &username,
                        &credentials.access_token,
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
        400..=599 => Err(Error::HttpRequestFailed(
            status,
            response.text().await.unwrap_or_else(|_| String::new()),
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
    app_id: &str,
    username: &str,
    access_token: &str,
    #[cfg(feature = "db")] persist: bool,
) -> Result<String, Error> {
    log::debug!("Refetching access token");
    let url = qobuz_api_endpoint!(
        UserLogin,
        &[],
        &[("username", username), ("user_auth_token", access_token)]
    );

    let value: Value = CLIENT
        .post(&url)
        .header(APP_ID_HEADER_NAME, app_id)
        .send()
        .await?
        .json()
        .await?;

    let access_token = value.to_value::<&str>("access_token")?;

    #[cfg(feature = "db")]
    if persist {
        let access_token: &str = value.to_value("user_auth_token")?;
        let user_id: u64 = value.to_nested_value(&["user", "id"])?;
        let user_email: &str = value.to_nested_value(&["user", "email"])?;
        let user_public_id: &str = value.to_nested_value(&["user", "publicId"])?;

        db::create_qobuz_config(db, access_token, user_id, user_email, user_public_id).await?;
    }

    Ok(access_token.to_string())
}

#[allow(unused)]
enum QobuzApiEndpoint {
    Login,
    Bundle,
    UserLogin,
    Artist,
    ArtistAlbums,
    Album,
    Track,
    TrackFileUrl,
    Favorites,
    AddFavorites,
    RemoveFavorites,
    Search,
}

impl ToUrl for QobuzApiEndpoint {
    fn to_url(&self) -> String {
        match self {
            Self::Login => {
                format!("{QOBUZ_PLAY_API_BASE_URL}/login")
            }
            Self::Bundle => format!("{QOBUZ_PLAY_API_BASE_URL}/resources/:bundleVersion/bundle.js"),
            Self::UserLogin => format!("{QOBUZ_API_BASE_URL}/user/login"),
            Self::Artist => format!("{QOBUZ_API_BASE_URL}/artist/get"),
            Self::ArtistAlbums => format!("{QOBUZ_API_BASE_URL}/artist/getReleasesList"),
            Self::Album => format!("{QOBUZ_API_BASE_URL}/album/get"),
            Self::Track => format!("{QOBUZ_API_BASE_URL}/track/get"),
            Self::TrackFileUrl => format!("{QOBUZ_API_BASE_URL}/track/getFileUrl"),
            Self::Favorites => format!("{QOBUZ_API_BASE_URL}/favorite/getUserFavorites"),
            Self::AddFavorites => format!("{QOBUZ_API_BASE_URL}/favorite/create"),
            Self::RemoveFavorites => format!("{QOBUZ_API_BASE_URL}/favorite/delete"),
            Self::Search => format!("{QOBUZ_API_BASE_URL}/catalog/search"),
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
macro_rules! qobuz_api_endpoint {
    ($name:ident $(,)?) => {
        QobuzApiEndpoint::$name.to_url()
    };

    ($name:ident, $params:expr) => {
        replace_all(&qobuz_api_endpoint!($name), $params)
    };

    ($name:ident, $params:expr, $query:expr) => {
        attach_query_string(&qobuz_api_endpoint!($name, $params), $query)
    };
}

/// Album release type categories used by Qobuz.
#[derive(Default, Debug, Serialize, Deserialize, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum QobuzAlbumReleaseType {
    /// Standard studio album.
    #[default]
    Album,
    /// Live recording.
    Live,
    /// Compilation album.
    Compilation,
    /// Extended play (EP).
    Ep,
    /// Single release.
    Single,
    /// EP or Single (combined category).
    EpSingle,
    /// Other release type.
    Other,
    /// Download-only release.
    Download,
}

impl TryFrom<&str> for QobuzAlbumReleaseType {
    type Error = strum::ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "album" => Self::Album,
            "live" => Self::Live,
            "compilation" => Self::Compilation,
            "ep" => Self::Ep,
            "single" => Self::Single,
            "epmini" | "epSingle" => Self::EpSingle,
            "other" => Self::Other,
            "download" => Self::Download,
            _ => return Err(Self::Error::VariantNotFound),
        })
    }
}

impl From<AlbumType> for QobuzAlbumReleaseType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => Self::Album,
            AlbumType::Live => Self::Live,
            AlbumType::Compilations => Self::Compilation,
            AlbumType::EpsAndSingles => Self::EpSingle,
            AlbumType::Other => Self::Other,
            AlbumType::Download => Self::Download,
        }
    }
}

impl From<QobuzAlbumReleaseType> for AlbumType {
    fn from(value: QobuzAlbumReleaseType) -> Self {
        match value {
            QobuzAlbumReleaseType::Album => Self::Lp,
            QobuzAlbumReleaseType::Live => Self::Live,
            QobuzAlbumReleaseType::Compilation => Self::Compilations,
            QobuzAlbumReleaseType::Ep
            | QobuzAlbumReleaseType::Single
            | QobuzAlbumReleaseType::EpSingle => Self::EpsAndSingles,
            QobuzAlbumReleaseType::Other => Self::Other,
            QobuzAlbumReleaseType::Download => Self::Download,
        }
    }
}

impl MissingValue<QobuzAlbumReleaseType> for &Value {}
impl ToValueType<QobuzAlbumReleaseType> for &Value {
    fn to_value_type(self) -> Result<QobuzAlbumReleaseType, ParseError> {
        QobuzAlbumReleaseType::try_from(self.as_str().ok_or_else(|| {
            ParseError::MissingValue(format!(
                "QobuzAlbumReleaseType: ({})",
                serde_json::to_string(self).unwrap_or_default()
            ))
        })?)
        .map_err(|e| ParseError::ConvertType(format!("QobuzAlbumReleaseType: {e:?}")))
    }
}

/// Sort options for album listings in Qobuz.
#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum QobuzAlbumSort {
    /// Sort by release date.
    ReleaseDate,
    /// Sort by relevance.
    Relevant,
    /// Sort by release date with priority weighting (default).
    #[default]
    ReleaseDateByPriority,
}

/// Sort order direction for album listings in Qobuz.
#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum QobuzAlbumOrder {
    /// Ascending order.
    Asc,
    /// Descending order (default).
    #[default]
    Desc,
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
            | Error::NoAppId
            | Error::NoSeedAndTimezone
            | Error::NoInfoAndExtras
            | Error::NoMatchingInfoForTimezone
            | Error::Utf8(..)
            | Error::FailedToFetchAppId
            | Error::NoAppSecretAvailable
            | Error::RequestFailed(..)
            | Error::HttpRequestFailed(..)
            | Error::MaxFailedAttempts
            | Error::NoResponseBody
            | Error::Serde(..)
            | Error::Base64Decode(..)
            | Error::Config(..) => Err(e),
            #[cfg(feature = "db")]
            Error::Database(_) | Error::DatabaseFetch(_) => Err(e),
        }
    } else {
        log::debug!("validate_credentials: success");
        Ok(true)
    }
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn user_login(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    username: &str,
    password: &str,
    app_id: Option<String>,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, Error> {
    let url = qobuz_api_endpoint!(
        UserLogin,
        &[],
        &[
            ("username", username),
            ("email", username),
            ("password", password),
        ]
    );

    let app_id = match app_id {
        Some(app_id) => app_id,
        None => {
            if let Some(app_config) = {
                #[cfg(feature = "db")]
                {
                    db::get_qobuz_app_config(db).await?
                }

                #[cfg(not(feature = "db"))]
                None::<String>
            } {
                #[cfg(feature = "db")]
                {
                    app_config.app_id
                }

                #[cfg(not(feature = "db"))]
                {
                    app_config
                }
            } else {
                let login_source = fetch_login_source().await?;
                let bundle_version =
                    search_bundle_version(&login_source).ok_or(Error::FailedToFetchAppId)?;
                let bundle = fetch_bundle_source(&bundle_version).await?;
                let config = search_app_config(&bundle)?;

                #[cfg(feature = "db")]
                {
                    log::debug!(
                        "Creating Qobuz app config: bundle_version={bundle_version} app_id={}",
                        config.app_id
                    );
                    let app_config =
                        db::create_qobuz_app_config(db, &bundle_version, &config.app_id).await?;

                    for (timezone, secret) in config.secrets {
                        log::debug!("Creating Qobuz app secret: timezone={bundle_version}");
                        db::create_qobuz_app_secret(db, app_config.id, &timezone, &secret).await?;
                    }
                }

                config.app_id
            }
        }
    };

    let response = CLIENT
        .post(&url)
        .header(APP_ID_HEADER_NAME, &app_id)
        .header(switchy::http::Header::ContentLength.as_ref(), "0")
        .send()
        .await?;

    if response.status() == StatusCode::Unauthorized {
        return Err(Error::Unauthorized);
    } else if !response.status().is_success() {
        log::error!(
            "Received unsuccessful response: error {}",
            response.status()
        );
    }

    let value: Value = response.json().await?;

    let access_token: &str = value.to_value("user_auth_token")?;
    let user_id: u64 = value.to_nested_value(&["user", "id"])?;
    let user_email: &str = value.to_nested_value(&["user", "email"])?;
    let user_public_id: &str = value.to_nested_value(&["user", "publicId"])?;

    #[cfg(feature = "db")]
    if persist.unwrap_or(false) {
        db::create_qobuz_config(db, access_token, user_id, user_email, user_public_id).await?;
    }

    Ok(serde_json::json!({
        "accessToken": access_token,
        "userId": user_id,
        "userEmail": user_email,
        "userPublicId": user_public_id,
    }))
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzArtist, Error> {
    let url = qobuz_api_endpoint!(
        Artist,
        &[],
        &[("artist_id", &artist_id.to_string()), ("limit", "0")]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received artist response: {value:?}");

    Ok(value.to_value_type()?)
}

/// Retrieves a paginated list of favorite artists from Qobuz.
///
/// # Errors
///
/// * If the API request failed
/// * If there is a database error while fetching the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> PagingResult<QobuzArtist, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = qobuz_api_endpoint!(
        Favorites,
        &[],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("type", "artists"),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id.clone(),
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite artists response: {value:?}");

    let items: Vec<QobuzArtist> = value.to_nested_value(&["artists", "items"])?;
    let total = value.to_nested_value(&["artists", "total"])?;

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
            let access_token = access_token.clone();
            let app_id = app_id.clone();

            Box::pin(async move {
                favorite_artists(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    access_token,
                    app_id,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn add_favorite_artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(
        AddFavorites,
        &[],
        &[("artist_ids", &artist_id.to_string()),]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received add favorite artist response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn remove_favorite_artist(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(
        RemoveFavorites,
        &[],
        &[("artist_ids", &artist_id.to_string()),]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite artist response: {value:?}");

    Ok(())
}

/// Retrieves a paginated list of albums for a specific artist from Qobuz.
///
/// # Errors
///
/// * If the API request failed
/// * If there is a database error while fetching the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    release_type: Option<QobuzAlbumReleaseType>,
    sort: Option<QobuzAlbumSort>,
    order: Option<QobuzAlbumOrder>,
    track_size: Option<u8>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> PagingResult<QobuzRelease, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = qobuz_api_endpoint!(
        ArtistAlbums,
        &[],
        &[
            ("artist_id", &artist_id.to_string()),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("release_type", release_type.unwrap_or_default().as_ref()),
            ("sort", sort.unwrap_or_default().as_ref()),
            ("order", order.unwrap_or_default().as_ref()),
            ("track_size", &track_size.unwrap_or(1).to_string()),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id.clone(),
        access_token.clone(),
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value.to_value("items")?;
    let has_more = value.to_value("has_more")?;

    #[cfg(feature = "db")]
    let db = db.clone();
    let artist_id = artist_id.clone();

    Ok(PagingResponse {
        page: Page::WithHasMore {
            items,
            offset,
            limit,
            has_more,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            #[cfg(feature = "db")]
            let db = db.clone();
            let artist_id = artist_id.clone();
            let access_token = access_token.clone();
            let app_id = app_id.clone();

            Box::pin(async move {
                artist_albums(
                    #[cfg(feature = "db")]
                    &db,
                    &artist_id,
                    Some(offset),
                    Some(limit),
                    release_type,
                    sort,
                    order,
                    track_size,
                    access_token,
                    app_id,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzAlbum, Error> {
    let url = qobuz_api_endpoint!(
        Album,
        &[],
        &[("album_id", &album_id.to_string()), ("limit", "0")]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    Ok(value.to_value_type()?)
}

async fn request_favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: u32,
    mut limit: u32,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<Value, Error> {
    let offset = offset.to_string();

    loop {
        let url = qobuz_api_endpoint!(
            Favorites,
            &[],
            &[
                ("offset", &offset),
                ("limit", &limit.to_string()),
                ("type", "albums"),
            ]
        );

        match authenticated_request(
            #[cfg(feature = "db")]
            db,
            &url,
            app_id.clone(),
            access_token.clone(),
        )
        .await
        {
            Ok(value) => return Ok(value),
            Err(err) => match err {
                Error::NoResponseBody => {
                    log::debug!("Received empty response for favorite albums... retrying");
                    limit += 1;
                }
                _ => return Err(err),
            },
        }
    }
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
#[async_recursion]
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    #[allow(clippy::used_underscore_binding)] _album_type: Option<QobuzAlbumReleaseType>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> PagingResult<QobuzAlbum, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let value = request_favorite_albums(
        #[cfg(feature = "db")]
        db,
        offset,
        limit,
        access_token.clone(),
        app_id.clone(),
    )
    .await?;

    let items: Vec<QobuzAlbum> = value
        .to_nested_value::<Vec<_>>(&["albums", "items"])?
        .into_iter()
        .take(limit as usize)
        .collect();

    let total = value.to_nested_value(&["albums", "total"])?;

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
            let access_token = access_token.clone();
            let app_id = app_id.clone();

            Box::pin(async move {
                favorite_albums(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    _album_type,
                    access_token,
                    app_id,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn all_favorite_albums(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<Vec<QobuzAlbum>, Error> {
    let mut all_albums = vec![];

    let mut offset = 0;
    let limit = 100;

    loop {
        let albums = favorite_albums(
            #[cfg(feature = "db")]
            db,
            Some(offset),
            Some(limit),
            None,
            access_token.clone(),
            app_id.clone(),
        )
        .await?;

        all_albums.extend_from_slice(&albums);

        if !albums.has_more() {
            break;
        }

        offset += limit;
    }

    Ok(all_albums)
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn add_favorite_album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(AddFavorites, &[], &[("album_ids", &album_id.to_string()),]);

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received add favorite album response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn remove_favorite_album(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(
        RemoveFavorites,
        &[],
        &[("album_ids", &album_id.to_string()),]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite album response: {value:?}");

    Ok(())
}

/// Retrieves a paginated list of tracks for a specific album from Qobuz.
///
/// # Errors
///
/// * If the API request failed
/// * If there is a database error while fetching the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> PagingResult<QobuzTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = qobuz_api_endpoint!(
        Album,
        &[],
        &[
            ("album_id", &album_id.to_string()),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id.clone(),
        access_token.clone(),
    )
    .await?;

    log::trace!("Received album tracks response: {value:?}");

    let artist = value.to_nested_value(&["artist", "name"])?;
    let artist_id = value.to_nested_value(&["artist", "id"])?;
    let album = value.to_value("title")?;
    let album_type = value.to_value("release_type")?;
    let version = value.to_value("version")?;
    let image: Option<QobuzImage> = value.to_value("image")?;
    let items: Vec<QobuzTrack> = value
        .to_nested_value::<Vec<&Value>>(&["tracks", "items"])?
        .iter()
        .map(move |value| {
            QobuzTrack::from_value(
                value,
                artist,
                artist_id,
                album,
                &TryInto::<String>::try_into(album_id.clone())
                    .map_err(|e| ParseError::Parse(format!("album_id: {e:?}")))?,
                album_type,
                version,
                image.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total = value.to_nested_value(&["tracks", "total"])?;

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
            let access_token = access_token.clone();
            let app_id = app_id.clone();

            Box::pin(async move {
                album_tracks(
                    #[cfg(feature = "db")]
                    &db,
                    &album_id,
                    Some(offset),
                    Some(limit),
                    access_token,
                    app_id,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
pub async fn track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzTrack, Error> {
    let url = qobuz_api_endpoint!(
        Track,
        &[],
        &[("track_id", &track_id.to_string()), ("limit", "0")]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::debug!("Received track response: {value:#?}");

    let album: QobuzAlbum = value.to_value("album")?;

    Ok(QobuzTrack::from_value(
        &value,
        &album.artist,
        album.artist_id,
        &album.title,
        album.id.as_ref(),
        album.album_type,
        album.version.as_deref(),
        album.image,
    )?)
}

/// Retrieves a paginated list of favorite tracks from Qobuz.
///
/// # Errors
///
/// * If the API request failed
/// * If there is a database error while fetching the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> PagingResult<QobuzTrack, Error> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = qobuz_api_endpoint!(
        Favorites,
        &[],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("type", "tracks"),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id.clone(),
        access_token.clone(),
    )
    .await?;

    let items: Vec<QobuzTrack> = value.to_nested_value(&["tracks", "items"])?;
    let total = value.to_nested_value(&["tracks", "total"])?;

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
            let access_token = access_token.clone();
            let app_id = app_id.clone();

            Box::pin(async move {
                favorite_tracks(
                    #[cfg(feature = "db")]
                    &db,
                    Some(offset),
                    Some(limit),
                    access_token,
                    app_id,
                )
                .await
            })
        }))),
    })
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn add_favorite_track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(AddFavorites, &[], &[("track_ids", &track_id.to_string()),]);

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received add favorite track response: {value:?}");

    Ok(())
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
pub async fn remove_favorite_track(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), Error> {
    let url = qobuz_api_endpoint!(
        RemoveFavorites,
        &[],
        &[("track_ids", &track_id.to_string()),]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received remove favorite track response: {value:?}");

    Ok(())
}

/// Audio quality options for Qobuz track streaming.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum QobuzAudioQuality {
    /// MP3 320 kbps.
    Low,
    /// FLAC 16-bit 44.1kHz lossless.
    FlacLossless,
    /// FLAC 24-bit up to 96kHz high-resolution.
    FlacHiRes,
    /// FLAC 24-bit above 96kHz up to 192kHz highest resolution.
    FlacHighestRes,
}

impl QobuzAudioQuality {
    const fn as_format_id(self) -> u8 {
        match self {
            Self::Low => 5,
            Self::FlacLossless => 6,
            Self::FlacHiRes => 7,
            Self::FlacHighestRes => 27,
        }
    }
}

impl From<TrackAudioQuality> for QobuzAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => Self::Low,
            TrackAudioQuality::FlacLossless => Self::FlacLossless,
            TrackAudioQuality::FlacHiRes => Self::FlacHiRes,
            TrackAudioQuality::FlacHighestRes => Self::FlacHighestRes,
        }
    }
}

/// # Panics
///
/// * If time went backwards
///
/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
pub async fn track_file_url(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    track_id: &Id,
    quality: QobuzAudioQuality,
    access_token: Option<String>,
    app_id: Option<String>,
    app_secret: Option<String>,
) -> Result<String, Error> {
    #[cfg(feature = "db")]
    let app_secret = if let Some(app_secret) = app_secret {
        app_secret
    } else {
        let app_secrets = db::get_qobuz_app_secrets(db).await?;
        let app_secrets = app_secrets
            .iter()
            .find(|secret| secret.timezone == "berlin")
            .or_else(|| app_secrets.first())
            .ok_or(Error::NoAppSecretAvailable)?;

        app_secrets.secret.clone()
    };

    #[cfg(not(feature = "db"))]
    let app_secret = app_secret.ok_or(Error::NoAppSecretAvailable)?;

    let intent = "stream";
    let format_id = quality.as_format_id();
    let request_ts = switchy::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let request_sig = format!(
        "trackgetFileUrlformat_id{format_id}intent{intent}track_id{track_id}{request_ts}{app_secret}"
    );
    let request_sig = format!("{:x}", md5::compute(request_sig));

    let url = qobuz_api_endpoint!(
        TrackFileUrl,
        &[],
        &[
            ("track_id", &track_id.to_string()),
            ("format_id", &format_id.to_string()),
            ("intent", intent),
            ("request_ts", &request_ts.to_string()),
            ("request_sig", &request_sig),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received track file url response: {value:?}");

    let url = value.to_value("url")?;

    Ok(url)
}

/// # Errors
///
/// * If the API request failed
/// * If there is a database error while saving the configuration
/// * If there is no access token available
/// * If there is no app ID available
/// * If failed to fetch the Qobuz login source
/// * If failed to fetch the Qobuz app bundle
/// * If failed to fetch the Qobuz app secrets
/// * If failed to fetch the Qobuz app ID
/// * If failed to parse the JSON response
#[allow(clippy::too_many_arguments)]
pub async fn search(
    #[cfg(feature = "db")] db: &LibraryDatabase,
    query: &str,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzSearchResults, Error> {
    let url = qobuz_api_endpoint!(
        Search,
        &[],
        &[
            ("query", query),
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(10).to_string()),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received search response: {value:?}");

    Ok(value.to_value_type()?)
}

#[allow(unused)]
async fn fetch_login_source() -> Result<String, Error> {
    let url = qobuz_api_endpoint!(Login);

    Ok(CLIENT.get(&url).send().await?.text().await?)
}

#[allow(unused)]
fn search_bundle_version(login_source: &str) -> Option<String> {
    static BUNDLE_ID_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"<script src="/resources/(\d+\.\d+\.\d+-[a-z]\d{3})/bundle\.js"></script>"#,
        )
        .unwrap()
    });

    if let Some(caps) = BUNDLE_ID_REGEX.captures(login_source)
        && let Some(version) = caps.get(1)
    {
        let version = version.as_str();
        log::debug!("Found version={version}");
        return Some(version.to_string());
    }

    None
}

#[allow(unused)]
async fn fetch_bundle_source(bundle_version: &str) -> Result<String, Error> {
    let url = qobuz_api_endpoint!(Bundle, &[(":bundleVersion", bundle_version)]);

    Ok(CLIENT.get(&url).send().await?.text().await?)
}

fn capitalize(value: &str) -> String {
    let mut v: Vec<char> = value.chars().collect();
    v[0] = v[0].to_uppercase().next().unwrap();
    v.into_iter().collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppConfig {
    pub(crate) app_id: String,
    pub(crate) secrets: BTreeMap<String, String>,
}

#[allow(unused)]
pub(crate) fn search_app_config(bundle: &str) -> Result<AppConfig, Error> {
    static SEED_AND_TIMEZONE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"[a-z]\.initialSeed\("([\w=]+)",window\.utimezone\.(.+?)\)"#).unwrap()
    });
    static INFO_AND_EXTRAS_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"name:"\w+/([^"]+)",info:"([\w=]+)",extras:"([\w=]+)""#).unwrap()
    });
    static APP_ID_REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"production:\{api:\{appId:"([^"]+)""#).unwrap());

    let app_id = if let Some(caps) = APP_ID_REGEX.captures(bundle) {
        if let Some(app_id) = caps.get(1) {
            let app_id = app_id.as_str();
            log::debug!("Found app_id={app_id}");
            app_id.to_string()
        } else {
            return Err(Error::NoAppId);
        }
    } else {
        return Err(Error::NoAppId);
    };

    let mut seed_timezones = vec![];

    for caps in SEED_AND_TIMEZONE_REGEX.captures_iter(bundle) {
        let seed = if let Some(seed) = caps.get(1) {
            let seed = seed.as_str();
            log::debug!("Found seed={seed}");
            seed.to_string()
        } else {
            return Err(Error::NoSeedAndTimezone);
        };
        let timezone = if let Some(timezone) = caps.get(2) {
            let timezone = timezone.as_str();
            log::debug!("Found timezone={timezone}");
            timezone.to_string()
        } else {
            return Err(Error::NoSeedAndTimezone);
        };

        seed_timezones.push((seed, timezone));
    }

    if seed_timezones.is_empty() {
        return Err(Error::NoSeedAndTimezone);
    }

    let mut name_info_extras = vec![];

    for caps in INFO_AND_EXTRAS_REGEX.captures_iter(bundle) {
        let name = if let Some(name) = caps.get(1) {
            let name = name.as_str();
            log::debug!("Found name={name}");
            name.to_string()
        } else {
            return Err(Error::NoInfoAndExtras);
        };
        let info = if let Some(info) = caps.get(2) {
            let info = info.as_str();
            log::debug!("Found info={info}");
            info.to_string()
        } else {
            return Err(Error::NoInfoAndExtras);
        };
        let extras = if let Some(extras) = caps.get(3) {
            let extras = extras.as_str();
            log::debug!("Found extras={extras}");
            extras.to_string()
        } else {
            return Err(Error::NoInfoAndExtras);
        };

        name_info_extras.push((name, info, extras));
    }

    if name_info_extras.is_empty() {
        return Err(Error::NoInfoAndExtras);
    }

    let mut secrets = BTreeMap::new();

    log::trace!("seed_timezones={:?}", &seed_timezones);
    for (seed, timezone) in seed_timezones {
        log::trace!("name_info_extras={:?}", &name_info_extras);
        let (_, info, _) = name_info_extras
            .iter()
            .find(|(name, _, _)| name.starts_with(&capitalize(&timezone)))
            .ok_or(Error::NoMatchingInfoForTimezone)
            .expect("No matching name for timezone");

        let secret_base64 = format!("{seed}{info}");
        let secret_base64 = &secret_base64[..44];
        let secret = general_purpose::STANDARD.decode(secret_base64)?;
        let secret = std::str::from_utf8(&secret)?.to_string();

        secrets.insert(timezone, secret);
    }

    Ok(AppConfig { app_id, secrets })
}

impl From<Error> for moosicbox_music_api::Error {
    fn from(err: Error) -> Self {
        Self::Other(Box::new(err))
    }
}

/// Errors that can occur during Qobuz API configuration and initialization.
#[derive(Debug, thiserror::Error)]
pub enum QobuzConfigError {
    /// Database connection is missing (requires `db` feature).
    #[cfg(feature = "db")]
    #[error("Missing Db")]
    MissingDb,
    /// Database fetch error (requires `db` feature).
    #[cfg(feature = "db")]
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Music API error.
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
}

/// Builder for constructing a `QobuzMusicApi` instance with configuration options.
#[derive(Default)]
pub struct QobuzMusicApiBuilder {
    /// Database connection for persisting authentication tokens and configuration (requires `db` feature).
    #[cfg(feature = "db")]
    db: Option<LibraryDatabase>,
}

impl QobuzMusicApiBuilder {
    /// Sets the database connection (builder pattern, consumes self).
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

    /// Builds a `QobuzMusicApi` instance with the configured settings.
    ///
    /// # Errors
    ///
    /// * If the `db` is missing
    #[allow(clippy::unused_async)]
    pub async fn build(self) -> Result<QobuzMusicApi, QobuzConfigError> {
        #[cfg(feature = "db")]
        let db = self.db.ok_or(QobuzConfigError::MissingDb)?;

        #[cfg(not(feature = "db"))]
        let logged_in = false;
        #[cfg(feature = "db")]
        let logged_in = crate::db::get_qobuz_config(&db)
            .await
            .is_ok_and(|x| x.is_some());

        let auth = ApiAuth::builder()
            .with_logged_in(logged_in)
            .with_auth(
                UsernamePasswordAuth::builder()
                    .with_handler({
                        #[cfg(feature = "db")]
                        let db = db.clone();
                        move |username, password| {
                            #[cfg(feature = "db")]
                            let db = db.clone();
                            async move {
                                user_login(
                                    #[cfg(feature = "db")]
                                    &db,
                                    &username,
                                    &password,
                                    None,
                                    #[cfg(feature = "db")]
                                    Some(true),
                                )
                                .await
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
                                Ok(true)
                            }
                        }
                    })
                    .build()?,
            )
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

        Ok(QobuzMusicApi {
            #[cfg(feature = "db")]
            db,
            auth,
        })
    }
}

/// Implementation of the `MusicApi` trait for Qobuz music service.
pub struct QobuzMusicApi {
    /// Database connection for storing authentication and configuration (requires `db` feature).
    #[cfg(feature = "db")]
    db: LibraryDatabase,
    /// Authentication manager for handling login and credential validation.
    auth: ApiAuth,
}

impl QobuzMusicApi {
    /// Creates a new builder for configuring and constructing a `QobuzMusicApi` instance.
    #[must_use]
    pub fn builder() -> QobuzMusicApiBuilder {
        QobuzMusicApiBuilder::default()
    }
}

#[async_trait]
impl MusicApi for QobuzMusicApi {
    fn source(&self) -> &ApiSource {
        &API_SOURCE
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<ArtistOrder>,
        _order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, moosicbox_music_api::Error> {
        Ok(favorite_artists(
            #[cfg(feature = "db")]
            &self.db,
            offset,
            limit,
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
        )
        .await?;

        Ok(artist
            .image
            .as_ref()
            .and_then(|x| x.cover_url_for_size(size.into()))
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
            request
                .filters
                .as_ref()
                .and_then(|x| x.album_type.map(Into::into)),
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
        Ok(artist_albums(
            #[cfg(feature = "db")]
            &self.db,
            artist_id,
            offset,
            limit,
            album_type.map(Into::into),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?)
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Ok(add_favorite_album(
            #[cfg(feature = "db")]
            &self.db,
            album_id,
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
        )
        .await?;

        Ok(album
            .image
            .as_ref()
            .and_then(|x| x.cover_url_for_size(size.into()))
            .map(|url| ImageCoverSource::RemoteUrl { url, headers: None }))
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        let Some(track_ids) = track_ids else {
            return Ok(favorite_tracks(
                #[cfg(feature = "db")]
                &self.db,
                offset,
                limit,
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
            track.id(),
            quality.into(),
            None,
            None,
            None,
        )
        .await?;

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
        let url = track_file_url(
            #[cfg(feature = "db")]
            &self.db,
            track.id(),
            QobuzAudioQuality::Low,
            None,
            None,
            None,
        )
        .await?;

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
        )
        .await?;

        Ok(results.into())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::*;

    static TEST_LOGIN_SOURCE: &str = r#"</script>
        <script src="/resources/7.1.3-b011/bundle.js"></script>
        </body>"#;
    static TEST_BUNDLE_SOURCE: &str = r#"s,extra:o},production:{api:{appId:"123456789",appSecret{var e=window.__ENVIRONMENT__;return"recette"===e?d.initialSeed("YjBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.london):"integration"===e?d.initialSeed("MjBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.algier):d.initialSeed("MzBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.berlin)},d.string{offset:"GMT",name:"Europe/Dublin",info:"XXXXX",extras:"XXXXX"},{offset:"GMT",name:"Europe/Lisbon"},{offset:"GMT",name:"Europe/London",info:"VmMjU1NTU1NTU=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"UTC",name:"UTC"},{offset:"GMT+01:00",name:"Africa/Algiers",info:"VmMjU1NTU1NTI=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"GMT+01:00",name:"Africa/Windhoek"},{offset:"GMT+01:00",name:"Atlantic/Azores"},{offset:"GMT+01:00",name:"Atlantic/Stanley"},{offset:"GMT+01:00",name:"Europe/Amsterdam"},{offset:"GMT+01:00",name:"Europe/Paris",info:"XXXXX",extras:"XXXXX"},{offset:"GMT+01:00",name:"Europe/Belgrade"},{offset:"GMT+01:00",name:"Europe/Brussels"},{offset:"GMT+02:00",name:"Africa/Cairo"},{offset:"GMT+02:00",name:"Africa/Blantyre"},{offset:"GMT+02:00",name:"Asia/Beirut"},{offset:"GMT+02:00",name:"Asia/Damascus"},{offset:"GMT+02:00",name:"Asia/Gaza"},{offset:"GMT+02:00",name:"Asia/Jerusalem"},{offset:"GMT+02:00",name:"Europe/Berlin",info:"VmMjU1NTU1NTM=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"GMT+03:00",name:"Africa/Addis_Ababa"},{offset:"GMT+03:00",name:"Asia/Riyadh89"},{offset:"GMT+03:00",name:"Europe/Minsk"},{offset:"GMT+03:30""#;

    #[switchy_async::test]
    async fn test_search_bundle_version() {
        let version =
            search_bundle_version(TEST_LOGIN_SOURCE).expect("Failed to search_bundle_version");

        assert_eq!(version, "7.1.3-b011");
    }

    #[switchy_async::test]
    async fn test_search_app_config() {
        let secrets = search_app_config(TEST_BUNDLE_SOURCE).expect("Failed to search_app_config");

        assert_eq!(
            secrets,
            AppConfig {
                app_id: "123456789".to_string(),
                secrets: BTreeMap::from([
                    (
                        "london".to_string(),
                        "b0b0b0bd3adb33fcd6a7405f25555555".to_string()
                    ),
                    (
                        "algier".to_string(),
                        "20b0b0bd3adb33fcd6a7405f25555552".to_string()
                    ),
                    (
                        "berlin".to_string(),
                        "30b0b0bd3adb33fcd6a7405f25555553".to_string()
                    )
                ])
            }
        );
    }
}
