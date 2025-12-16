//! HTTP API endpoints for music library scanning.
//!
//! This module provides REST API endpoints for initiating scans, managing
//! scan origins, and configuring scan paths.

#![allow(clippy::needless_for_each)]

use std::str::FromStr;

use actix_web::{
    Result, Scope, delete,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError},
    post,
    web::{self, Json},
};
use moosicbox_auth::NonTunnelRequestAuthorized;
use moosicbox_music_api::MusicApis;
use serde::Deserialize;
use serde_json::Value;
use switchy_database::profiles::LibraryDatabase;

use crate::{ScanError, ScanOrigin, disable_scan_origin, enable_scan_origin, run_scan};

/// Validates a path to prevent path traversal attacks.
///
/// This function canonicalizes the path and checks for common path traversal patterns.
/// Returns the canonicalized path string if valid, or an error if the path is unsafe.
///
/// # Errors
///
/// * If the path contains traversal sequences like `..`
/// * If the path cannot be canonicalized (doesn't exist or isn't accessible)
#[cfg(feature = "local")]
fn validate_path(path: &str) -> Result<String, actix_web::Error> {
    // Check for obvious path traversal patterns in the raw input
    if path.contains("..") {
        return Err(ErrorBadRequest("Path traversal detected: '..' not allowed"));
    }

    // Canonicalize the path to resolve symlinks and normalize the path
    let path_buf = std::path::Path::new(path);
    let canonical = path_buf
        .canonicalize()
        .map_err(|e| ErrorBadRequest(format!("Invalid path: {e}")))?;

    // Convert back to string and verify no traversal occurred
    let canonical_str = canonical
        .to_str()
        .ok_or_else(|| ErrorBadRequest("Path contains invalid UTF-8"))?;

    // Double-check the canonicalized path doesn't contain traversal patterns
    if canonical_str.contains("..") {
        return Err(ErrorBadRequest(
            "Path traversal detected after canonicalization",
        ));
    }

    Ok(canonical_str.to_string())
}

/// Binds all scan-related API endpoints to an Actix-Web scope.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    let scope = scope
        .service(run_scan_endpoint)
        .service(start_scan_endpoint)
        .service(enable_scan_origin_endpoint)
        .service(disable_scan_origin_endpoint);

    #[cfg(feature = "local")]
    let scope = scope
        .service(get_scan_origins_endpoint)
        .service(run_scan_path_endpoint)
        .service(get_scan_paths_endpoint)
        .service(add_scan_path_endpoint)
        .service(remove_scan_path_endpoint);

    scope
}

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
        crate::models::api::ApiScanPath,
    ))
)]
/// `OpenAPI` documentation structure for scan endpoints.
pub struct Api;

/// Query parameters for scan endpoints.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    /// Comma-separated list of scan origins to scan.
    origins: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/run-scan",
        description = "Run a scan for the specified origin(s)",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
