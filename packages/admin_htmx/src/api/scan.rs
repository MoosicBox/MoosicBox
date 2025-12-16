//! Local music library scanning endpoints for the admin interface.
//!
//! Provides endpoints for managing scan paths and triggering local music library scans.

#![allow(clippy::module_name_repetitions)]

use actix_htmx::Htmx;
use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web,
};
use maud::{Markup, html};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::MusicApis;
use moosicbox_scan::ScanOrigin;
use serde::Deserialize;
use switchy_database::profiles::LibraryDatabase;

use crate::api::util::clear_input;

/// Binds scan-related endpoints to the provided Actix web scope.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(add_scan_paths_endpoint)
        .service(delete_scan_paths_endpoint)
        .service(start_scan_endpoint)
        .service(get_scans_endpoint)
}

/// Form data for adding a scan path.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathForm {
    /// The filesystem path to add for scanning.
    path: String,
}

/// Endpoint that adds a new scan path to the database.
///
/// # Errors
///
/// * If fails to add the scan path to the database
/// * If fails to render the updated scan paths list
#[route("scan-paths", method = "POST")]
pub async fn add_scan_paths_endpoint(
    _htmx: Htmx,
    form: web::Form<AddScanPathForm>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::add_scan_path(&db, &form.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))?;

    scan_paths(&db)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))
}

/// Query parameters for removing a scan path.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    /// The filesystem path to remove from scanning.
    path: String,
}

/// Endpoint that removes a scan path from the database.
///
/// # Errors
///
/// * If fails to remove the scan path from the database
#[route("scan-paths", method = "DELETE")]
pub async fn delete_scan_paths_endpoint(
    _htmx: Htmx,
    query: web::Query<RemoveScanPathQuery>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::remove_scan_path(&db, &query.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove scan path: {e:?}")))?;

    Ok(html! {})
}

/// Endpoint that triggers a local music library scan.
///
/// # Errors
///
/// * If the scan fails to start or encounters errors during execution
#[route("run-scan", method = "POST")]
pub async fn start_scan_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::run_scan(Some(vec![ScanOrigin::Local]), &db, music_apis)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))?;

    Ok(html! {})
}

/// Endpoint that renders the scan configuration and controls.
///
/// # Errors
///
/// * If fails to fetch scan paths or render the scan UI
#[route("scans", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_scans_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    scan(&db)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))
}

/// Renders the table of configured scan paths.
///
/// # Errors
///
/// * If fails to get the scan paths from the database
pub async fn scan_paths(db: &LibraryDatabase) -> Result<Markup, DatabaseFetchError> {
    let paths = moosicbox_scan::get_scan_paths(db).await?;

    Ok(html! {
        table id="scan-paths" {
            tbody {
                @for path in paths {
                    tr {
                        td { (path) }
                        td {
                            form hx-delete="/admin/scan-paths" hx-target="closest tr" {
                                input type="hidden" name="path" value=(path);
                                button type="submit" { "Remove" }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Renders the complete scan management interface including paths, add form, and scan button.
///
/// # Errors
///
/// * If the `scan_paths` fails to render
pub async fn scan(db: &LibraryDatabase) -> Result<Markup, DatabaseFetchError> {
    Ok(html! {
        (scan_paths(db).await?)
        form
            hx-post="/admin/scan-paths"
            hx-target="#scan-paths"
            hx-on--after-request=(clear_input("#new-scan-path"))
            {
                input id="new-scan-path" type="text" name="path";
                button type="submit" { "Add new scan source" }
            }
        form
            hx-post="/admin/run-scan?origins=LOCAL"
            hx-target="#run-scan-button"
            hx-disabled-elt="#run-scan-button"
            hx-swap="none" {
            button id="run-scan-button" type="submit" { "Run Scan" }
        }
    })
}
