#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

use std::{collections::HashMap, str::Utf8Error};

use base64::{engine::general_purpose, Engine as _};
use moosicbox_core::sqlite::models::AsModel;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use url::form_urlencoded;

static AUTH_HEADER_NAME: &str = "x-user-auth-token";

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzDeviceType {
    Browser,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbum {
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

impl QobuzAlbum {
    pub fn cover_url(&self, size: u16) -> String {
        let cover_path = self.cover.replace('-', "/");
        format!("https://resources.qobuz.com/images/{cover_path}/{size}x{size}.jpg")
    }
}

impl AsModel<QobuzAlbum> for Value {
    fn as_model(&self) -> QobuzAlbum {
        QobuzAlbum {
            id: self.get("id").unwrap().as_u64().unwrap(),
            artist: self
                .get("artist")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            artist_id: self
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap(),
            audio_quality: self
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: self
                .get("copyright")
                .and_then(|c| c.as_str().map(|c| c.to_string())),
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub album_id: u64,
    pub artist_id: u64,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl AsModel<QobuzTrack> for Value {
    fn as_model(&self) -> QobuzTrack {
        QobuzTrack {
            id: self.get("id").unwrap().as_u64().unwrap(),
            track_number: self.get("trackNumber").unwrap().as_u64().unwrap() as u32,
            album_id: self
                .get("album")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap(),
            artist_id: self
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap(),
            audio_quality: self
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: self
                .get("copyright")
                .and_then(|c| c.as_str().map(|c| c.to_string())),
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub popularity: u32,
    pub name: String,
}

impl QobuzArtist {
    pub fn picture_url(&self, size: u16) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.qobuz.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
}

impl AsModel<QobuzArtist> for Value {
    fn as_model(&self) -> QobuzArtist {
        QobuzArtist {
            id: self.get("id").unwrap().as_u64().unwrap(),
            picture: self
                .get("picture")
                .unwrap()
                .as_str()
                .map(|pic| pic.to_string()),
            popularity: self.get("popularity").unwrap().as_u64().unwrap() as u32,
            name: self.get("name").unwrap().as_str().unwrap().to_string(),
        }
    }
}

trait ToUrl {
    fn to_url(&self) -> String;
}

enum QobuzApiEndpoint {
    Login,
    Bundle,
    FavoriteAlbums,
    AlbumTracks,
}

static QOBUZ_PLAY_API_BASE_URL: &str = "https://play.qobuz.com";
static QOBUZ_API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

impl ToUrl for QobuzApiEndpoint {
    fn to_url(&self) -> String {
        match self {
            Self::Login => {
                format!("{QOBUZ_PLAY_API_BASE_URL}/login")
            }
            Self::Bundle => format!("{QOBUZ_PLAY_API_BASE_URL}/resources/:bundleVersion/bundle.js"),
            Self::FavoriteAlbums => format!("{QOBUZ_API_BASE_URL}/favorite/getUserFavorites"),
            Self::AlbumTracks => format!("{QOBUZ_API_BASE_URL}/album/get"),
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzAlbumOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum QobuzAlbumOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum QobuzFavoriteAlbumsError {
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
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
) -> Result<(Vec<QobuzAlbum>, u32), QobuzFavoriteAlbumsError> {
    #[cfg(feature = "db")]
    let access_token = {
        match access_token.clone() {
            Some(access_token) => access_token,
            _ => {
                let config = db::get_qobuz_config(&db.library.lock().unwrap().inner)?
                    .ok_or(QobuzFavoriteAlbumsError::NoAccessTokenAvailable)?;
                access_token.unwrap_or(config.access_token)
            }
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(QobuzFavoriteAlbumsError::NoAccessTokenAvailable)?;

    let url = qobuz_api_endpoint!(
        FavoriteAlbums,
        &[],
        &[
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
            ("type", "albums"),
        ]
    );

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(AUTH_HEADER_NAME, format!("Bearer {}", access_token))
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
        .collect::<Vec<_>>();

    let count = value.get("totalNumberOfItems").unwrap().as_u64().unwrap() as u32;

    Ok((items, count))
}

#[derive(Debug, Error)]
pub enum QobuzAlbumTracksError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
}

#[allow(clippy::too_many_arguments)]
pub async fn album_tracks(
    #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
    album_id: &str,
    offset: Option<u32>,
    limit: Option<u32>,
    access_token: Option<String>,
) -> Result<(Vec<QobuzTrack>, u32), QobuzAlbumTracksError> {
    #[cfg(feature = "db")]
    let access_token = match access_token {
        Some(access_token) => access_token,
        _ => {
            let config = db::get_qobuz_config(&db.library.lock().as_ref().unwrap().inner)?
                .ok_or(QobuzAlbumTracksError::NoAccessTokenAvailable)?;

            access_token.unwrap_or(config.access_token)
        }
    };

    #[cfg(not(feature = "db"))]
    let access_token = access_token.ok_or(QobuzAlbumTracksError::NoAccessTokenAvailable)?;

    let url = qobuz_api_endpoint!(
        AlbumTracks,
        &[
            ("album_id", album_id),
            ("offset", &offset.unwrap_or(0).to_string()),
            ("limit", &limit.unwrap_or(100).to_string()),
        ]
    );

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(AUTH_HEADER_NAME, format!("Bearer {}", access_token))
        .send()
        .await?
        .json()
        .await?;

    let items = match value.get("tracks").unwrap().get("items") {
        Some(items) => items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_model())
            .collect::<Vec<_>>(),
        None => {
            return Err(QobuzAlbumTracksError::RequestFailed(format!("{value:?}")));
        }
    };

    let count = value.get("totalNumberOfItems").unwrap().as_u64().unwrap() as u32;

    Ok((items, count))
}

static BUNDLE_ID_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"<script src="/resources/(\d+\.\d+\.\d+-[a-z]\d{3})/bundle\.js"></script>"#)
        .unwrap()
});

static APP_ID_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"production:\{api:\{appId:"([^"]+)""#).unwrap());

static SEED_AND_TIMEZONE_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"[a-z]\.initialSeed\("([\w=]+)",window\.utimezone\.(.+?)\)"#).unwrap()
});

static INFO_AND_EXTRAS_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r#"name:"\w+/([^"]+)",info:"([\w=]+)",extras:"([\w=]+)""#).unwrap()
});

#[derive(Debug, Error)]
pub enum QobuzFetchBundleError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error("No bundle file found in output")]
    NoBundleFile,
}

async fn fetch_bundle_version() -> Result<String, QobuzFetchBundleError> {
    let url = qobuz_api_endpoint!(Login);

    let value = reqwest::Client::new().get(url).send().await?.text().await?;

    if let Some(caps) = BUNDLE_ID_REGEX.captures(&value) {
        if let Some(version) = caps.get(1) {
            let version = version.as_str();
            log::debug!("Found version={version}");
            Ok(version.to_string())
        } else {
            Err(QobuzFetchBundleError::NoBundleFile)
        }
    } else {
        Err(QobuzFetchBundleError::NoBundleFile)
    }
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
    QobuzFetchBundleError(#[from] QobuzFetchBundleError),
    #[error(transparent)]
    Utf8(#[from] Utf8Error),
}

fn capitalize(value: &str) -> String {
    let mut v: Vec<char> = value.chars().collect();
    v[0] = v[0].to_uppercase().next().unwrap();
    v.into_iter().collect()
}

#[allow(unused)]
pub(crate) async fn fetch_app_secrets(
) -> Result<(String, HashMap<String, String>), QobuzFetchAppSecretsError> {
    let version = fetch_bundle_version().await?;

    let url = qobuz_api_endpoint!(Bundle, &[(":bundleVersion", &version)]);

    let value = reqwest::Client::new().get(url).send().await?.text().await?;

    let app_id = if let Some(caps) = APP_ID_REGEX.captures(&value) {
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

    for caps in SEED_AND_TIMEZONE_REGEX.captures_iter(&value) {
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

    for caps in INFO_AND_EXTRAS_REGEX.captures_iter(&value) {
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
        let (_, info, extras) = name_info_extras
            .iter()
            .find(|(name, _, _)| name.starts_with(&capitalize(&timezone)))
            .ok_or(QobuzFetchAppSecretsError::NoMatchingInfoForTimezone)
            .expect("No matching name for timezone");

        let secret_base64 = format!("{}{}{}", seed, info, extras);
        let secret_base64 = &secret_base64[..secret_base64.len() - 44];
        let secret = general_purpose::STANDARD.decode(secret_base64)?;
        let secret = std::str::from_utf8(&secret)?.to_string();

        secrets.insert(timezone, secret);
    }

    Ok((app_id, secrets))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{fetch_app_secrets, fetch_bundle_version};

    #[tokio::test]
    async fn test_fetch_bundle() {
        let version = fetch_bundle_version()
            .await
            .expect("Failed to fetch bundle");

        assert_eq!(version, "7.1.3-b011");
    }

    #[tokio::test]
    async fn test_fetch_app_secrets_bundle() {
        let secrets = fetch_app_secrets().await.expect("Failed to fetch bundle");

        assert_eq!(
            secrets,
            (
                "950096963".to_string(),
                HashMap::from([
                    (
                        "london".to_string(),
                        "10b251c286cfbf64d6b7105f253d9a2e".to_string()
                    ),
                    (
                        "algier".to_string(),
                        "2ab7131d383623cf403cf3d4676c56b6".to_string()
                    ),
                    (
                        "berlin".to_string(),
                        "979549437fcc4a3faad4867b5cd25dcb".to_string()
                    )
                ])
            )
        );
    }
}
