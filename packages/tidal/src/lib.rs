#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use async_recursion::async_recursion;
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalDeviceType {
    Browser,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    pub id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl TidalAlbum {
    pub fn cover_url(&self, size: u16) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.tidal.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl MissingValue<TidalAlbum> for &Value {}
impl ToValueType<TidalAlbum> for &Value {
    fn to_value_type(self) -> Result<TidalAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<TidalAlbum, ParseError> {
        Ok(TidalAlbum {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            contains_cover: true,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("releaseDate")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    pub id: u64,
    pub track_number: u32,
    pub artist_id: u64,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub album_id: u64,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl MissingValue<TidalTrack> for &Value {}
impl ToValueType<TidalTrack> for &Value {
    fn to_value_type(self) -> Result<TidalTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalTrack, ParseError> for Value {
    fn as_model(&self) -> Result<TidalTrack, ParseError> {
        Ok(TidalTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            isrc: self.to_value("isrc")?,
            popularity: self.to_value("popularity")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub popularity: u32,
    pub name: String,
}

impl TidalArtist {
    pub fn picture_url(&self, size: u16) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.tidal.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
}

impl MissingValue<TidalArtist> for &Value {}
impl ToValueType<TidalArtist> for &Value {
    fn to_value_type(self) -> Result<TidalArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalArtist, ParseError> for Value {
    fn as_model(&self) -> Result<TidalArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(TidalArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            popularity: self.to_value("popularity")?,
            name: self.to_value("name")?,
        })
    }
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
}

static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn device_authorization_token(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
            &db.library.lock().as_ref().unwrap().inner,
            &client_id,
            access_token,
            refresh_token,
            client_name,
            expires_in,
            scope,
            token_type,
            &user,
            user_id,
        )?;
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

fn fetch_credentials(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    access_token: Option<String>,
) -> Result<TidalCredentials, FetchCredentialsError> {
    #[cfg(feature = "db")]
    {
        access_token
            .map(|token| {
                log::debug!("Using passed access_token");
                Ok(TidalCredentials {
                    access_token: token.to_string(),
                    client_id: None,
                    refresh_token: None,
                    persist: false,
                })
            })
            .or_else(|| {
                log::debug!("Fetching db Tidal config");

                match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
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
            })
            .transpose()?
            .ok_or(FetchCredentialsError::NoAccessTokenAvailable)
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
    )?;

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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

async fn refetch_access_token(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
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
            &db.library.lock().as_ref().unwrap().inner,
            client_id,
            access_token,
            refresh_token,
            client_name,
            expires_in,
            scope,
            token_type,
            &user,
            user_id,
        )?;
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
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalArtistOrder>,
    order_direction: Option<TidalArtistOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(Vec<TidalArtist>, u32), TidalFavoriteArtistsError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

    let user_id = user_id.ok_or(TidalFavoriteArtistsError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteArtists,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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
        db,
        &url,
        access_token,
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

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteArtistError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(Vec<TidalAlbum>, u32), TidalFavoriteAlbumsError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

    let user_id = user_id.ok_or(TidalFavoriteAlbumsError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteAlbums,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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
        db,
        &url,
        access_token,
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

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteAlbumError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalTrackOrder>,
    order_direction: Option<TidalTrackOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(Vec<TidalTrack>, u32), TidalFavoriteTracksError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

    let user_id = user_id.ok_or(TidalFavoriteTracksError::NoUserIdAvailable)?;

    let url = tidal_api_endpoint!(
        FavoriteTracks,
        &[(":userId", &user_id.to_string())],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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
        db,
        &url,
        access_token,
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

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalAddFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u64>,
) -> Result<(), TidalRemoveFavoriteTrackError> {
    #[cfg(feature = "db")]
    let user_id = user_id.or_else(|| {
        match db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner) {
            Ok(Some(config)) => Some(config.user_id),
            _ => None,
        }
    });

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
    Lp,
    EpsAndSingles,
    Compilations,
}

#[allow(clippy::too_many_arguments)]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<TidalAlbumType>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<(Vec<TidalAlbum>, u32), TidalArtistAlbumsError> {
    let mut query: Vec<(&str, String)> = vec![
        ("offset", offset.unwrap_or(0).to_string()),
        ("limit", limit.unwrap_or(100).to_string()),
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
        db,
        &url,
        access_token,
    )
    .await?;

    log::trace!("Received artist albums response: {value:?}");

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| TidalArtistAlbumsError::RequestFailed(format!("{value:?}")))?;

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<(Vec<TidalTrack>, u32), TidalAlbumTracksError> {
    let url = tidal_api_endpoint!(
        AlbumTracks,
        &[(":albumId", &album_id.to_string())],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
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

    let items = value
        .to_value::<Option<_>>("items")?
        .ok_or_else(|| TidalAlbumTracksError::RequestFailed(format!("{value:?}")))?;

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: u64,
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    artist_id: u64,
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

#[allow(clippy::too_many_arguments)]
pub async fn track(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    track_id: u64,
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    audio_quality: TidalAudioQuality,
    track_id: u64,
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
