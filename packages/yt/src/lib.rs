#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

pub mod models;

use std::{fmt::Display, sync::Arc};

use models::{YtAlbum, YtArtist, YtSearchResults, YtTrack};
#[cfg(feature = "db")]
use moosicbox_database::{Database, DatabaseError};

use async_recursion::async_recursion;
use async_trait::async_trait;
use moosicbox_core::{
    sqlite::models::{Album, AlbumSort, ApiSource, Artist, AsModelResult, Id, Track},
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_files::get_content_length;
use moosicbox_json_utils::{serde_json::ToValue, ParseError};
use moosicbox_music_api::{
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumOrder, AlbumOrderDirection,
    AlbumType, AlbumsError, AlbumsRequest, ArtistAlbumsError, ArtistError, ArtistOrder,
    ArtistOrderDirection, ArtistsError, MusicApi, RemoveAlbumError, RemoveArtistError,
    RemoveTrackError, TrackAudioQuality, TrackError, TrackOrder, TrackOrderDirection, TrackSource,
    TracksError,
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

static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| reqwest::Client::builder().build().unwrap());

static YT_API_BASE_URL: &str = "https://music.youtube.com/youtubei/v1";

impl ToUrl for YtApiEndpoint {
    fn to_url(&self) -> String {
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

#[derive(Debug, Error)]
pub enum YtDeviceAuthorizationError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn device_authorization(
    client_id: String,
    open: bool,
) -> Result<Value, YtDeviceAuthorizationError> {
    let url = yt_api_endpoint!(DeviceAuthorization);

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
pub enum RequestError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Request failed (error {0})")]
    RequestFailed(u16, String),
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
    #[error("No response body")]
    NoResponseBody,
}

#[allow(unused)]
async fn request(url: &str) -> Result<Value, RequestError> {
    request_inner(Method::Get, url, None, None, 1)
        .await?
        .ok_or_else(|| RequestError::NoResponseBody)
}

async fn post_request(
    url: &str,
    body: Option<Value>,
    form: Option<Vec<(&str, &str)>>,
) -> Result<Option<Value>, RequestError> {
    request_inner(
        Method::Post,
        url,
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

#[allow(unused)]
async fn delete_request(url: &str) -> Result<Option<Value>, RequestError> {
    request_inner(Method::Delete, url, None, None, 1).await
}

#[async_recursion]
async fn request_inner(
    method: Method,
    url: &str,
    body: Option<Value>,
    form: Option<Vec<(String, String)>>,
    attempt: u8,
) -> Result<Option<Value>, RequestError> {
    if attempt > 3 {
        log::error!("Max failed attempts for request reached");
        return Err(RequestError::MaxFailedAttempts);
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
            Err(RequestError::Unauthorized)
        }
        400..=599 => Err(RequestError::RequestFailed(
            status,
            response.text().await.unwrap_or("".to_string()),
        )),
        _ => match response.json::<Value>().await {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if err.is_decode() {
                    Ok(None)
                } else {
                    Err(RequestError::Reqwest(err))
                }
            }
        },
    }
}

#[derive(Debug, Error)]
pub enum YtDeviceAuthorizationTokenError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn device_authorization_token(
    #[cfg(feature = "db")] db: &dyn Database,
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, YtDeviceAuthorizationTokenError> {
    let url = yt_api_endpoint!(AuthorizationToken);

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

#[derive(Debug, Error)]
pub enum FetchCredentialsError {
    #[cfg(feature = "db")]
    #[error(transparent)]
    YtConfig(#[from] db::YtConfigError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
}

async fn fetch_credentials(
    #[cfg(feature = "db")] db: &dyn Database,
    access_token: Option<String>,
) -> Result<YtCredentials, FetchCredentialsError> {
    #[cfg(feature = "db")]
    {
        Ok(if let Some(access_token) = access_token {
            log::debug!("Using passed access_token");
            Some(Ok(YtCredentials {
                access_token: access_token.to_string(),
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
        .ok_or(FetchCredentialsError::NoAccessTokenAvailable)??)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(YtCredentials {
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
    #[cfg(feature = "db")] db: &dyn Database,
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
    #[cfg(feature = "db")] db: &dyn Database,
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
    #[cfg(feature = "db")] db: &dyn Database,
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

#[derive(Copy, Debug, EnumString, AsRefStr, PartialEq, Clone)]
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
    #[cfg(feature = "db")] db: &dyn Database,
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
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", credentials.access_token),
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
            if let (Some(ref client_id), Some(ref refresh_token)) =
                (credentials.client_id, credentials.refresh_token)
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
    #[cfg(feature = "db")] db: &dyn Database,
    client_id: &str,
    refresh_token: &str,
    #[cfg(feature = "db")] persist: bool,
) -> Result<String, RefetchAccessTokenError> {
    log::debug!("Refetching access token");
    let url = yt_api_endpoint!(AuthorizationToken);

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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtArtistOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtArtistOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum YtFavoriteArtistsError {
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
    order: Option<YtArtistOrder>,
    order_direction: Option<YtArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtArtist, YtFavoriteArtistsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtFavoriteArtistsError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        &**db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite artists response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| YtFavoriteArtistsError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| YtFavoriteArtistsError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
pub enum YtAddFavoriteArtistError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtAddFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtAddFavoriteArtistError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteArtist,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("artistId", &artist_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite artist response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum YtRemoveFavoriteArtistError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtRemoveFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtRemoveFavoriteArtistError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtAlbumOrder {
    Date,
}

impl From<AlbumSort> for YtAlbumOrder {
    fn from(_value: AlbumSort) -> Self {
        YtAlbumOrder::Date
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtAlbumOrderDirection {
    Asc,
    Desc,
}

impl From<AlbumSort> for YtAlbumOrderDirection {
    fn from(value: AlbumSort) -> Self {
        match value {
            AlbumSort::ArtistAsc => YtAlbumOrderDirection::Asc,
            AlbumSort::ArtistDesc => YtAlbumOrderDirection::Desc,
            AlbumSort::NameAsc => YtAlbumOrderDirection::Asc,
            AlbumSort::NameDesc => YtAlbumOrderDirection::Desc,
            AlbumSort::ReleaseDateAsc => YtAlbumOrderDirection::Asc,
            AlbumSort::ReleaseDateDesc => YtAlbumOrderDirection::Desc,
            AlbumSort::DateAddedAsc => YtAlbumOrderDirection::Asc,
            AlbumSort::DateAddedDesc => YtAlbumOrderDirection::Desc,
        }
    }
}

#[derive(Debug, Error)]
pub enum YtFavoriteAlbumsError {
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
    order: Option<YtAlbumOrder>,
    order_direction: Option<YtAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtAlbum, YtFavoriteAlbumsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtFavoriteAlbumsError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        &**db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite albums response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| YtFavoriteAlbumsError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| YtFavoriteAlbumsError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
    order: Option<YtAlbumOrder>,
    order_direction: Option<YtAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<Vec<YtAlbum>, YtFavoriteAlbumsError> {
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
pub enum YtAddFavoriteAlbumError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtAddFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtAddFavoriteAlbumError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteAlbum,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("albumId", &album_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite album response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum YtRemoveFavoriteAlbumError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtRemoveFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtRemoveFavoriteAlbumError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtTrackOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtTrackOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum YtFavoriteTracksError {
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
    order: Option<YtTrackOrder>,
    order_direction: Option<YtTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> PagingResult<YtTrack, YtFavoriteTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(&**db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtFavoriteTracksError::NoUserIdAvailable)?;

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
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        &**db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received favorite tracks response: {value:?}");

    let items = value
        .to_value::<Option<Vec<&Value>>>("items")?
        .ok_or_else(|| YtFavoriteTracksError::RequestFailed(format!("{value:?}")))?
        .into_iter()
        .map(|value| value.to_value("item"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| YtFavoriteTracksError::RequestFailed(format!("{e:?}: {value:?}")))?;

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
pub enum YtAddFavoriteTrackError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtAddFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtAddFavoriteTrackError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
        AddFavoriteTrack,
        &[(":userId", &user_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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
        Some(vec![("trackId", &track_id.to_string())]),
    )
    .await?;

    log::trace!("Received add favorite track response: {value:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum YtRemoveFavoriteTrackError {
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
    #[cfg(feature = "db")] db: &dyn Database,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), YtRemoveFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = if let Some(user_id) = user_id {
        Some(user_id)
    } else {
        match db::get_yt_config(db).await {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    };

    let user_id = user_id.ok_or(YtRemoveFavoriteTrackError::NoUserIdAvailable)?;

    let url = yt_api_endpoint!(
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

#[derive(Debug, Error)]
pub enum YtArtistAlbumsError {
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
pub enum YtAlbumType {
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
    album_type: Option<YtAlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> PagingResult<YtAlbum, YtArtistAlbumsError> {
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
        &**db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| YtArtistAlbumsError::RequestFailed(format!("{value:?}")))?;

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
pub enum YtAlbumTracksError {
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
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> PagingResult<YtTrack, YtAlbumTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let url = yt_api_endpoint!(
        AlbumTracks,
        &[(":albumId", &album_id.to_string())],
        &[
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
            (
                "deviceType",
                device_type.unwrap_or(YtDeviceType::Browser).as_ref(),
            ),
        ]
    );

    let value = authenticated_request(
        #[cfg(feature = "db")]
        &**db,
        &url,
        access_token.clone(),
    )
    .await?;

    log::trace!("Received album tracks response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| YtAlbumTracksError::RequestFailed(format!("{value:?}")))?;

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
pub enum YtAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn album(
    #[cfg(feature = "db")] db: &dyn Database,
    album_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtAlbum, YtAlbumError> {
    let url = yt_api_endpoint!(
        Album,
        &[(":albumId", &album_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum YtArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn artist(
    #[cfg(feature = "db")] db: &dyn Database,
    artist_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtArtist, YtArtistError> {
    let url = yt_api_endpoint!(
        Artist,
        &[(":artistId", &artist_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum YtTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track(
    #[cfg(feature = "db")] db: &dyn Database,
    track_id: &Id,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<YtDeviceType>,
    access_token: Option<String>,
) -> Result<YtTrack, YtTrackError> {
    let url = yt_api_endpoint!(
        Track,
        &[(":trackId", &track_id.to_string())],
        &[
            ("countryCode", &country_code.clone().unwrap_or("US".into())),
            ("locale", &locale.clone().unwrap_or("en_US".into())),
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

#[derive(Debug, Error)]
pub enum YtSearchError {
    #[error(transparent)]
    Request(#[from] RequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("Empty response")]
    EmptyResponse,
}

#[allow(clippy::too_many_arguments)]
pub async fn search(
    query: &str,
    _offset: Option<usize>,
    _limit: Option<usize>,
) -> Result<YtSearchResults, YtSearchError> {
    let url = yt_api_endpoint!(Search, &[], &[("prettyPrint", &false.to_string()),]);

    let date = chrono::Local::now();

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
    .ok_or(YtSearchError::EmptyResponse)?;

    log::trace!("Received search response: {value:?}");

    Ok(value.as_model()?)
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum YtAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

impl From<TrackAudioQuality> for YtAudioQuality {
    fn from(value: TrackAudioQuality) -> Self {
        match value {
            TrackAudioQuality::Low => YtAudioQuality::High,
            TrackAudioQuality::FlacLossless => YtAudioQuality::Lossless,
            TrackAudioQuality::FlacHiRes => YtAudioQuality::HiResLossless,
            TrackAudioQuality::FlacHighestRes => YtAudioQuality::HiResLossless,
        }
    }
}

#[derive(Debug, Error)]
pub enum YtTrackFileUrlError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track_file_url(
    #[cfg(feature = "db")] db: &dyn Database,
    audio_quality: YtAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<Vec<String>, YtTrackFileUrlError> {
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

#[derive(Debug, Error)]
pub enum YtTrackPlaybackInfoError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track_playback_info(
    #[cfg(feature = "db")] db: &dyn Database,
    audio_quality: YtAudioQuality,
    track_id: &Id,
    access_token: Option<String>,
) -> Result<YtTrackPlaybackInfo, YtTrackPlaybackInfoError> {
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
            ArtistOrder::DateAdded => YtArtistOrder::Date,
        }
    }
}

impl From<ArtistOrderDirection> for YtArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => YtArtistOrderDirection::Asc,
            ArtistOrderDirection::Descending => YtArtistOrderDirection::Desc,
        }
    }
}

impl From<AlbumOrder> for YtAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => YtAlbumOrder::Date,
        }
    }
}

impl From<AlbumOrderDirection> for YtAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => YtAlbumOrderDirection::Asc,
            AlbumOrderDirection::Descending => YtAlbumOrderDirection::Desc,
        }
    }
}

impl From<TrackOrder> for YtTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => YtTrackOrder::Date,
        }
    }
}

impl From<TrackOrderDirection> for YtTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => YtTrackOrderDirection::Asc,
            TrackOrderDirection::Descending => YtTrackOrderDirection::Desc,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromAlbumTypeError {
    #[error("Unsupported AlbumType")]
    UnsupportedAlbumType,
}

impl TryFrom<AlbumType> for YtAlbumType {
    type Error = TryFromAlbumTypeError;

    fn try_from(value: AlbumType) -> Result<Self, Self::Error> {
        match value {
            AlbumType::All => Ok(YtAlbumType::All),
            AlbumType::Lp => Ok(YtAlbumType::Lp),
            AlbumType::Compilations => Ok(YtAlbumType::Compilations),
            AlbumType::EpsAndSingles => Ok(YtAlbumType::EpsAndSingles),
            _ => Err(TryFromAlbumTypeError::UnsupportedAlbumType),
        }
    }
}

impl From<YtFavoriteArtistsError> for ArtistsError {
    fn from(err: YtFavoriteArtistsError) -> Self {
        ArtistsError::Other(Box::new(err))
    }
}

impl From<YtArtistError> for ArtistError {
    fn from(err: YtArtistError) -> Self {
        ArtistError::Other(Box::new(err))
    }
}

impl From<YtAddFavoriteArtistError> for AddArtistError {
    fn from(err: YtAddFavoriteArtistError) -> Self {
        AddArtistError::Other(Box::new(err))
    }
}

impl From<YtRemoveFavoriteArtistError> for RemoveArtistError {
    fn from(err: YtRemoveFavoriteArtistError) -> Self {
        RemoveArtistError::Other(Box::new(err))
    }
}

impl From<YtFavoriteAlbumsError> for AlbumsError {
    fn from(err: YtFavoriteAlbumsError) -> Self {
        AlbumsError::Other(Box::new(err))
    }
}

impl From<YtAlbumError> for AlbumError {
    fn from(err: YtAlbumError) -> Self {
        AlbumError::Other(Box::new(err))
    }
}

impl From<YtArtistAlbumsError> for ArtistAlbumsError {
    fn from(err: YtArtistAlbumsError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<TryFromAlbumTypeError> for ArtistAlbumsError {
    fn from(err: TryFromAlbumTypeError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<YtAddFavoriteAlbumError> for AddAlbumError {
    fn from(err: YtAddFavoriteAlbumError) -> Self {
        AddAlbumError::Other(Box::new(err))
    }
}

impl From<YtRemoveFavoriteAlbumError> for RemoveAlbumError {
    fn from(err: YtRemoveFavoriteAlbumError) -> Self {
        RemoveAlbumError::Other(Box::new(err))
    }
}

impl From<YtFavoriteTracksError> for TracksError {
    fn from(err: YtFavoriteTracksError) -> Self {
        TracksError::Other(Box::new(err))
    }
}

impl From<YtAlbumTracksError> for TracksError {
    fn from(err: YtAlbumTracksError) -> Self {
        TracksError::Other(Box::new(err))
    }
}

impl From<YtTrackError> for TrackError {
    fn from(err: YtTrackError) -> Self {
        TrackError::Other(Box::new(err))
    }
}

impl From<YtTrackFileUrlError> for TrackError {
    fn from(err: YtTrackFileUrlError) -> Self {
        TrackError::Other(Box::new(err))
    }
}

impl From<YtAddFavoriteTrackError> for AddTrackError {
    fn from(err: YtAddFavoriteTrackError) -> Self {
        AddTrackError::Other(Box::new(err))
    }
}

impl From<YtRemoveFavoriteTrackError> for RemoveTrackError {
    fn from(err: YtRemoveFavoriteTrackError) -> Self {
        RemoveTrackError::Other(Box::new(err))
    }
}

pub struct YtMusicApi {
    #[cfg(feature = "db")]
    db: Arc<Box<dyn Database>>,
}

impl YtMusicApi {
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
impl MusicApi for YtMusicApi {
    fn source(&self) -> ApiSource {
        ApiSource::Yt
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
                &**self.db,
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
            &**self.db,
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
            &**self.db,
            artist_id,
            None,
            None,
            None,
            None,
            None,
        )
        .await?)
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, AlbumsError> {
        Ok(favorite_albums(
            #[cfg(feature = "db")]
            self.db.clone(),
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
        .map(|x| x.into()))
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError> {
        Ok(Some(
            album(
                #[cfg(feature = "db")]
                &**self.db,
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
                    YtAlbumType::Lp,
                    YtAlbumType::EpsAndSingles,
                    YtAlbumType::Compilations,
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

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError> {
        Ok(add_favorite_album(
            #[cfg(feature = "db")]
            &**self.db,
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
            &**self.db,
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
    ) -> PagingResult<Track, TracksError> {
        moosicbox_assert::assert_or_unimplemented!(
            track_ids.is_none(),
            "Fetching specific tracks by id is not implemented yet"
        );

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

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        Ok(album_tracks(
            #[cfg(feature = "db")]
            self.db.clone(),
            album_id,
            offset,
            limit,
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
                &**self.db,
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
            &**self.db,
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
            &**self.db,
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
        track: &Track,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError> {
        Ok(track_file_url(
            #[cfg(feature = "db")]
            &**self.db,
            quality.into(),
            &track.id,
            None,
        )
        .await?
        .first()
        .map(|x| x.to_string())
        .map(|url| TrackSource::RemoteUrl {
            url,
            format: track.format.unwrap_or(AudioFormat::Source),
            track_id: Some(track.id.to_owned()),
            source: track.source,
        }))
    }

    async fn track_size(
        &self,
        track_id: &Id,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, TrackError> {
        let url = if let Some(url) = track_file_url(
            #[cfg(feature = "db")]
            &**self.db,
            YtAudioQuality::High,
            track_id,
            None,
        )
        .await?
        .into_iter()
        .next()
        {
            url
        } else {
            return Ok(None);
        };

        Ok(get_content_length(&url, None, None)
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?)
    }
}
