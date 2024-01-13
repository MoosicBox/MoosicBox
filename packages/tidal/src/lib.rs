#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use moosicbox_core::sqlite::models::AsModelResult;
use moosicbox_json_utils::{ParseError, ToNestedValue, ToValue, ToValueType};
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
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl TidalAlbum {
    pub fn cover_url(&self, size: u16) -> String {
        let cover_path = self.cover.replace('-', "/");
        format!("https://resources.tidal.com/images/{cover_path}/{size}x{size}.jpg")
    }
}

impl ToValueType<TidalAlbum> for &Value {
    fn to_value_type(self) -> Result<TidalAlbum, ParseError> {
        self.as_model()
    }

    fn missing_value(self, error: ParseError) -> Result<TidalAlbum, ParseError> {
        Err(error)
    }
}

impl AsModelResult<TidalAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<TidalAlbum, ParseError> {
        Ok(TidalAlbum {
            id: self.to_value("id")?,
            artist: self.to_value("artist")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("release_date")?,
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
    pub album_id: u64,
    pub album: String,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl ToValueType<TidalTrack> for &Value {
    fn to_value_type(self) -> Result<TidalTrack, ParseError> {
        self.as_model()
    }

    fn missing_value(self, error: ParseError) -> Result<TidalTrack, ParseError> {
        Err(error)
    }
}

impl AsModelResult<TidalTrack, ParseError> for Value {
    fn as_model(&self) -> Result<TidalTrack, ParseError> {
        Ok(TidalTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_value("artist")?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_value("album")?,
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

impl ToValueType<TidalArtist> for &Value {
    fn to_value_type(self) -> Result<TidalArtist, ParseError> {
        self.as_model()
    }

    fn missing_value(self, error: ParseError) -> Result<TidalArtist, ParseError> {
        Err(error)
    }
}

impl AsModelResult<TidalArtist, ParseError> for Value {
    fn as_model(&self) -> Result<TidalArtist, ParseError> {
        Ok(TidalArtist {
            id: self.to_value("id")?,
            picture: self.to_value("picture")?,
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
    DeviceAuthorizationToken,
    Artist,
    FavoriteArtists,
    Album,
    FavoriteAlbums,
    ArtistAlbums,
    Track,
    FavoriteTracks,
    AlbumTracks,
    TrackUrl,
}

static TIDAL_AUTH_API_BASE_URL: &str = "https://auth.tidal.com/v1";
static TIDAL_API_BASE_URL: &str = "https://api.tidal.com/v1";

impl ToUrl for TidalApiEndpoint {
    fn to_url(&self) -> String {
        match self {
            Self::DeviceAuthorization => {
                format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/device_authorization")
            }
            Self::DeviceAuthorizationToken => format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/token"),
            Self::Artist => format!("{TIDAL_API_BASE_URL}/artists/:artistId"),
            Self::FavoriteArtists => {
                format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/artists")
            }
            Self::Album => format!("{TIDAL_API_BASE_URL}/albums/:albumId"),
            Self::FavoriteAlbums => format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/albums"),
            Self::ArtistAlbums => format!("{TIDAL_API_BASE_URL}/artists/:artistId/albums"),
            Self::Track => format!("{TIDAL_API_BASE_URL}/tracks/:trackId"),
            Self::FavoriteTracks => format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/tracks"),
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

    let value: Value = reqwest::Client::new()
        .post(url)
        .form(&params)
        .send()
        .await?
        .json()
        .await?;

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
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
    client_id: String,
    client_secret: String,
    device_code: String,
    #[cfg(feature = "db")] persist: Option<bool>,
) -> Result<Value, TidalDeviceAuthorizationTokenError> {
    let url = tidal_api_endpoint!(DeviceAuthorizationToken);

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

    let value: Value = reqwest::Client::new()
        .post(url)
        .form(&params)
        .send()
        .await?
        .json()
        .await?;

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
            &db.inner,
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

#[derive(Debug, Error)]
pub enum TidalTrackUrlError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn track_url(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    audio_quality: TidalAudioQuality,
    track_id: u64,
    access_token: Option<String>,
) -> Result<Vec<String>, TidalTrackUrlError> {
    #[cfg(feature = "db")]
    let access_token = access_token.unwrap_or(
        db::get_tidal_access_token(&db.library.lock().as_ref().unwrap().inner)?
            .ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?,
    );

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?;

    let url = tidal_api_endpoint!(
        TrackUrl,
        &[(":trackId", &track_id.to_string())],
        &[
            ("audioquality", audio_quality.as_ref()),
            ("urlusagemode", "STREAM"),
            ("assetpresentation", "FULL")
        ]
    );

    let urls = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json::<Value>()
        .await?
        .to_value("urls")?;

    Ok(urls)
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
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn favorite_artists(
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
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
    let (access_token, user_id) = {
        match (access_token.clone(), user_id) {
            (Some(access_token), Some(user_id)) => (access_token, user_id),
            _ => {
                let config = db::get_tidal_config(&db.inner)?
                    .ok_or(TidalFavoriteArtistsError::NoAccessTokenAvailable)?;
                (
                    access_token.unwrap_or(config.access_token),
                    user_id.unwrap_or(config.user_id),
                )
            }
        }
    };

    #[cfg(not(feature = "db"))]
    let (access_token, user_id) = (
        access_token.ok_or(TidalFavoriteArtistsError::NoAccessTokenAvailable)?,
        user_id.ok_or(TidalFavoriteArtistsError::NoUserIdAvailable)?,
    );

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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    let items = value.to_nested_value(&["items", "item"])?;
    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("No user ID available")]
    NoUserIdAvailable,
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
    let (access_token, user_id) = {
        match (access_token.clone(), user_id) {
            (Some(access_token), Some(user_id)) => (access_token, user_id),
            _ => {
                let config = db::get_tidal_config(&db.library.lock().unwrap().inner)?
                    .ok_or(TidalFavoriteAlbumsError::NoAccessTokenAvailable)?;
                (
                    access_token.unwrap_or(config.access_token),
                    user_id.unwrap_or(config.user_id),
                )
            }
        }
    };

    #[cfg(not(feature = "db"))]
    let (access_token, user_id) = (
        access_token.ok_or(TidalFavoriteAlbumsError::NoAccessTokenAvailable)?,
        user_id.ok_or(TidalFavoriteAlbumsError::NoUserIdAvailable)?,
    );

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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    let items = value.to_nested_value(&["items", "item"])?;
    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
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
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn favorite_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
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
    let (access_token, user_id) = {
        match (access_token.clone(), user_id) {
            (Some(access_token), Some(user_id)) => (access_token, user_id),
            _ => {
                let config = db::get_tidal_config(&db.inner)?
                    .ok_or(TidalFavoriteTracksError::NoAccessTokenAvailable)?;
                (
                    access_token.unwrap_or(config.access_token),
                    user_id.unwrap_or(config.user_id),
                )
            }
        }
    };

    #[cfg(not(feature = "db"))]
    let (access_token, user_id) = (
        access_token.ok_or(TidalFavoriteTracksError::NoAccessTokenAvailable)?,
        user_id.ok_or(TidalFavoriteTracksError::NoUserIdAvailable)?,
    );

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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    let items = value.to_nested_value(&["items", "item"])?;
    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum TidalArtistAlbumsError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn artist_albums(
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
    artist_id: u64,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<(Vec<TidalAlbum>, u32), TidalArtistAlbumsError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config = db::get_tidal_config(&db.inner)?
                .ok_or(TidalArtistAlbumsError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalArtistAlbumsError::NoAccessTokenAvailable)?;

    let url = tidal_api_endpoint!(
        ArtistAlbums,
        &[(":artistId", &artist_id.to_string())],
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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    let items = value.to_nested_value(&["items", "item"])?;
    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum TidalAlbumTracksError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
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
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config = db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner)?
                .ok_or(TidalAlbumTracksError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalAlbumTracksError::NoAccessTokenAvailable)?;

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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    let items = value
        .to_nested_value::<Option<_>>(&["items", "item"])?
        .ok_or_else(|| TidalAlbumTracksError::RequestFailed(format!("{value:?}")))?;

    let count = value.to_value("totalNumberOfItems")?;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum TidalAlbumError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn album(
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
    album_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalAlbum, TidalAlbumError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config =
                db::get_tidal_config(&db.inner)?.ok_or(TidalAlbumError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalAlbumError::NoAccessTokenAvailable)?;

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

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json()
        .await?;

    Ok(value.as_model()?)
}

#[derive(Debug, Error)]
pub enum TidalArtistError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
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
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config = db::get_tidal_config(&db.library.lock().as_ref().unwrap().inner)?
                .ok_or(TidalArtistError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalArtistError::NoAccessTokenAvailable)?;

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

    let value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json::<Value>()
        .await?
        .as_model()?;

    Ok(value)
}

#[derive(Debug, Error)]
pub enum TidalTrackError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[allow(clippy::too_many_arguments)]
pub async fn track(
    #[cfg(feature = "db")] db: &moosicbox_core::app::DbConnection,
    track_id: u64,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalTrack, TidalTrackError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config =
                db::get_tidal_config(&db.inner)?.ok_or(TidalTrackError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalTrackError::NoAccessTokenAvailable)?;

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

    let value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await?
        .json::<Value>()
        .await?
        .as_model()?;

    Ok(value)
}
