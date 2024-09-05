use std::str::FromStr;

use actix_web::{
    delete,
    error::{ErrorBadRequest, ErrorInternalServerError},
    post,
    web::{self, Json},
    Result,
};
use moosicbox_auth::NonTunnelRequestAuthorized;
use moosicbox_core::app::AppState;
use moosicbox_music_api::MusicApiState;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    disable_scan_origin, enable_scan_origin, get_origins_or_default, ScanError, ScanOrigin, Scanner,
};

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Scan")),
    paths(
        run_scan_endpoint,
        start_scan_endpoint,
        run_scan_path_endpoint,
        get_scan_origins_endpoint,
        enable_scan_origin_endpoint,
        disable_scan_origin_endpoint,
        get_scan_paths_endpoint,
        add_scan_path_endpoint,
        remove_scan_path_endpoint,
    ),
    components(schemas(
        ScanOrigin,
    ))
)]
pub struct Api;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    origins: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/run-scan",
        description = "Run a scan for the specified origin(s)",
        params(
            ("origins" = Option<String>, Query, description = "Comma-separated list of ScanOrigins"),
        ),
        responses(
            (
                status = 200,
                description = "The scan has successfully ran",
                body = Value,
            )
        )
    )
)]
#[post("/run-scan")]
pub async fn run_scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
    _: NonTunnelRequestAuthorized,
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

    let db = data.database.clone();
    let origins = get_origins_or_default(&**db, origins).await?;

    for origin in origins {
        Scanner::from_origin(&**db, origin)
            .await?
            .scan(api_state.as_ref().clone(), db.clone())
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;
    }

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/start-scan",
        description = "Start a scan for the specified origin(s)",
        params(
            ("origins" = Option<String>, Query, description = "Comma-separated list of ScanOrigins"),
        ),
        responses(
            (
                status = 200,
                description = "The scan has successfully started",
                body = Value,
            )
        )
    )
)]
#[post("/start-scan")]
pub async fn start_scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
    _: NonTunnelRequestAuthorized,
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

    let db = data.database.clone();
    let origins = get_origins_or_default(&**db, origins).await?;

    moosicbox_task::spawn("scan", async move {
        for origin in origins {
            Scanner::from_origin(&**db, origin)
                .await?
                .scan(api_state.as_ref().clone(), db.clone())
                .await?;
        }

        Ok::<_, ScanError>(())
    });

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/run-scan-path",
        description = "Run a local scan on the specific path",
        params(
            ("path" = String, Query, description = "Local file path to scan"),
        ),
        responses(
            (
                status = 200,
                description = "The scan has successfully ran",
                body = Value,
            )
        )
    )
)]
#[post("/run-scan-path")]
pub async fn run_scan_path_endpoint(
    query: web::Query<ScanPathQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let scanner = Scanner::new(crate::event::ScanTask::Local {
        paths: vec![query.path.clone()],
    })
    .await;

    scanner
        .scan(api_state.as_ref().clone(), data.database.clone())
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    crate::local::scan(
        &query.path,
        data.database.clone(),
        crate::CANCELLATION_TOKEN.clone(),
        scanner,
    )
    .await
    .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanOriginsQuery {}

#[cfg(feature = "local")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        get,
        path = "/scan-origins",
        description = "Get the enabled scan origins",
        params(),
        responses(
            (
                status = 200,
                description = "The enabled scan origins",
                body = Value,
            )
        )
    )
)]
#[actix_web::get("/scan-origins")]
pub async fn get_scan_origins_endpoint(
    _query: web::Query<GetScanOriginsQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = crate::get_scan_origins(&**data.database)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan origins: {e:?}")))?;

    Ok(Json(serde_json::json!({"origins": origins})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanOriginQuery {
    origin: ScanOrigin,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/scan-origins",
        description = "Enable a scan origin",
        params(
            ("origin" = ScanOrigin, Query, description = "ScanOrigin to enable"),
        ),
        responses(
            (
                status = 200,
                description = "The ScanOrigin was successfully enabled",
                body = Value,
            )
        )
    )
)]
#[post("/scan-origins")]
pub async fn enable_scan_origin_endpoint(
    query: web::Query<EnableScanOriginQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    enable_scan_origin(&**data.database, query.origin)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to enable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisableScanOriginQuery {
    origin: ScanOrigin,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        delete,
        path = "/scan-origins",
        description = "Disable a scan origin",
        params(
            ("origin" = ScanOrigin, Query, description = "ScanOrigin to disable"),
        ),
        responses(
            (
                status = 200,
                description = "The ScanOrigin was successfully disabled",
                body = Value,
            )
        )
    )
)]
#[delete("/scan-origins")]
pub async fn disable_scan_origin_endpoint(
    query: web::Query<DisableScanOriginQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    disable_scan_origin(&**data.database, query.origin)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to disable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanPathsQuery {}

#[cfg(feature = "local")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        get,
        path = "/scan-paths",
        description = "Get the enabled local scan paths",
        params(),
        responses(
            (
                status = 200,
                description = "The enabled local scan paths",
                body = Value,
            )
        )
    )
)]
#[actix_web::get("/scan-paths")]
pub async fn get_scan_paths_endpoint(
    _query: web::Query<GetScanPathsQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let paths = crate::get_scan_paths(&**data.database)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan paths: {e:?}")))?;

    Ok(Json(serde_json::json!({"paths": paths})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/scan-paths",
        description = "Enable a local scan path",
        params(
            ("origin" = String, Query, description = "Local scan path to enable"),
        ),
        responses(
            (
                status = 200,
                description = "The local scan path was successfully enabled",
                body = Value,
            )
        )
    )
)]
#[post("/scan-paths")]
pub async fn add_scan_path_endpoint(
    query: web::Query<AddScanPathQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    static REGEX: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"/mnt/(\w+)").unwrap());

    let path = if std::env::consts::OS == "windows" {
        REGEX
            .replace(&query.path, |caps: &regex::Captures| {
                format!("{}:", caps[1].to_uppercase())
            })
            .replace('/', "\\")
    } else {
        query.path.clone()
    };

    crate::add_scan_path(&**data.database, &path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        delete,
        path = "/scan-paths",
        description = "Disable a local scan path",
        params(
            ("origin" = String, Query, description = "Local scan path to disable"),
        ),
        responses(
            (
                status = 200,
                description = "The local scan path was successfully disabled",
                body = Value,
            )
        )
    )
)]
#[delete("/scan-paths")]
pub async fn remove_scan_path_endpoint(
    query: web::Query<RemoveScanPathQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    crate::remove_scan_path(&**data.database, &query.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}
