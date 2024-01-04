use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::{app::AppState, sqlite::models::ToApi};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use url::form_urlencoded;

use crate::db::{
    models::{ApiTidalAlbum, TidalAlbum},
    {create_tidal_config, get_tidal_access_token, get_tidal_config},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationQuery {
    client_id: String,
}

#[route("/tidal/auth/device-authorization", method = "POST")]
pub async fn tidal_device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    let url = "https://auth.tidal.com/v1/oauth2/device_authorization";

    let params = [
        ("client_id", query.client_id.clone()),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = reqwest::Client::new()
        .post(url)
        .form(&params)
        .send()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization link"))?
        .json()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization link"))?;

    let verification_uri_complete = value
        .get("verificationUriComplete")
        .unwrap()
        .as_str()
        .unwrap();

    let device_code = value.get("deviceCode").unwrap().as_str().unwrap();

    Ok(Json(serde_json::json!({
        "url": format!("https://{verification_uri_complete}"),
        "device_code": device_code,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationTokenQuery {
    client_id: String,
    client_secret: String,
    device_code: String,
    persist: Option<bool>,
}

#[route("/tidal/auth/device-authorization/token", method = "POST")]
pub async fn tidal_device_authorization_token_endpoint(
    query: web::Query<TidalDeviceAuthorizationTokenQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let url = "https://auth.tidal.com/v1/oauth2/token";

    let params = [
        ("client_id", query.client_id.clone()),
        ("client_secret", query.client_secret.clone()),
        ("device_code", query.device_code.clone()),
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
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization token"))?
        .json()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization token"))?;

    let access_token = value.get("access_token").unwrap().as_str().unwrap();
    let refresh_token = value.get("refresh_token").unwrap().as_str().unwrap();

    if query.persist.unwrap_or(false) {
        let client_name = value.get("clientName").unwrap().as_str().unwrap();
        let expires_in = value.get("expires_in").unwrap().as_u64().unwrap() as u32;
        let scope = value.get("scope").unwrap().as_str().unwrap();
        let token_type = value.get("token_type").unwrap().as_str().unwrap();
        let user = serde_json::to_string(value.get("user").unwrap()).unwrap();
        let user_id = value.get("user_id").unwrap().as_u64().unwrap() as u32;

        create_tidal_config(
            &data
                .db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            access_token,
            refresh_token,
            client_name,
            expires_in,
            scope,
            token_type,
            &user,
            user_id,
        )
        .map_err(|e| ErrorInternalServerError(format!("Failed to persist tidal config: {e:?}")))?;
    }

    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
    })))
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TidalAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrackUrlQuery {
    audio_quality: TidalAudioQuality,
    track_id: u32,
}

#[route("/tidal/track/url", method = "GET")]
pub async fn tidal_track_url_endpoint(
    query: web::Query<TidalTrackUrlQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("audioquality", query.audio_quality.as_ref())
        .append_pair("urlusagemode", "STREAM")
        .append_pair("assetpresentation", "FULL")
        .finish();

    let access_token = get_tidal_access_token(
        &data
            .db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap()
            .inner,
    )
    .map_err(|e| {
        ErrorInternalServerError(format!("Failed to get tidal config access token: {e:?}"))
    })?
    .ok_or(ErrorInternalServerError("No access token available"))?;

    let url = format!(
        "https://api.tidal.com/v1/tracks/{}/urlpostpaywall?{query_string}",
        query.track_id
    );

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token),
        )
        .send()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get track url: {e:?}")))?
        .json()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get track url: {e:?}")))?;

    let urls = value
        .get("urls")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "urls": urls,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[route("/tidal/favorites/albums", method = "GET")]
pub async fn tidal_favorite_albums_endpoint(
    query: web::Query<TidalFavoriteAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiTidalAlbum>>> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("offset", &query.offset.unwrap_or(0).to_string())
        .append_pair("limit", &query.limit.unwrap_or(100).to_string())
        .append_pair("order", "DATE")
        .append_pair("orderDirection", "DESC")
        .append_pair("countryCode", "US")
        .append_pair("locale", "en_US")
        .append_pair("deviceType", "BROWSER")
        .finish();

    let config = get_tidal_config(
        &data
            .db
            .clone()
            .expect("Db not set")
            .library
            .lock()
            .as_ref()
            .unwrap()
            .inner,
    )
    .map_err(|e| {
        ErrorInternalServerError(format!("Failed to get tidal config access token: {e:?}"))
    })?
    .ok_or(ErrorInternalServerError("No access token available"))?;

    let url = format!(
        "https://api.tidal.com/v1/users/{}/favorites/albums?{query_string}",
        config.user_id
    );
    println!("test.... {url}");

    let value: Value = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", config.access_token),
        )
        .send()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get track url: {e:?}")))?
        .json()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get track url: {e:?}")))?;

    let items = value
        .get("items")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.get("item").unwrap())
        .map(|item| item.as_object().unwrap())
        .map(|item| TidalAlbum {
            id: item.get("id").unwrap().as_u64().unwrap() as u32,
            artist_id: item
                .get("artist")
                .unwrap()
                .get("id")
                .unwrap()
                .as_u64()
                .unwrap() as u32,
            audio_quality: item
                .get("audioQuality")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            copyright: item.get("copyright").unwrap().as_str().unwrap().to_string(),
            cover: item.get("cover").unwrap().as_str().unwrap().to_string(),
            duration: item.get("duration").unwrap().as_u64().unwrap() as u32,
            explicit: item.get("explicit").unwrap().as_bool().unwrap(),
            number_of_tracks: item.get("numberOfTracks").unwrap().as_u64().unwrap() as u32,
            popularity: item.get("popularity").unwrap().as_u64().unwrap() as u32,
            release_date: item
                .get("releaseDate")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            title: item.get("title").unwrap().as_str().unwrap().to_string(),
            media_metadata_tags: item
                .get("mediaMetadata")
                .unwrap()
                .get("tags")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
        })
        .map(|album| album.to_api())
        .collect::<Vec<_>>();

    Ok(Json(items))
}
