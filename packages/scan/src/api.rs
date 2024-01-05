use std::str::FromStr;

use actix_web::{
    delete,
    error::{ErrorBadRequest, ErrorInternalServerError},
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    add_scan_path, disable_scan_origin, enable_scan_origin, get_scan_origins, get_scan_paths,
    remove_scan_path, scan, ScanOrigin, CANCELLATION_TOKEN,
};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    origins: Option<String>,
}

#[post("/run-scan")]
pub async fn run_scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let origins = query
        .origins
        .as_ref()
        .map(|origins| {
            origins
                .split(',')
                .map(|s| s.trim())
                .map(|s| {
                    ScanOrigin::from_str(s)
                        .map_err(|_e| ErrorBadRequest(format!("Invalid ScanOrigin value: {s}")))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    scan(data.db.as_ref().unwrap(), origins)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanPathQuery {
    path: String,
}

#[post("/run-scan-path")]
pub async fn run_scan_path_endpoint(
    query: web::Query<ScanPathQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    #[cfg(feature = "local")]
    crate::local::scan(
        &query.path,
        data.db.as_ref().unwrap(),
        CANCELLATION_TOKEN.clone(),
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanOriginsQuery {}

#[get("/scan-origins")]
pub async fn get_scan_origins_endpoint(
    _query: web::Query<GetScanOriginsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let origins = get_scan_origins(&data.db.as_ref().unwrap().library.lock().unwrap())
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan origins: {e:?}")))?;

    Ok(Json(serde_json::json!({"origins": origins})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanOriginQuery {
    origin: ScanOrigin,
}

#[post("/scan-origins")]
pub async fn enable_scan_origin_endpoint(
    query: web::Query<EnableScanOriginQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    enable_scan_origin(
        &data.db.as_ref().unwrap().library.lock().unwrap(),
        query.origin,
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to enable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisableScanOriginQuery {
    origin: ScanOrigin,
}

#[delete("/scan-origins")]
pub async fn disable_scan_origin_endpoint(
    query: web::Query<DisableScanOriginQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    disable_scan_origin(
        &data.db.as_ref().unwrap().library.lock().unwrap(),
        query.origin,
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to disable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanPathsQuery {}

#[get("/scan-paths")]
pub async fn get_scan_paths_endpoint(
    _query: web::Query<GetScanPathsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let paths = get_scan_paths(&data.db.as_ref().unwrap().library.lock().unwrap())
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan paths: {e:?}")))?;

    Ok(Json(serde_json::json!({"paths": paths})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathQuery {
    path: String,
}

#[post("/scan-paths")]
pub async fn add_scan_path_endpoint(
    query: web::Query<AddScanPathQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    add_scan_path(
        &data.db.as_ref().unwrap().library.lock().unwrap(),
        &query.path,
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    path: String,
}

#[delete("/scan-paths")]
pub async fn remove_scan_path_endpoint(
    query: web::Query<RemoveScanPathQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    remove_scan_path(
        &data.db.as_ref().unwrap().library.lock().unwrap(),
        &query.path,
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to remove scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}
