#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use std::sync::Arc;

#[cfg(feature = "db")]
use moosicbox_database::{Database, DatabaseError};

use async_recursion::async_recursion;
use async_trait::async_trait;
use moosicbox_core::sqlite::models::{
    tidal::{TidalAlbum, TidalArtist, TidalTrack},
    Album, ApiSource, Artist, AsModelResult, LibraryAlbum, Track,
};
use moosicbox_json_utils::{serde_json::ToValue, ParseError};
use moosicbox_music_api::{
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumOrder, AlbumOrderDirection,
    AlbumType, AlbumsError, ArtistAlbumsError, ArtistError, ArtistOrder, ArtistOrderDirection,
    ArtistsError, Id, LibraryAlbumError, MusicApi, RemoveAlbumError, RemoveArtistError,
    RemoveTrackError, TrackError, TrackOrder, TrackOrderDirection, TracksError,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::sync::Mutex;
use url::form_urlencoded;

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalDeviceType {
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
}

static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| reqwest::Client::builder().build().unwrap());

static TIDAL_AUTH_API_BASE_URL: &str = "https://auth.tidal.com/v1";
static TIDAL_API_BASE_URL: &str = "https://api.tidal.com/v1";

impl ToUrl for TidalApiEndpoint {
    fn to_url(&self) -> String {
        match self {
            Self::DeviceAuthorization => {
                format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/device_authorization")
            }
            Self::AuthorizationToken => format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/token"),
            Self::Artist => format!("{TIDAL_API_BASE_URL}/artists/:artistId"),
            Self::FavoriteArtists => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/artists")
            }
            Self::AddFavoriteArtist => {
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

#[derive(Debug, Error)]
pub enum TidalDeviceAuthorizationError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn device_authorization(
    client_id: String,
    open: bool,
) -> Result<Value, TidalDeviceAuthorizationError> {
    let url = tidal_api_endpoint!(DeviceAuthorization);

    let params = [
        ("client_id", client_id.clone()),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = CLIENT.post(url).form(&params).send().await?.json().await?;

    let verification_uri_complete = value.to_value::<&str>("verificationUriComplete")?;
    let device_code = value.to_value::<&str>("deviceCode")?;

    let url = format!("https://{verification_uri_complete}");

    if open {
        match open::that(&url) {
            Ok(_) => {
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

#[derive(Debug, Error)]
pub enum TidalDeviceAuthorizationTokenError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn device_authorization_token(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, TidalDeviceAuthorizationTokenError> {
    let url = tidal_api_endpoint!(AuthorizationToken);

    let params = [
        ("client_id", client_id.clone()),
        ("client_secret", client_secret.clone()),
        ("device_code", device_code.clone()),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        ),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = CLIENT.post(url).form(&params).send().await?.json().await?;

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
            &**db,
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

struct TidalCredentials {
    access_token: String,
    client_id: Option<String>,
    refresh_token: Option<String>,
    #[cfg(feature = "db")]
    persist: bool,
}

#[derive(Debug, Error)]
pub enum FetchCredentialsError {
    #[cfg(feature = "db")]
    #[error(transparent)]
    TidalConfig(#[from] db::TidalConfigError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
}

async fn fetch_credentials(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    access_token: Option<String>,
) -> Result<TidalCredentials, FetchCredentialsError> {
    #[cfg(feature = "db")]
    {
        Ok(if let Some(access_token) = access_token {
            log::debug!("Using passed access_token");
            Some(Ok(TidalCredentials {
                access_token: access_token.to_string(),
                client_id: None,
                refresh_token: None,
                persist: false,
            }))
        } else {
            log::debug!("Fetching db Tidal config");

            match db::get_tidal_config(&**db).await {
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
        .ok_or(FetchCredentialsError::NoAccessTokenAvailable)??)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(TidalCredentials {
            access_token: access_token.ok_or(FetchCredentialsError::NoAccessTokenAvailable)?,
            client_id: None,
            refresh_token: None,
        })
    }
}

#[derive(Debug, Error)]
pub enum AuthenticatedRequestError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    FetchCredentials(#[from] FetchCredentialsError),
    #[error(transparent)]
    RefetchAccessToken(#[from] RefetchAccessTokenError),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Request failed (error {0})")]
    RequestFailed(u16, String),
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
    #[error("No response body")]
    NoResponseBody,
}

async fn authenticated_request(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    url: &str,
    access_token: Option<String>,
) -> Result<Value, AuthenticatedRequestError> {
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
    .ok_or_else(|| AuthenticatedRequestError::NoResponseBody)
}

async fn authenticated_post_request(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    url: &str,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Vec<(&str, &str)>>,
) -> Result<Option<Value>, AuthenticatedRequestError> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Post,
        url,
        access_token,
        body,
        form.map(|values| {
            values
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<Vec<_>>()
        }),
        1,
    )
    .await
}

async fn authenticated_delete_request(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    url: &str,
    access_token: Option<String>,
) -> Result<Option<Value>, AuthenticatedRequestError> {
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

#[derive(Clone, Copy)]
enum Method {
    Get,
    Post,
    Delete,
}

#[async_recursion]
async fn authenticated_request_inner(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    method: Method,
    url: &str,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Vec<(String, String)>>,
    attempt: u8,
) -> Result<Option<Value>, AuthenticatedRequestError> {
    if attempt > 3 {
        log::error!("Max failed attempts for reauthentication reached");
        return Err(AuthenticatedRequestError::MaxFailedAttempts);
    }

    log::debug!("Making authenticated request to {url}");

    let credentials = fetch_credentials(
        #[cfg(feature = "db")]
        db.clone(),
        access_token,
    )
    .await?;

    let mut request = match method {
        Method::Get => CLIENT.get(url),
        Method::Post => CLIENT.post(url),
        Method::Delete => CLIENT.delete(url),
    }
    .header(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", credentials.access_token),
    );

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
            if let (Some(ref client_id), Some(ref refresh_token)) =
                (credentials.client_id, credentials.refresh_token)
            {
                return authenticated_request_inner(
                    #[cfg(feature = "db")]
                    db.clone(),
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
            } else {
                log::debug!("No client_id or refresh_token available. Unauthorized");
                Err(AuthenticatedRequestError::Unauthorized)
            }
        }
        400..=599 => Err(AuthenticatedRequestError::RequestFailed(
            status,
            response.text().await.unwrap_or("".to_string()),
        )),
        _ => match response.json::<Value>().await {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if err.is_decode() {
                    Ok(None)
                } else {
                    Err(AuthenticatedRequestError::Reqwest(err))
                }
            }
        },
    }
}

#[derive(Debug, Error)]
pub enum RefetchAccessTokenError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

async fn refetch_access_token(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    client_id: &str,
    refresh_token: &str,
    #[cfg(feature = "db")] persist: bool,
) -> Result<String, RefetchAccessTokenError> {
    log::debug!("Refetching access token");
    let url = tidal_api_endpoint!(AuthorizationToken);

    let params = [
        ("client_id", client_id.to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("grant_type", "refresh_token".to_string()),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = CLIENT.post(url).form(&params).send().await?.json().await?;

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
            &**db,
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalArtistOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalArtistOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum TidalFavoriteArtistsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalArtistOrder>,
    order_direction: Option<TidalArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalArtist, TidalFavoriteArtistsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalFavoriteArtistsError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db.clone(),
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite artists response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| TidalFavoriteArtistsError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TidalFavoriteArtistsError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
                    db,
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

#[derive(Debug, Error)]
pub enum TidalAddFavoriteArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_artist(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalAddFavoriteArtistError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        AddFavoriteArtist,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("artistId", &artist_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite artist response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TidalRemoveFavoriteArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_artist(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalRemoveFavoriteArtistError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        RemoveFavoriteArtist,
        &[
            (":userId", &user_id.to_string()),
            (":artistId", &artist_id.to_string())
        ],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalAlbumOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalAlbumOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum TidalFavoriteAlbumsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalAlbum, TidalFavoriteAlbumsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalFavoriteAlbumsError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db.clone(),
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite albums response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| TidalFavoriteAlbumsError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TidalFavoriteAlbumsError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
                    db,
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

#[allow(clippy::too_many_arguments)]
pub async fn all_favorite_albums(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<Vec<TidalAlbum>, TidalFavoriteAlbumsError> {
    let mut all_albums = vec![];

    let mut offset = 0;
    let limit = 100;

    loop {
        let albums = favorite_albums(
            #[cfg(feature = "db")]
            db.clone(),
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

        if albums.is_empty() || all_albums.len() == (albums.has_more() as usize) {
            break;
        }

        offset += limit;
    }

    Ok(all_albums)
}

#[derive(Debug, Error)]
pub enum TidalAddFavoriteAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_album(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalAddFavoriteAlbumError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        AddFavoriteAlbum,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("albumId", &album_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite album response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TidalRemoveFavoriteAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_album(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalRemoveFavoriteAlbumError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        RemoveFavoriteAlbum,
        &[
            (":userId", &user_id.to_string()),
            (":albumId", &album_id.to_string())
        ],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalTrackOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalTrackOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum TidalFavoriteTracksError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalTrackOrder>,
    order_direction: Option<TidalTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<TidalTrack, TidalFavoriteTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalFavoriteTracksError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db.clone(),
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite tracks response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| TidalFavoriteTracksError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TidalFavoriteTracksError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
                    db,
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

#[derive(Debug, Error)]
pub enum TidalAddFavoriteTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_track(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalAddFavoriteTrackError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        AddFavoriteTrack,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("trackId", &track_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite track response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TidalRemoveFavoriteTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_track(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_tidal_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(TidalRemoveFavoriteTrackError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        RemoveFavoriteTrack,
        &[
            (":userId", &user_id.to_string()),
            (":trackId", &track_id.to_string())
        ],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum TidalArtistAlbumsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum TidalAlbumType {
    All,
    Lp,
    EpsAndSingles,
    Compilations,
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<TidalAlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> PagingResult<TidalAlbum, TidalArtistAlbumsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let mut query: Vec<(&str, String)> = vec![
        ("offset", offset.to_string()),
        ("limit", limit.to_string()),
        ("countryCode", country_code.clone().unwrap_or("US".into())),
        ("locale", locale.clone().unwrap_or("en_US".into())),
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
        db.clone(),
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| TidalArtistAlbumsError::RequestFailed(format!("{value:?}")))?;

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
                    db,
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

#[derive(Debug, Error)]
pub enum TidalAlbumTracksError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> PagingResult<TidalTrack, TidalAlbumTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = tidal_api_endpoint!(
        AlbumTracks,
        &[(":albumId", &album_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db.clone(),
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received album tracks response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| TidalAlbumTracksError::RequestFailed(format!("{value:?}")))?;

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
                    db,
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

#[derive(Debug, Error)]
pub enum TidalAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn album(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalAlbum, TidalAlbumError> {
    let url = tidal_api_endpoint!(
        Album,
        &[(":albumId", &album_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum TidalArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn artist(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalArtist, TidalArtistError> {
    let url = tidal_api_endpoint!(
        Artist,
        &[(":artistId", &artist_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum TidalTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalTrack, TidalTrackError> {
    let url = tidal_api_endpoint!(
        Track,
        &[(":trackId", &track_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

#[derive(Debug, Error)]
pub enum TidalTrackFileUrlError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track_file_url(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    audio_quality: TidalAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<Vec<String>, TidalTrackFileUrlError> {
    let url = tidal_api_endpoint!(
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
pub struct TidalTrackPlaybackInfo {
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

#[derive(Debug, Error)]
pub enum TidalTrackPlaybackInfoError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track_playback_info(
    #[cfg(feature = "db")] db: Arc<Box<dyn Database>>,
    audio_quality: TidalAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<TidalTrackPlaybackInfo, TidalTrackPlaybackInfoError> {
    let url = tidal_api_endpoint!(
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

impl From<ArtistOrder> for TidalArtistOrder {
    fn from(value: ArtistOrder) -> Self {
        match value {
            ArtistOrder::DateAdded => TidalArtistOrder::Date,
        }
    }
}

impl From<ArtistOrderDirection> for TidalArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => TidalArtistOrderDirection::Asc,
            ArtistOrderDirection::Descending => TidalArtistOrderDirection::Desc,
        }
    }
}

impl From<AlbumOrder> for TidalAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => TidalAlbumOrder::Date,
        }
    }
}

impl From<AlbumOrderDirection> for TidalAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => TidalAlbumOrderDirection::Asc,
            AlbumOrderDirection::Descending => TidalAlbumOrderDirection::Desc,
        }
    }
}

impl From<TrackOrder> for TidalTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => TidalTrackOrder::Date,
        }
    }
}

impl From<TrackOrderDirection> for TidalTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => TidalTrackOrderDirection::Asc,
            TrackOrderDirection::Descending => TidalTrackOrderDirection::Desc,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromAlbumTypeError {
    #[error("Unsupported AlbumType")]
    UnsupportedAlbumType,
}

impl TryFrom<AlbumType> for TidalAlbumType {
    type Error = TryFromAlbumTypeError;

    fn try_from(value: AlbumType) -> Result<Self, Self::Error> {
        match value {
            AlbumType::All => Ok(TidalAlbumType::All),
            AlbumType::Lp => Ok(TidalAlbumType::Lp),
            AlbumType::Compilations => Ok(TidalAlbumType::Compilations),
            AlbumType::EpsAndSingles => Ok(TidalAlbumType::EpsAndSingles),
            _ => Err(TryFromAlbumTypeError::UnsupportedAlbumType),
        }
    }
}

impl From<TidalFavoriteArtistsError> for ArtistsError {
    fn from(err: TidalFavoriteArtistsError) -> Self {
        ArtistsError::Other(Box::new(err))
    }
}

impl From<TidalArtistError> for ArtistError {
    fn from(err: TidalArtistError) -> Self {
        ArtistError::Other(Box::new(err))
    }
}

impl From<TidalAddFavoriteArtistError> for AddArtistError {
    fn from(err: TidalAddFavoriteArtistError) -> Self {
        AddArtistError::Other(Box::new(err))
    }
}

impl From<TidalRemoveFavoriteArtistError> for RemoveArtistError {
    fn from(err: TidalRemoveFavoriteArtistError) -> Self {
        RemoveArtistError::Other(Box::new(err))
    }
}

impl From<TidalFavoriteAlbumsError> for AlbumsError {
    fn from(err: TidalFavoriteAlbumsError) -> Self {
        AlbumsError::Other(Box::new(err))
    }
}

impl From<TidalAlbumError> for AlbumError {
    fn from(err: TidalAlbumError) -> Self {
        AlbumError::Other(Box::new(err))
    }
}

impl From<TidalArtistAlbumsError> for ArtistAlbumsError {
    fn from(err: TidalArtistAlbumsError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<TryFromAlbumTypeError> for ArtistAlbumsError {
    fn from(err: TryFromAlbumTypeError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<TidalAddFavoriteAlbumError> for AddAlbumError {
    fn from(err: TidalAddFavoriteAlbumError) -> Self {
        AddAlbumError::Other(Box::new(err))
    }
}

impl From<TidalRemoveFavoriteAlbumError> for RemoveAlbumError {
    fn from(err: TidalRemoveFavoriteAlbumError) -> Self {
        RemoveAlbumError::Other(Box::new(err))
    }
}

impl From<TidalFavoriteTracksError> for TracksError {
    fn from(err: TidalFavoriteTracksError) -> Self {
        TracksError::Other(Box::new(err))
    }
}

impl From<TidalTrackError> for TrackError {
    fn from(err: TidalTrackError) -> Self {
        TrackError::Other(Box::new(err))
    }
}

impl From<TidalAddFavoriteTrackError> for AddTrackError {
    fn from(err: TidalAddFavoriteTrackError) -> Self {
        AddTrackError::Other(Box::new(err))
    }
}

impl From<TidalRemoveFavoriteTrackError> for RemoveTrackError {
    fn from(err: TidalRemoveFavoriteTrackError) -> Self {
        RemoveTrackError::Other(Box::new(err))
    }
}

pub struct TidalMusicApi {
    #[cfg(feature = "db")]
    db: Arc<Box<dyn Database>>,
}

impl TidalMusicApi {
    #[cfg(not(feature = "db"))]
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(feature = "db")]
    pub fn new(db: Arc<Box<dyn Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl MusicApi for TidalMusicApi {
    fn source(&self) -> ApiSource {
        ApiSource::Tidal
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, ArtistsError> {
        Ok(favorite_artists(
            #[cfg(feature = "db")]
            self.db.clone(),
            offset,
            limit,
            order.map(|x| x.into()),
            order_direction.map(|x| x.into()),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .map(|x| x.into()))
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, ArtistError> {
        Ok(Some(
            artist(
                #[cfg(feature = "db")]
                self.db.clone(),
                artist_id,
                None,
                None,
                None,
                None,
            )
            .await?
            .into(),
        ))
    }

    async fn add_artist(&self, artist_id: &Id) -> Result<(), AddArtistError> {
        Ok(add_favorite_artist(
            #[cfg(feature = "db")]
            self.db.clone(),
            artist_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), RemoveArtistError> {
        Ok(remove_favorite_artist(
            #[cfg(feature = "db")]
            self.db.clone(),
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
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, AlbumsError> {
        Ok(favorite_albums(
            #[cfg(feature = "db")]
            self.db.clone(),
            offset,
            limit,
            order.map(|x| x.into()),
            order_direction.map(|x| x.into()),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .map(|x| x.into()))
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError> {
        Ok(Some(
            album(
                #[cfg(feature = "db")]
                self.db.clone(),
                album_id,
                None,
                None,
                None,
                None,
            )
            .await?
            .into(),
        ))
    }

    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: AlbumType,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        if album_type == AlbumType::All {
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
                        self.db.clone(),
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
            let album_type = album_type.try_into()?;

            return Ok(PagingResponse {
                page: Page::WithTotal {
                    items: pages
                        .into_iter()
                        .flat_map(|page| page.items())
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
                            db,
                            &artist_id,
                            Some(offset),
                            Some(limit),
                            Some(album_type),
                            None,
                            None,
                            None,
                            None,
                        )
                        .await
                    })
                }))),
            }
            .map(|item| item.into()));
        }

        Ok(artist_albums(
            #[cfg(feature = "db")]
            self.db.clone(),
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
        .map(|x| x.into()))
    }

    #[cfg(not(feature = "db"))]
    async fn library_album(
        &self,
        _album_id: &Id,
    ) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
        Err(LibraryAlbumError::NoDb)
    }

    #[cfg(feature = "db")]
    async fn library_album(
        &self,
        album_id: &Id,
    ) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
        Ok(
            moosicbox_core::sqlite::menu::get_album(&**self.db, None, Some(album_id.into()), None)
                .await
                .map_err(|err| LibraryAlbumError::Other(Box::new(err)))?,
        )
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError> {
        Ok(add_favorite_album(
            #[cfg(feature = "db")]
            self.db.clone(),
            album_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_album(&self, album_id: &Id) -> Result<(), RemoveAlbumError> {
        Ok(remove_favorite_album(
            #[cfg(feature = "db")]
            self.db.clone(),
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
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        Ok(favorite_tracks(
            #[cfg(feature = "db")]
            self.db.clone(),
            offset,
            limit,
            order.map(|x| x.into()),
            order_direction.map(|x| x.into()),
            None,
            None,
            None,
            None,
            None,
        )
        .await?
        .map(|x| x.into()))
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError> {
        Ok(Some(
            track(
                #[cfg(feature = "db")]
                self.db.clone(),
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

    async fn add_track(&self, track_id: &Id) -> Result<(), AddTrackError> {
        Ok(add_favorite_track(
            #[cfg(feature = "db")]
            self.db.clone(),
            track_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn remove_track(&self, track_id: &Id) -> Result<(), RemoveTrackError> {
        Ok(remove_favorite_track(
            #[cfg(feature = "db")]
            self.db.clone(),
            track_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }
}