/// Runs a synchronous scan for the specified origin(s), blocking until complete.
#[post("/run-scan")]
pub async fn run_scan_endpoint(
    query: web::Query<ScanQuery>,
    db: LibraryDatabase,
    music_apis: MusicApis,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = query
        .origins
        .as_ref()
        .map(|origins| {
            origins
                .split(',')
                .map(str::trim)
                .map(|s| {
                    ScanOrigin::from_str(s)
                        .map_err(|_e| ErrorBadRequest(format!("Invalid ScanOrigin value: {s}")))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    run_scan(origins, &db, music_apis).await.map_err(|e| {
        log::error!("Failed to scan: {e:?}");
        ErrorInternalServerError("Failed to scan")
    })?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/start-scan",
        description = "Start a scan for the specified origin(s)",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
/// Starts an asynchronous scan for the specified origin(s) in the background.
#[post("/start-scan")]
pub async fn start_scan_endpoint(
    query: web::Query<ScanQuery>,
    db: LibraryDatabase,
    music_apis: MusicApis,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = query
        .origins
        .as_ref()
        .map(|origins| {
            origins
                .split(',')
                .map(str::trim)
                .map(|s| {
                    ScanOrigin::from_str(s)
                        .map_err(|_e| ErrorBadRequest(format!("Invalid ScanOrigin value: {s}")))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    switchy_async::runtime::Handle::current().spawn_with_name("scan", async move {
        run_scan(origins, &db, music_apis).await.map_err(|e| {
            moosicbox_assert::die_or_error!("Scan error: {e:?}");
            e
        })?;

        Ok::<_, ScanError>(())
    });

    Ok(Json(serde_json::json!({"success": true})))
}

/// Query parameters for local path scan endpoint.
#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanPathQuery {
    /// Filesystem path to scan.
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
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
/// Runs a local filesystem scan on a specific path.
#[post("/run-scan-path")]
pub async fn run_scan_path_endpoint(
    query: web::Query<ScanPathQuery>,
    db: LibraryDatabase,
    music_apis: MusicApis,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    // Validate and canonicalize the path to prevent path traversal attacks
    let validated_path = validate_path(&query.path)?;

    let scanner = crate::Scanner::new(crate::event::ScanTask::Local {
        paths: vec![validated_path.clone()],
    });

    scanner.scan(music_apis, &db).await.map_err(|e| {
        log::error!("Failed to scan: {e:?}");
        ErrorInternalServerError("Failed to scan")
    })?;

    crate::local::scan(
        &validated_path,
        &db,
        crate::CANCELLATION_TOKEN.clone(),
        scanner,
    )
    .await
    .map_err(|e| {
        log::error!("Scan failed: {e:?}");
        ErrorInternalServerError("Failed to scan")
    })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Query parameters for the get scan origins endpoint.
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
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
        ),
        responses(
            (
                status = 200,
                description = "The enabled scan origins",
                body = Value,
            )
        )
    )
)]
/// Retrieves all enabled scan origins.
#[actix_web::get("/scan-origins")]
pub async fn get_scan_origins_endpoint(
    _query: web::Query<GetScanOriginsQuery>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = crate::get_scan_origins(&db).await.map_err(|e| {
        log::error!("Failed to get scan origins: {e:?}");
        ErrorInternalServerError("Failed to get scan origins")
    })?;

    Ok(Json(serde_json::json!({"origins": origins})))
}

/// Query parameters for the enable scan origin endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanOriginQuery {
    /// The scan origin to enable.
    origin: ScanOrigin,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        post,
        path = "/scan-origins",
        description = "Enable a scan origin",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
/// Enables a scan origin for future scans.
#[post("/scan-origins")]
pub async fn enable_scan_origin_endpoint(
    query: web::Query<EnableScanOriginQuery>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    enable_scan_origin(&db, &query.origin).await.map_err(|e| {
        log::error!("Failed to enable scan origin: {e:?}");
        ErrorInternalServerError("Failed to enable scan origin")
    })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Query parameters for the disable scan origin endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisableScanOriginQuery {
    /// The scan origin to disable.
    origin: ScanOrigin,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Scan"],
        delete,
        path = "/scan-origins",
        description = "Disable a scan origin",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
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
/// Disables a scan origin to prevent future scans.
#[delete("/scan-origins")]
pub async fn disable_scan_origin_endpoint(
    query: web::Query<DisableScanOriginQuery>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    disable_scan_origin(&db, &query.origin).await.map_err(|e| {
        log::error!("Failed to disable scan origin: {e:?}");
        ErrorInternalServerError("Failed to disable scan origin")
    })?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
/// Query parameters for the get scan paths endpoint.
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
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
        ),
        responses(
            (
                status = 200,
                description = "The enabled local scan paths",
                body = Vec<crate::models::api::ApiScanPath>,
            )
        )
    )
)]
/// Retrieves all enabled local filesystem scan paths.
#[actix_web::get("/scan-paths")]
pub async fn get_scan_paths_endpoint(
    _query: web::Query<GetScanPathsQuery>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Vec<crate::models::api::ApiScanPath>>> {
    let paths = crate::get_scan_paths(&db)
        .await
        .map_err(|e| {
            log::error!("Failed to get scan paths: {e:?}");
            ErrorInternalServerError("Failed to get scan paths")
        })?
        .into_iter()
        .map(|x| crate::models::api::ApiScanPath { path: x })
        .collect();

    Ok(Json(paths))
}

#[cfg(feature = "local")]
/// Query parameters for the add scan path endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathQuery {
    /// The filesystem path to add.
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
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("path" = String, Query, description = "Local scan path to enable"),
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
/// Enables a local filesystem path for scanning.
#[post("/scan-paths")]
pub async fn add_scan_path_endpoint(
    query: web::Query<AddScanPathQuery>,
    db: LibraryDatabase,
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

    // Validate and canonicalize the path to prevent path traversal attacks
    let validated_path = validate_path(&path)?;

    crate::add_scan_path(&db, &validated_path)
        .await
        .map_err(|e| {
            log::error!("Failed to add scan path: {e:?}");
            ErrorInternalServerError("Failed to add scan path")
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
/// Query parameters for the remove scan path endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    /// The filesystem path to remove.
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
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("path" = String, Query, description = "Local scan path to disable"),
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
/// Disables a local filesystem path to prevent scanning.
#[delete("/scan-paths")]
pub async fn remove_scan_path_endpoint(
    query: web::Query<RemoveScanPathQuery>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    crate::remove_scan_path(&db, &query.path)
        .await
        .map_err(|e| {
            log::error!("Failed to remove scan path: {e:?}");
            ErrorInternalServerError("Failed to remove scan path")
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}
