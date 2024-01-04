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

#[derive(Debug, Error)]
pub enum TidalDeviceAuthorizationError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

trait ToUrl {
    fn to_url(&self) -> &str;
}

enum TidalApiEndpoint {
    DeviceAuthorization,
    DeviceAuthorizationToken,
    TrackUrl,
    FavoriteAlbums,
}

impl ToUrl for TidalApiEndpoint {
    fn to_url(&self) -> &str {
        match self {
            TidalApiEndpoint::DeviceAuthorization => {
                "https://auth.tidal.com/v1/oauth2/device_authorization"
            }
            TidalApiEndpoint::DeviceAuthorizationToken => "https://auth.tidal.com/v1/oauth2/token",
            TidalApiEndpoint::TrackUrl => "https://api.tidal.com/v1/tracks/:trackId/urlpostpaywall",
            TidalApiEndpoint::FavoriteAlbums => {
                "https://api.tidal.com/v1/users/:userId/favorites/albums"
            }
        }
    }
}

pub async fn device_authorization(
    client_id: String,
) -> Result<Value, TidalDeviceAuthorizationError> {
    let url = TidalApiEndpoint::DeviceAuthorization.to_url();

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

    Ok(serde_json::json!({
        "url": format!("https://{verification_uri_complete}"),
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
    let url = TidalApiEndpoint::DeviceAuthorizationToken.to_url();

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
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("audioquality", audio_quality.as_ref())
        .append_pair("urlusagemode", "STREAM")
        .append_pair("assetpresentation", "FULL")
        .finish();

    #[cfg(feature = "db")]
    let access_token = access_token.unwrap_or(
        db::get_tidal_access_token(db)?.ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?,
    );

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?;

    let url = TidalApiEndpoint::TrackUrl
        .to_url()
        .replace(":trackId", &track_id.to_string())
        + "?"
        + &query_string;

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
) -> Result<Vec<ApiTidalAlbum>, TidalFavoriteAlbumsError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("offset", &offset.unwrap_or(0).to_string())
        .append_pair("limit", &limit.unwrap_or(100).to_string())
        .append_pair("order", order.unwrap_or(TidalAlbumOrder::Date).as_ref())
        .append_pair(
            "orderDirection",
            order_direction
                .unwrap_or(TidalAlbumOrderDirection::Desc)
                .as_ref(),
        )
        .append_pair("countryCode", &country_code.clone().unwrap_or("US".into()))
        .append_pair("locale", &locale.clone().unwrap_or("en_US".into()))
        .append_pair(
            "deviceType",
            device_type.unwrap_or(TidalDeviceType::Browser).as_ref(),
        )
        .finish();

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

    let url = TidalApiEndpoint::FavoriteAlbums
        .to_url()
        .replace(":userId", &user_id.to_string())
        + "?"
        + &query_string;

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

    Ok(items)
}
