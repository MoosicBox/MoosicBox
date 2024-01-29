#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use std::{collections::HashMap, str::Utf8Error};

use async_recursion::async_recursion;
use base64::{engine::general_purpose, Engine as _};
use moosicbox_core::sqlite::models::AsModelResult;
use moosicbox_json_utils::{
    serde_json::{ToNestedValue, ToValue},
    MissingValue, ParseError, ToValueType,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use url::form_urlencoded;

static AUTH_HEADER_NAME: &str = "x-user-auth-token";
static APP_ID_HEADER_NAME: &str = "x-app-id";

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzDeviceType {
    Browser,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzImage {
    pub thumbnail: Option<String>,
    pub small: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
    pub extralarge: Option<String>,
    pub mega: Option<String>,
}

impl QobuzImage {
    pub fn cover_url(&self) -> Option<String> {
        self.mega
            .clone()
            .or(self.extralarge.clone())
            .or(self.large.clone())
            .or(self.medium.clone())
            .or(self.small.clone())
            .or(self.thumbnail.clone())
    }
}

impl MissingValue<QobuzImage> for &Value {}
impl ToValueType<QobuzImage> for &Value {
    fn to_value_type(self) -> Result<QobuzImage, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzImage, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzImage, ParseError> {
        Ok(QobuzImage {
            thumbnail: self.to_value("thumbnail")?,
            small: self.to_value("small")?,
            medium: self.to_value("medium")?,
            large: self.to_value("large")?,
            extralarge: self.to_value("extralarge")?,
            mega: self.to_value("mega")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzGenre {
    pub id: u64,
    pub name: String,
    pub slug: String,
}

impl MissingValue<QobuzGenre> for &Value {}
impl ToValueType<QobuzGenre> for &Value {
    fn to_value_type(self) -> Result<QobuzGenre, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzGenre, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzGenre, ParseError> {
        Ok(QobuzGenre {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            slug: self.to_value("slug")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub qobuz_id: u64,
    pub released_at: u64,
    pub release_date_original: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub popularity: u32,
    pub tracks_count: u32,
    pub genre: QobuzGenre,
    pub maximum_channel_count: u16,
    pub maximum_sampling_rate: f32,
}

impl QobuzAlbum {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl MissingValue<QobuzAlbum> for &Value {}
impl ToValueType<QobuzAlbum> for &Value {
    fn to_value_type(self) -> Result<QobuzAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzAlbum, ParseError> {
        Ok(QobuzAlbum {
            id: self.to_value("id")?,
            artist: self
                .to_nested_value::<String>(&["artist", "name"])
                .or_else(|_| self.to_nested_value(&["artist", "name", "display"]))?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            maximum_bit_depth: self
                .to_value("maximum_bit_depth")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_bit_depth"]))?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            qobuz_id: self.to_value("qobuz_id")?,
            released_at: self.to_value("released_at")?,
            release_date_original: self.to_value("release_date_original")?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            popularity: self.to_value("popularity")?,
            tracks_count: self.to_value("tracks_count")?,
            genre: self.to_value("genre")?,
            maximum_channel_count: self
                .to_value("maximum_channel_count")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_channel_count"]))?,
            maximum_sampling_rate: self
                .to_value("maximum_sampling_rate")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_sampling_rate"]))?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzRelease {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub release_date_original: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub tracks_count: u32,
    pub genre: String,
    pub maximum_channel_count: u16,
    pub maximum_sampling_rate: f32,
}

impl QobuzRelease {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl MissingValue<QobuzRelease> for &Value {}
impl ToValueType<QobuzRelease> for &Value {
    fn to_value_type(self) -> Result<QobuzRelease, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzRelease, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzRelease, ParseError> {
        Ok(QobuzRelease {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name", "display"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            maximum_bit_depth: self.to_nested_value(&["audio_info", "maximum_bit_depth"])?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            release_date_original: self.to_nested_value(&["dates", "original"])?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            tracks_count: self.to_value("tracks_count")?,
            genre: self.to_nested_value(&["genre", "name"])?,
            maximum_channel_count: self
                .to_nested_value(&["audio_info", "maximum_channel_count"])?,
            maximum_sampling_rate: self
                .to_nested_value(&["audio_info", "maximum_sampling_rate"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct QobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub artist: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: String,
    pub image: Option<QobuzImage>,
    pub copyright: Option<String>,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub title: String,
}

impl QobuzTrack {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl MissingValue<QobuzTrack> for &Value {}
impl ToValueType<QobuzTrack> for &Value {
    fn to_value_type(self) -> Result<QobuzTrack, ParseError> {
        self.as_model()
    }
}

impl QobuzTrack {
    fn from_value(
        value: &Value,
        artist: &str,
        artist_id: u64,
        album: &str,
        album_id: &str,
        image: Option<QobuzImage>,
    ) -> Result<QobuzTrack, ParseError> {
        Ok(QobuzTrack {
            id: value.to_value("id")?,
            track_number: value.to_value("track_number")?,
            artist: artist.to_string(),
            artist_id,
            album: album.to_string(),
            album_id: album_id.to_string(),
            image,
            copyright: value.to_value("copyright")?,
            duration: value.to_value("duration")?,
            parental_warning: value.to_value("parental_warning")?,
            isrc: value.to_value("isrc")?,
            title: value.to_value("title")?,
        })
    }
}

impl AsModelResult<QobuzTrack, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzTrack, ParseError> {
        Ok(QobuzTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("track_number")?,
            album: self.to_nested_value(&["album", "title"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            artist: self.to_nested_value(&["album", "artist", "name"])?,
            artist_id: self.to_nested_value(&["album", "artist", "id"])?,
            image: self.to_value("image")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            isrc: self.to_value("isrc")?,
            title: self.to_value("title")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtist {
    pub id: u64,
    pub image: Option<QobuzImage>,
    pub name: String,
}

impl QobuzArtist {
    pub fn cover_url(&self) -> Option<String> {
        self.image.clone().and_then(|image| {
            image
                .mega
                .or(image.extralarge)
                .or(image.large)
                .or(image.medium)
                .or(image.small)
                .or(image.thumbnail)
        })
    }
}

impl MissingValue<QobuzArtist> for &Value {}
impl ToValueType<QobuzArtist> for &Value {
    fn to_value_type(self) -> Result<QobuzArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzArtist, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzArtist, ParseError> {
        Ok(QobuzArtist {
            id: self.to_value("id")?,
            image: self.to_value("image")?,
            name: self.to_value("name")?,
        })
    }
}

trait ToUrl {
    fn to_url(&self) -> String;
}

static QOBUZ_PLAY_API_BASE_URL: &str = "https://play.qobuz.com";
static QOBUZ_API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[derive(Clone)]
struct QobuzCredentials {
    access_token: String,
    app_id: Option<String>,
    username: Option<String>,
    #[cfg(feature = "db")]
    persist: bool,
}

#[derive(Debug, Error)]
pub enum FetchCredentialsError {
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
}

fn fetch_credentials(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<QobuzCredentials, FetchCredentialsError> {
    #[cfg(feature = "db")]
    {
        access_token
            .map(|token| {
                log::debug!("Using passed access_token");
                Ok(QobuzCredentials {
                    access_token: token,
                    app_id: None,
                    username: None,
                    persist: false,
                })
            })
            .or_else(|| {
                log::debug!("Fetching db Qobuz config");

                let db = &db.library.lock().unwrap().inner;

                match db::get_qobuz_config(db) {
                    Ok(Some(config)) => {
                        log::debug!("Using db Qobuz config");
                        log::debug!("Fetching db Qobuz app config");
                        match db::get_qobuz_app_config(db) {
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
            })
            .transpose()?
            .ok_or(FetchCredentialsError::NoAccessTokenAvailable)
    }

    #[cfg(not(feature = "db"))]
    {
        Ok(QobuzCredentials {
            access_token: access_token.ok_or(FetchCredentialsError::NoAccessTokenAvailable)?,
            app_id,
            username: None,
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
    #[error("No response body")]
    NoResponseBody,
    #[error("MaxFailedAttempts")]
    MaxFailedAttempts,
}

async fn authenticated_request(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<Value, AuthenticatedRequestError> {
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
    .ok_or_else(|| AuthenticatedRequestError::NoResponseBody)
}

#[allow(unused)]
async fn authenticated_post_request(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
    body: Option<Value>,
    form: Option<Vec<(&str, &str)>>,
) -> Result<Option<Value>, AuthenticatedRequestError> {
    authenticated_request_inner(
        #[cfg(feature = "db")]
        db,
        Method::Post,
        url,
        app_id,
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

#[allow(unused)]
async fn authenticated_delete_request(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    url: &str,
    app_id: Option<String>,
    access_token: Option<String>,
) -> Result<Option<Value>, AuthenticatedRequestError> {
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

#[derive(Clone, Copy)]
enum Method {
    Get,
    Post,
    Delete,
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
async fn authenticated_request_inner(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    method: Method,
    url: &str,
    app_id: Option<String>,
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
        app_id,
        access_token,
    )?;

    let app_id = if let Some(ref app_id) = credentials.app_id {
        app_id
    } else {
        log::debug!("No app_id available");
        return Err(AuthenticatedRequestError::Unauthorized);
    };

    let mut request = match method {
        Method::Get => CLIENT.get(url),
        Method::Post => CLIENT.post(url),
        Method::Delete => CLIENT.delete(url),
    }
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

            let username = if let Some(ref username) = credentials.username {
                username
            } else {
                return Err(AuthenticatedRequestError::Unauthorized);
            };

            return authenticated_request_inner(
                #[cfg(feature = "db")]
                db,
                method,
                url,
                Some(app_id.to_string()),
                Some(
                    refetch_access_token(
                        #[cfg(feature = "db")]
                        db,
                        app_id,
                        username,
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
        400..=599 => Err(AuthenticatedRequestError::RequestFailed(
            status,
            response.text().await.unwrap_or("".to_string()),
        )),
        _ => match response.json::<Value>().await {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                log::debug!("JSON response error: {err:?}");
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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

async fn refetch_access_token(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    app_id: &str,
    username: &str,
    access_token: &str,
    #[cfg(feature = "db")] persist: bool,
) -> Result<String, RefetchAccessTokenError> {
    log::debug!("Refetching access token");
    let url = qobuz_api_endpoint!(
        UserLogin,
        &[],
        &[("username", username), ("user_auth_token", access_token)]
    );

    let value: Value = CLIENT
        .post(url)
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

        db::create_qobuz_config(
            &db.library.lock().as_ref().unwrap().inner,
            access_token,
            user_id,
            user_email,
            user_public_id,
        )?;
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

#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum QobuzAlbumReleaseType {
    #[default]
    All,
    Album,
    Live,
    Compilation,
    EpSingle,
    Other,
    Download,
}

#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum QobuzAlbumSort {
    ReleaseDate,
    Relevant,
    #[default]
    ReleaseDateByPriority,
}

#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum QobuzAlbumOrder {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Error)]
pub enum QobuzUserLoginError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("No app id available")]
    NoAppIdAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    QobuzFetchLoginSource(#[from] QobuzFetchLoginSourceError),
    #[error(transparent)]
    QobuzFetchBundleSource(#[from] QobuzFetchBundleSourceError),
    #[error(transparent)]
    QobuzFetchAppSecrets(#[from] QobuzFetchAppSecretsError),
    #[error("Failed to fetch app id")]
    FailedToFetchAppId,
}

pub async fn user_login(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    username: &str,
    password: &str,
    app_id: Option<String>,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, QobuzUserLoginError> {
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
                    let db_connection = &db.library.lock().unwrap().inner;
                    db::get_qobuz_app_config(db_connection)?
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
                let bundle_version = search_bundle_version(&login_source)
                    .await
                    .ok_or(QobuzUserLoginError::FailedToFetchAppId)?;
                let bundle = fetch_bundle_source(&bundle_version).await?;
                let config = search_app_config(&bundle).await?;

                #[cfg(feature = "db")]
                {
                    log::debug!(
                        "Creating Qobuz app config: bundle_version={bundle_version} app_id={}",
                        config.app_id
                    );
                    let db_connection = &db.library.lock().unwrap().inner;
                    let app_config = db::create_qobuz_app_config(
                        db_connection,
                        &bundle_version,
                        &config.app_id,
                    )?;

                    for (timezone, secret) in config.secrets {
                        log::debug!("Creating Qobuz app secret: timezone={bundle_version}");
                        db::create_qobuz_app_secret(
                            db_connection,
                            app_config.id,
                            &timezone,
                            &secret,
                        )?;
                    }
                }

                config.app_id
            }
        }
    };

    let value: Value = CLIENT
        .post(url)
        .header(APP_ID_HEADER_NAME, app_id)
        .send()
        .await?
        .json()
        .await?;

    let access_token: &str = value.to_value("user_auth_token")?;
    let user_id: u64 = value.to_nested_value(&["user", "id"])?;
    let user_email: &str = value.to_nested_value(&["user", "email"])?;
    let user_public_id: &str = value.to_nested_value(&["user", "publicId"])?;

    #[cfg(feature = "db")]
    if persist.unwrap_or(false) {
        db::create_qobuz_config(
            &db.library.lock().as_ref().unwrap().inner,
            access_token,
            user_id,
            user_email,
            user_public_id,
        )?;
    }

    Ok(serde_json::json!({
        "accessToken": access_token,
        "userId": user_id,
        "userEmail": user_email,
        "userPublicId": user_public_id,
    }))
}

#[derive(Debug, Error)]
pub enum QobuzArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn artist(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzArtist, QobuzArtistError> {
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

    let artist = value.to_value_type()?;

    Ok(artist)
}

#[derive(Debug, Error)]
pub enum QobuzFavoriteArtistsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(Vec<QobuzArtist>, u32), QobuzFavoriteArtistsError> {
    let url = qobuz_api_endpoint!(
        Favorites,
        &[],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
            ("type", "artists"),
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

    log::trace!("Received favorite artists response: {value:?}");

    let items = value.to_nested_value(&["artists", "items"])?;
    let count = value.to_nested_value(&["artists", "total"])?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum QobuzAddFavoriteArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn add_favorite_artist(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzAddFavoriteArtistError> {
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

#[derive(Debug, Error)]
pub enum QobuzRemoveFavoriteArtistError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn remove_favorite_artist(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzRemoveFavoriteArtistError> {
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

#[derive(Debug, Error)]
pub enum QobuzArtistAlbumsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    release_type: Option<QobuzAlbumReleaseType>,
    sort: Option<QobuzAlbumSort>,
    order: Option<QobuzAlbumOrder>,
    track_size: Option<u8>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(Vec<QobuzRelease>, bool), QobuzArtistAlbumsError> {
    let url = qobuz_api_endpoint!(
        ArtistAlbums,
        &[],
        &[
            ("artist_id", &artist_id.to_string()),
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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
        app_id,
        access_token,
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value.to_value("items")?;
    let has_more = value.to_value("has_more")?;

    Ok((items, has_more))
}

#[derive(Debug, Error)]
pub enum QobuzAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn album(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: &str,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzAlbum, QobuzAlbumError> {
    let url = qobuz_api_endpoint!(Album, &[], &[("album_id", album_id), ("limit", "0")]);

    let value = authenticated_request(
        #[cfg(feature = "db")]
        db,
        &url,
        app_id,
        access_token,
    )
    .await?;

    let album = value.to_value_type()?;

    Ok(album)
}

#[derive(Debug, Error)]
pub enum QobuzFavoriteAlbumsError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(Vec<QobuzAlbum>, u32), QobuzFavoriteAlbumsError> {
    let url = qobuz_api_endpoint!(
        Favorites,
        &[],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
            ("type", "albums"),
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

    let items = value.to_nested_value(&["albums", "items"])?;
    let count = value.to_nested_value(&["albums", "total"])?;

    Ok((items, count))
}

pub async fn all_favorite_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<Vec<QobuzAlbum>, QobuzFavoriteAlbumsError> {
    let mut all_albums = vec![];

    let mut offset = 0;
    let limit = 100;

    loop {
        let albums = favorite_albums(
            #[cfg(feature = "db")]
            db,
            Some(offset),
            Some(limit),
            access_token.clone(),
            app_id.clone(),
        )
        .await?;

        all_albums.extend_from_slice(&albums.0);

        if albums.0.is_empty() || all_albums.len() == (albums.1 as usize) {
            break;
        }

        offset += limit;
    }

    Ok(all_albums)
}

#[derive(Debug, Error)]
pub enum QobuzAddFavoriteAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn add_favorite_album(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: &str,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzAddFavoriteAlbumError> {
    let url = qobuz_api_endpoint!(AddFavorites, &[], &[("album_ids", album_id),]);

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

#[derive(Debug, Error)]
pub enum QobuzRemoveFavoriteAlbumError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn remove_favorite_album(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: &str,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzRemoveFavoriteAlbumError> {
    let url = qobuz_api_endpoint!(RemoveFavorites, &[], &[("album_ids", album_id),]);

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

#[derive(Debug, Error)]
pub enum QobuzAlbumTracksError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: &str,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(Vec<QobuzTrack>, u32), QobuzAlbumTracksError> {
    let url = qobuz_api_endpoint!(
        Album,
        &[],
        &[
            ("album_id", album_id),
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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

    log::trace!("Received album tracks response: {value:?}");

    let artist = value.to_nested_value(&["artist", "name"])?;
    let artist_id = value.to_nested_value(&["artist", "id"])?;
    let album = value.to_value("title")?;
    let image: Option<QobuzImage> = value.to_value("image")?;
    let items = value
        .to_nested_value::<Vec<&Value>>(&["tracks", "items"])?
        .iter()
        .map(move |value| {
            QobuzTrack::from_value(value, artist, artist_id, album, album_id, image.clone())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = value.to_nested_value(&["tracks", "total"])?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum QobuzTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn track(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<QobuzTrack, QobuzTrackError> {
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

    let track = value.to_value_type()?;

    Ok(track)
}

#[derive(Debug, Error)]
pub enum QobuzFavoriteTracksError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(Vec<QobuzTrack>, u32), QobuzFavoriteTracksError> {
    let url = qobuz_api_endpoint!(
        Favorites,
        &[],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
            ("type", "tracks"),
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

    let items = value.to_nested_value(&["tracks", "items"])?;
    let count = value.to_nested_value(&["tracks", "total"])?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum QobuzAddFavoriteTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn add_favorite_track(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzAddFavoriteTrackError> {
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

#[derive(Debug, Error)]
pub enum QobuzRemoveFavoriteTrackError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
}

pub async fn remove_favorite_track(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    access_token: Option<String>,
    app_id: Option<String>,
) -> Result<(), QobuzRemoveFavoriteTrackError> {
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzAudioQuality {
    Low,
    FlacLossless,
    FlacHiRes,
    FlacHighestRes,
}

impl QobuzAudioQuality {
    fn as_format_id(&self) -> u8 {
        match self {
            QobuzAudioQuality::Low => 5,
            QobuzAudioQuality::FlacLossless => 6,
            QobuzAudioQuality::FlacHiRes => 7,
            QobuzAudioQuality::FlacHighestRes => 27,
        }
    }
}

#[derive(Debug, Error)]
pub enum QobuzTrackFileUrlError {
    #[error(transparent)]
    AuthenticatedRequest(#[from] AuthenticatedRequestError),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No app secret available")]
    NoAppSecretAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn track_file_url(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    quality: QobuzAudioQuality,
    access_token: Option<String>,
    app_id: Option<String>,
    app_secret: Option<String>,
) -> Result<String, QobuzTrackFileUrlError> {
    #[cfg(feature = "db")]
    let app_secret = match app_secret {
        Some(app_secret) => app_secret,
        _ => {
            let app_secrets =
                db::get_qobuz_app_secrets(&db.library.lock().as_ref().unwrap().inner)?;
            let app_secrets = app_secrets
                .iter()
                .find(|secret| secret.timezone == "berlin")
                .or_else(|| app_secrets.first())
                .ok_or(QobuzTrackFileUrlError::NoAppSecretAvailable)?;

            app_secrets.secret.clone()
        }
    };

    #[cfg(not(feature = "db"))]
    let app_secret = app_secret.ok_or(QobuzTrackFileUrlError::NoAppSecretAvailable)?;

    let intent = "stream";
    let format_id = quality.as_format_id();
    let request_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let request_sig = format!("trackgetFileUrlformat_id{format_id}intent{intent}track_id{track_id}{request_ts}{app_secret}");
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

#[derive(Debug, Error)]
pub enum QobuzFetchLoginSourceError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
}

#[allow(unused)]
async fn fetch_login_source() -> Result<String, QobuzFetchLoginSourceError> {
    let url = qobuz_api_endpoint!(Login);

    Ok(CLIENT.get(url).send().await?.text().await?)
}

#[allow(unused)]
async fn search_bundle_version(login_source: &str) -> Option<String> {
    static BUNDLE_ID_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(
            r#"<script src="/resources/(\d+\.\d+\.\d+-[a-z]\d{3})/bundle\.js"></script>"#,
        )
        .unwrap()
    });

    if let Some(caps) = BUNDLE_ID_REGEX.captures(login_source) {
        if let Some(version) = caps.get(1) {
            let version = version.as_str();
            log::debug!("Found version={version}");
            return Some(version.to_string());
        }
    }

    None
}

#[derive(Debug, Error)]
pub enum QobuzFetchBundleSourceError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
}

#[allow(unused)]
async fn fetch_bundle_source(bundle_version: &str) -> Result<String, QobuzFetchBundleSourceError> {
    let url = qobuz_api_endpoint!(Bundle, &[(":bundleVersion", bundle_version)]);

    Ok(CLIENT.get(url).send().await?.text().await?)
}

#[derive(Debug, Error)]
pub enum QobuzFetchAppSecretsError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No App ID found in output")]
    NoAppId,
    #[error("No seed and timezone found in output")]
    NoSeedAndTimezone,
    #[error("No info and extras found in output")]
    NoInfoAndExtras,
    #[error("No matching info for timezone")]
    NoMatchingInfoForTimezone,
    #[error(transparent)]
    Utf8(#[from] Utf8Error),
}

fn capitalize(value: &str) -> String {
    let mut v: Vec<char> = value.chars().collect();
    v[0] = v[0].to_uppercase().next().unwrap();
    v.into_iter().collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppConfig {
    pub(crate) app_id: String,
    pub(crate) secrets: HashMap<String, String>,
}

#[allow(unused)]
pub(crate) async fn search_app_config(
    bundle: &str,
) -> Result<AppConfig, QobuzFetchAppSecretsError> {
    static APP_ID_REGEX: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r#"production:\{api:\{appId:"([^"]+)""#).unwrap());

    let app_id = if let Some(caps) = APP_ID_REGEX.captures(bundle) {
        if let Some(app_id) = caps.get(1) {
            let app_id = app_id.as_str();
            log::debug!("Found app_id={app_id}");
            app_id.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoAppId);
        }
    } else {
        return Err(QobuzFetchAppSecretsError::NoAppId);
    };

    let mut seed_timezones = vec![];

    static SEED_AND_TIMEZONE_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r#"[a-z]\.initialSeed\("([\w=]+)",window\.utimezone\.(.+?)\)"#).unwrap()
    });

    for caps in SEED_AND_TIMEZONE_REGEX.captures_iter(bundle) {
        let seed = if let Some(seed) = caps.get(1) {
            let seed = seed.as_str();
            log::debug!("Found seed={seed}");
            seed.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoSeedAndTimezone);
        };
        let timezone = if let Some(timezone) = caps.get(2) {
            let timezone = timezone.as_str();
            log::debug!("Found timezone={timezone}");
            timezone.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoSeedAndTimezone);
        };

        seed_timezones.push((seed, timezone));
    }

    if seed_timezones.is_empty() {
        return Err(QobuzFetchAppSecretsError::NoSeedAndTimezone);
    };

    let mut name_info_extras = vec![];

    static INFO_AND_EXTRAS_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r#"name:"\w+/([^"]+)",info:"([\w=]+)",extras:"([\w=]+)""#).unwrap()
    });

    for caps in INFO_AND_EXTRAS_REGEX.captures_iter(bundle) {
        let name = if let Some(name) = caps.get(1) {
            let name = name.as_str();
            log::debug!("Found name={name}");
            name.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoInfoAndExtras);
        };
        let info = if let Some(info) = caps.get(2) {
            let info = info.as_str();
            log::debug!("Found info={info}");
            info.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoInfoAndExtras);
        };
        let extras = if let Some(extras) = caps.get(3) {
            let extras = extras.as_str();
            log::debug!("Found extras={extras}");
            extras.to_string()
        } else {
            return Err(QobuzFetchAppSecretsError::NoInfoAndExtras);
        };

        name_info_extras.push((name, info, extras));
    }

    if name_info_extras.is_empty() {
        return Err(QobuzFetchAppSecretsError::NoInfoAndExtras);
    };

    let mut secrets = HashMap::new();

    log::trace!("seed_timezones={:?}", &seed_timezones);
    for (seed, timezone) in seed_timezones {
        log::trace!("name_info_extras={:?}", &name_info_extras);
        let (_, info, _) = name_info_extras
            .iter()
            .find(|(name, _, _)| name.starts_with(&capitalize(&timezone)))
            .ok_or(QobuzFetchAppSecretsError::NoMatchingInfoForTimezone)
            .expect("No matching name for timezone");

        let secret_base64 = format!("{seed}{info}");
        let secret_base64 = &secret_base64[..44];
        let secret = general_purpose::STANDARD.decode(secret_base64)?;
        let secret = std::str::from_utf8(&secret)?.to_string();

        secrets.insert(timezone, secret);
    }

    Ok(AppConfig { app_id, secrets })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::*;

    static TEST_LOGIN_SOURCE: &str = r#"</script>
        <script src="/resources/7.1.3-b011/bundle.js"></script>
        </body>"#;
    static TEST_BUNDLE_SOURCE: &str = r#"s,extra:o},production:{api:{appId:"123456789",appSecret{var e=window.__ENVIRONMENT__;return"recette"===e?d.initialSeed("YjBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.london):"integration"===e?d.initialSeed("MjBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.algier):d.initialSeed("MzBiMGIwYmQzYWRiMzNmY2Q2YTc0MD",window.utimezone.berlin)},d.string{offset:"GMT",name:"Europe/Dublin",info:"XXXXX",extras:"XXXXX"},{offset:"GMT",name:"Europe/Lisbon"},{offset:"GMT",name:"Europe/London",info:"VmMjU1NTU1NTU=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"UTC",name:"UTC"},{offset:"GMT+01:00",name:"Africa/Algiers",info:"VmMjU1NTU1NTI=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"GMT+01:00",name:"Africa/Windhoek"},{offset:"GMT+01:00",name:"Atlantic/Azores"},{offset:"GMT+01:00",name:"Atlantic/Stanley"},{offset:"GMT+01:00",name:"Europe/Amsterdam"},{offset:"GMT+01:00",name:"Europe/Paris",info:"XXXXX",extras:"XXXXX"},{offset:"GMT+01:00",name:"Europe/Belgrade"},{offset:"GMT+01:00",name:"Europe/Brussels"},{offset:"GMT+02:00",name:"Africa/Cairo"},{offset:"GMT+02:00",name:"Africa/Blantyre"},{offset:"GMT+02:00",name:"Asia/Beirut"},{offset:"GMT+02:00",name:"Asia/Damascus"},{offset:"GMT+02:00",name:"Asia/Gaza"},{offset:"GMT+02:00",name:"Asia/Jerusalem"},{offset:"GMT+02:00",name:"Europe/Berlin",info:"VmMjU1NTU1NTM=YjBiMGIwYmQzMzMz",extras:"MzMzMzMzMzMzMDVmMjU4OTA1NTU="},{offset:"GMT+03:00",name:"Africa/Addis_Ababa"},{offset:"GMT+03:00",name:"Asia/Riyadh89"},{offset:"GMT+03:00",name:"Europe/Minsk"},{offset:"GMT+03:30""#;

    #[tokio::test]
    async fn test_search_bundle_version() {
        let version = search_bundle_version(TEST_LOGIN_SOURCE)
            .await
            .expect("Failed to search_bundle_version");

        assert_eq!(version, "7.1.3-b011");
    }

    #[tokio::test]
    async fn test_search_app_config() {
        let secrets = search_app_config(TEST_BUNDLE_SOURCE)
            .await
            .expect("Failed to search_app_config");

        assert_eq!(
            secrets,
            AppConfig {
                app_id: "123456789".to_string(),
                secrets: HashMap::from([
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
