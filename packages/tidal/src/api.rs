use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    db::models::ApiTidalAlbum, tidal_device_authorization, tidal_device_authorization_token,
    tidal_favorite_albums, tidal_track_url, TidalAlbumOrder, TidalAlbumOrderDirection,
    TidalAudioQuality, TidalDeviceAuthorizationError, TidalDeviceAuthorizationTokenError,
    TidalDeviceType, TidalFavoriteAlbumsError, TidalTrackUrlError,
};

impl From<TidalDeviceAuthorizationError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationQuery {
    client_id: String,
}

#[route("/tidal/auth/device-authorization", method = "POST")]
pub async fn tidal_device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        tidal_device_authorization(query.client_id.clone()).await?,
    ))
}

impl From<TidalDeviceAuthorizationTokenError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationTokenError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
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
    Ok(Json(
        tidal_device_authorization_token(
            &data
                .db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            query.client_id.clone(),
            query.client_secret.clone(),
            query.device_code.clone(),
            query.persist,
        )
        .await?,
    ))
}

impl From<TidalTrackUrlError> for actix_web::Error {
    fn from(err: TidalTrackUrlError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
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
    Ok(Json(
        tidal_track_url(
            &data
                .db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            query.audio_quality,
            query.track_id,
        )
        .await?,
    ))
}

impl From<TidalFavoriteAlbumsError> for actix_web::Error {
    fn from(err: TidalFavoriteAlbumsError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteAlbumsQuery {
    offset: Option<u32>,
    limit: Option<u32>,
    order: Option<TidalAlbumOrder>,
    order_direction: Option<TidalAlbumOrderDirection>,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<TidalDeviceType>,
}

#[route("/tidal/favorites/albums", method = "GET")]
pub async fn tidal_favorite_albums_endpoint(
    query: web::Query<TidalFavoriteAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<ApiTidalAlbum>>> {
    Ok(Json(
        tidal_favorite_albums(
            &data
                .db
                .clone()
                .expect("Db not set")
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            query.offset,
            query.limit,
            query.order,
            query.order_direction,
            query.country_code.clone(),
            query.locale.clone(),
            query.device_type,
        )
        .await?,
    ))
}
