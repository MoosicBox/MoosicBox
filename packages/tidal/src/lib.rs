#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use moosicbox_core::sqlite::models::{AsModel, ToApi};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use url::form_urlencoded;

trait ToUrl {
    fn to_url(&self) -> String;
}

enum TidalApiEndpoint {
    DeviceAuthorization,
    DeviceAuthorizationToken,
    TrackUrl,
    FavoriteAlbums,
    AlbumTracks,
    Artist,
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
            Self::TrackUrl => format!("{TIDAL_API_BASE_URL}/tracks/:trackId/urlpostpaywall"),
            Self::FavoriteAlbums => format!("{TIDAL_API_BASE_URL}/users/:userId/favorites/albums"),
            Self::AlbumTracks => format!("{TIDAL_API_BASE_URL}/albums/:albumId/tracks"),
            Self::Artist => format!("{TIDAL_API_BASE_URL}/artists/:artistId"),
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

    let verification_uri_complete = value
        .get("verificationUriComplete")
        .unwrap()
        .as_str()
        .unwrap();

    let device_code = value.get("deviceCode").unwrap().as_str().unwrap();
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
}

pub async fn device_authorization_token(
    #[cfg(feature = "db")] db: &rusqlite::Connection,
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

    let access_token = value.get("access_token").unwrap().as_str().unwrap();
    let refresh_token = value.get("refresh_token").unwrap().as_str().unwrap();

    #[cfg(feature = "db")]
    if persist.unwrap_or(false) {
        let client_name = value.get("clientName").unwrap().as_str().unwrap();
        let expires_in = value.get("expires_in").unwrap().as_u64().unwrap() as u32;
        let scope = value.get("scope").unwrap().as_str().unwrap();
        let token_type = value.get("token_type").unwrap().as_str().unwrap();
        let user = serde_json::to_string(value.get("user").unwrap()).unwrap();
        let user_id = value.get("user_id").unwrap().as_u64().unwrap() as u32;

        db::create_tidal_config(
            db,
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
}

pub async fn track_url(
    #[cfg(feature = "db")] db: &rusqlite::Connection,
    audio_quality: TidalAudioQuality,
    track_id: u32,
    access_token: Option<String>,
) -> Result<Value, TidalTrackUrlError> {
    #[cfg(feature = "db")]
    let access_token = access_token.unwrap_or(
        db::get_tidal_access_token(db)?.ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?,
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

    let urls = value
        .get("urls")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "urls": urls,
    }))
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalDeviceType {
    Browser,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    pub id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl AsModel<TidalAlbum> for Value {
    fn as_model(&self) -> TidalAlbum {
        TidalAlbum {
            id: self.get("id").unwrap().as_u64().unwrap() as u32,
            artist_id: self
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap() as u32,
            audio_quality: self
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: self.get("copyright").unwrap().as_str().unwrap().to_string(),
            cover: self.get("cover").unwrap().as_str().unwrap().to_string(),
            duration: self.get("duration").unwrap().as_u64().unwrap() as u32,
            explicit: self.get("explicit").unwrap().as_bool().unwrap(),
            number_of_tracks: self.get("numberOfTracks").unwrap().as_u64().unwrap() as u32,
            popularity: self.get("popularity").unwrap().as_u64().unwrap() as u32,
            release_date: self
                .get("releaseDate")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            title: self.get("title").unwrap().as_str().unwrap().to_string(),
            media_metadata_tags: self
                .get("mediaMetadata")
                .unwrap()
                .get("tags")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
        }
    }
}

impl ToApi<ApiTidalAlbum> for TidalAlbum {
    fn to_api(&self) -> ApiTidalAlbum {
        ApiTidalAlbum {
            id: self.id,
            artist_id: self.artist_id,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            cover: self.cover.clone(),
            duration: self.duration,
            explicit: self.explicit,
            number_of_tracks: self.number_of_tracks,
            popularity: self.popularity,
            release_date: self.release_date.clone(),
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalAlbum {
    pub id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub cover: String,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: String,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    pub id: u32,
    pub track_number: u32,
    pub album_id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl AsModel<TidalTrack> for Value {
    fn as_model(&self) -> TidalTrack {
        TidalTrack {
            id: self.get("id").unwrap().as_u64().unwrap() as u32,
            track_number: self.get("trackNumber").unwrap().as_u64().unwrap() as u32,
            album_id: self
                .get("album")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap() as u32,
            artist_id: self
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap() as u32,
            audio_quality: self
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: self.get("copyright").unwrap().as_str().unwrap().to_string(),
            duration: self.get("duration").unwrap().as_u64().unwrap() as u32,
            explicit: self.get("explicit").unwrap().as_bool().unwrap(),
            isrc: self.get("isrc").unwrap().as_str().unwrap().to_string(),
            popularity: self.get("popularity").unwrap().as_u64().unwrap() as u32,
            title: self.get("title").unwrap().as_str().unwrap().to_string(),
            media_metadata_tags: self
                .get("mediaMetadata")
                .unwrap()
                .get("tags")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
        }
    }
}

impl ToApi<ApiTidalTrack> for TidalTrack {
    fn to_api(&self) -> ApiTidalTrack {
        ApiTidalTrack {
            id: self.id,
            track_number: self.track_number,
            album_id: self.album_id,
            artist_id: self.artist_id,
            audio_quality: self.audio_quality.clone(),
            copyright: self.copyright.clone(),
            duration: self.duration,
            explicit: self.explicit,
            isrc: self.isrc.clone(),
            popularity: self.popularity,
            title: self.title.clone(),
            media_metadata_tags: self.media_metadata_tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalTrack {
    pub id: u32,
    pub track_number: u32,
    pub album_id: u32,
    pub artist_id: u32,
    pub audio_quality: String,
    pub copyright: String,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtist {
    pub id: u32,
    pub picture: String,
    pub popularity: u32,
    pub name: String,
}

impl AsModel<TidalArtist> for Value {
    fn as_model(&self) -> TidalArtist {
        TidalArtist {
            id: self.get("id").unwrap().as_u64().unwrap() as u32,
            picture: self.get("picture").unwrap().as_str().unwrap().to_string(),
            popularity: self.get("popularity").unwrap().as_u64().unwrap() as u32,
            name: self.get("name").unwrap().as_str().unwrap().to_string(),
        }
    }
}

impl ToApi<ApiTidalArtist> for TidalArtist {
    fn to_api(&self) -> ApiTidalArtist {
        ApiTidalArtist {
            id: self.id,
            picture: self.picture.clone(),
            popularity: self.popularity,
            name: self.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTidalArtist {
    pub id: u32,
    pub picture: String,
    pub popularity: u32,
    pub name: String,
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
}

#[allow(clippy::too_many_arguments)]
pub async fn favorite_albums(
    #[cfg(feature = "db")] db: &rusqlite::Connection,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
    user_id: Option<u32>,
) -> Result<(Vec<ApiTidalAlbum>, u32), TidalFavoriteAlbumsError> {
    #[cfg(feature = "db")]
    let (access_token, user_id) = {
        match (access_token.clone(), user_id) {
            (Some(access_token), Some(user_id)) => (access_token, user_id),
            _ => {
                let config = db::get_tidal_config(db)?
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

    let items = value
        .get("items")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.get("item").unwrap())
        .map(|item| item.as_model())
        .map(|album: TidalAlbum| album.to_api())
        .collect::<Vec<_>>();

    let count = value.get("totalNumberOfItems").unwrap().as_u64().unwrap() as u32;

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
}

#[allow(clippy::too_many_arguments)]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &rusqlite::Connection,
    album_id: u32,
    offset: Option<u32>,
    limit: Option<u32>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<(Vec<ApiTidalTrack>, u32), TidalAlbumTracksError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config =
                db::get_tidal_config(db)?.ok_or(TidalAlbumTracksError::NoAccessTokenAvailable)?;

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
        .get("items")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_model())
        .map(|album: TidalTrack| album.to_api())
        .collect::<Vec<_>>();

    let count = value.get("totalNumberOfItems").unwrap().as_u64().unwrap() as u32;

    Ok((items, count))
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
}

#[allow(clippy::too_many_arguments)]
pub async fn artist(
    #[cfg(feature = "db")] db: &rusqlite::Connection,
    artist_id: u32,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
    access_token: Option<String>,
) -> Result<TidalArtist, TidalArtistError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config =
                db::get_tidal_config(db)?.ok_or(TidalArtistError::NoAccessTokenAvailable)?;

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

    Ok(value.as_model())
}
