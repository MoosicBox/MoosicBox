#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

pub mod api;
pub mod db;

use moosicbox_core::sqlite::{
    db::DbError,
    models::{AsModel, ToApi},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use url::form_urlencoded;

use crate::db::{
    models::ApiTidalAlbum,
    {create_tidal_config, get_tidal_access_token, get_tidal_config},
};

#[derive(Debug, Error)]
pub enum TidalDeviceAuthorizationError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

pub async fn tidal_device_authorization(
    client_id: String,
) -> Result<Value, TidalDeviceAuthorizationError> {
    let url = "https://auth.tidal.com/v1/oauth2/device_authorization";

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
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn tidal_device_authorization_token(
    db: &Connection,
    client_id: String,
    client_secret: String,
    device_code: String,
    persist: Option<bool>,
) -> Result<Value, TidalDeviceAuthorizationTokenError> {
    let url = "https://auth.tidal.com/v1/oauth2/token";

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

    if persist.unwrap_or(false) {
        let client_name = value.get("clientName").unwrap().as_str().unwrap();
        let expires_in = value.get("expires_in").unwrap().as_u64().unwrap() as u32;
        let scope = value.get("scope").unwrap().as_str().unwrap();
        let token_type = value.get("token_type").unwrap().as_str().unwrap();
        let user = serde_json::to_string(value.get("user").unwrap()).unwrap();
        let user_id = value.get("user_id").unwrap().as_u64().unwrap() as u32;

        create_tidal_config(
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
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
}

pub async fn tidal_track_url(
    db: &Connection,
    audio_quality: TidalAudioQuality,
    track_id: u32,
) -> Result<Value, TidalTrackUrlError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("audioquality", audio_quality.as_ref())
        .append_pair("urlusagemode", "STREAM")
        .append_pair("assetpresentation", "FULL")
        .finish();

    let access_token =
        get_tidal_access_token(db)?.ok_or(TidalTrackUrlError::NoAccessTokenAvailable)?;

    let url = format!(
        "https://api.tidal.com/v1/tracks/{}/urlpostpaywall?{query_string}",
        track_id
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

#[derive(Debug, Error)]
pub enum TidalFavoriteAlbumsError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No access token available")]
    NoAccessTokenAvailable,
}

#[allow(clippy::too_many_arguments)]
pub async fn tidal_favorite_albums(
    db: &Connection,
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
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

    let config = get_tidal_config(db)?.ok_or(TidalFavoriteAlbumsError::NoAccessTokenAvailable)?;

    let url = format!(
        "https://api.tidal.com/v1/users/{}/favorites/albums?{query_string}",
        config.user_id
    );

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", config.access_token),
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
        .map(|album| album.to_api())
        .collect::<Vec<_>>();

    Ok(items)
}
