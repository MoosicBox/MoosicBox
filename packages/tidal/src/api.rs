use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    device_authorization, device_authorization_token, favorite_albums, track_url, ApiTidalAlbum,
    TidalAlbumOrder, TidalAlbumOrderDirection, TidalAudioQuality, TidalDeviceAuthorizationError,
    TidalDeviceAuthorizationTokenError, TidalDeviceType, TidalFavoriteAlbumsError,
    TidalTrackUrlError,
};

impl From<TidalDeviceAuthorizationError> for actix_web::Error {
    fn from(err: TidalDeviceAuthorizationError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

static TIDAL_ACCESS_TOKEN_HEADER: &str = "x-tidal-access-token";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationQuery {
    client_id: String,
}

#[route("/tidal/auth/device-authorization", method = "POST")]
pub async fn device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    Ok(Json(device_authorization(query.client_id.clone()).await?))
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
    #[cfg(feature = "db")]
    persist: Option<bool>,
}

#[route("/tidal/auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    query: web::Query<TidalDeviceAuthorizationTokenQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        device_authorization_token(
            #[cfg(feature = "db")]
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
            #[cfg(feature = "db")]
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
pub async fn track_url_endpoint(
    req: HttpRequest,
    query: web::Query<TidalTrackUrlQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    Ok(Json(
        track_url(
            #[cfg(feature = "db")]
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
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
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
    user_id: Option<u32>,
}

#[route("/tidal/favorites/albums", method = "GET")]
pub async fn favorite_albums_endpoint(
    req: HttpRequest,
    query: web::Query<TidalFavoriteAlbumsQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Vec<ApiTidalAlbum>>> {
    Ok(Json(
        favorite_albums(
            #[cfg(feature = "db")]
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
            req.headers()
                .get(TIDAL_ACCESS_TOKEN_HEADER)
                .map(|x| x.to_str().unwrap().to_string()),
            query.user_id,
        )
        .await?,
    ))
}
