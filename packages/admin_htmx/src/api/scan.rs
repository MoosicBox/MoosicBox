#![allow(clippy::module_name_repetitions)]

use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::MusicApis;
use moosicbox_scan::ScanOrigin;
use serde::Deserialize;

use crate::api::util::clear_input;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathForm {
    path: String,
}

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    path: String,
}

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

#[route("scans", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_scans_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    scan(&db)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))
}

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
