use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::Database;
use moosicbox_music_api::MusicApiState;
use moosicbox_scan::ScanOrigin;
use serde::Deserialize;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(add_scan_paths_endpoint)
        .service(delete_scan_paths_endpoint)
        .service(start_scan_endpoint)
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
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::add_scan_path(&**data.database, &form.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))?;

    scan_paths(&**data.database)
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
    form: web::Query<RemoveScanPathQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::remove_scan_path(&**data.database, &form.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove scan path: {e:?}")))?;

    Ok(html! {})
}

#[route("run-scan", method = "POST")]
pub async fn start_scan_endpoint(
    _htmx: Htmx,
    data: web::Data<moosicbox_core::app::AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::run_scan(
        Some(vec![ScanOrigin::Local]),
        data.database.clone(),
        api_state.as_ref().clone(),
    )
    .await
    .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))?;

    Ok(html! {})
}

pub async fn scan_paths(db: &dyn Database) -> Result<Markup, DbError> {
    let paths = moosicbox_scan::get_scan_paths(db).await?;

    Ok(html! {
        div id="scan-paths" {
            @for path in paths {
                form hx-delete="/admin/scan-paths" {
                    div.scan-path {
                        p { (path) }
                        input type="hidden" name="path" value=(path) {}
                        button type="submit" { "Remove" }
                    }
                }
            }
        }
    })
}

pub async fn scan(db: &dyn Database) -> Result<Markup, DbError> {
    Ok(html! {
        (scan_paths(db).await?)
        form
            hx-post="/admin/scan-paths"
            hx-target="#scan-paths"
            hx-on--after-request="document.querySelector('#new-scan-path').value = ''"
            {
                input id="new-scan-path" type="text" name="path" {}
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
