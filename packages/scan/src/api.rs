use actix_web::{
    error::ErrorInternalServerError,
    post,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::Value;

use crate::{local::scan, CANCELLATION_TOKEN};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    location: String,
}

#[post("/scan")]
pub async fn scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    scan(&query.location, &data, CANCELLATION_TOKEN.clone())
        .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}
