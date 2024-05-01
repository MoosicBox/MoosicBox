use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    HttpRequest, Result,
};
use serde::Deserialize;

use crate::{models::UpnpDevice, scan_devices, ScanError};

impl From<ScanError> for actix_web::Error {
    fn from(err: ScanError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDevicesQuery {}

#[route("/upnp/scan-devices", method = "GET")]
pub async fn scan_devices_endpoint(
    _req: HttpRequest,
    _query: web::Query<ScanDevicesQuery>,
    #[cfg(feature = "db")] data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Vec<UpnpDevice>>> {
    Ok(Json(scan_devices().await?))
}
