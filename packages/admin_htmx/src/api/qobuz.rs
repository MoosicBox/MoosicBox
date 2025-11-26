//! Qobuz music service integration endpoints for the admin interface.
//!
//! Provides endpoints for Qobuz user authentication, settings management, and library scanning.

use actix_htmx::{Htmx, TriggerPayload, TriggerType};
use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web,
};
use maud::{Markup, html};
use moosicbox_json_utils::database::DatabaseFetchError;
#[cfg(feature = "scan")]
use moosicbox_music_api::MusicApis;
#[cfg(feature = "scan")]
use moosicbox_scan::ScanOrigin;
use serde::Deserialize;
use switchy_database::profiles::LibraryDatabase;

/// Binds Qobuz authentication and settings endpoints to the provided Actix web scope.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    let nested = web::scope("/qobuz");
    let nested = nested
        .service(get_settings_endpoint)
        .service(user_login_endpoint);

    #[cfg(feature = "scan")]
    let nested = nested.service(run_scan_endpoint);

    scope.service(nested)
}

/// Form data for Qobuz user login.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLoginForm {
    /// Qobuz account username.
    username: String,
    /// Qobuz account password.
    password: String,
}

/// Endpoint that handles Qobuz user login.
///
/// # Errors
///
/// This endpoint does not return errors; failures are rendered as HTML with error messages.
#[route("auth/user-login", method = "POST")]
pub async fn user_login_endpoint(
    htmx: Htmx,
    form: web::Form<UserLoginForm>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    let response =
        moosicbox_qobuz::user_login(&db, &form.username, &form.password, None, Some(true)).await;

    Ok(if response.is_ok_and(|x| x.get("accessToken").is_some()) {
        htmx.trigger_event(
            "qobuz-login-attempt".to_string(),
            Some(
                TriggerPayload::json(serde_json::json!({
                    "level": "info",
                    "message": "Successfully logged in to Qobuz",
                    "success": true
                }))
                .unwrap(),
            ),
            Some(TriggerType::Standard),
        );

        settings_logged_in(
            #[cfg(feature = "scan")]
            false,
        )
    } else {
        htmx.trigger_event(
            "qobuz-login-attempt".to_string(),
            Some(
                TriggerPayload::json(serde_json::json!({
                    "level": "info",
                    "message": "Failed to login to Qobuz",
                    "success": false
                }))
                .unwrap(),
            ),
            Some(TriggerType::Standard),
        );

        settings_logged_out(Some(html! { p { "Invalid username/password" } }))
    })
}

/// Query parameters for the Qobuz settings endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSettingsQuery {
    /// Whether to show scan controls in the settings UI.
    #[cfg(feature = "scan")]
    show_scan: Option<bool>,
}

/// Endpoint that renders the Qobuz settings and authentication status.
///
/// # Errors
///
/// * If fails to fetch the Qobuz config from the database
#[route("settings", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_settings_endpoint(
    _htmx: Htmx,
    #[allow(unused)] query: web::Query<GetSettingsQuery>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    settings(
        &db,
        #[cfg(feature = "scan")]
        query.show_scan.unwrap_or_default(),
    )
    .await
    .map_err(|e| ErrorInternalServerError(format!("Failed to get Qobuz settings: {e:?}")))
}

/// Endpoint that triggers a scan of the Qobuz music library.
///
/// # Errors
///
/// * If the Qobuz API source is not registered
/// * If the scan fails to start or encounters errors during execution
#[cfg(feature = "scan")]
#[route("run-scan", method = "POST")]
pub async fn run_scan_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Markup, actix_web::Error> {
    let api_source = ScanOrigin::for_api_source("Qobuz")
        .ok_or_else(|| ErrorInternalServerError("Qobuz ApiSource is not registered"))?;

    moosicbox_scan::run_scan(Some(vec![api_source]), &db, music_apis)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))?;

    Ok(html! {})
}

/// Renders the Qobuz settings UI when the user is logged in.
#[must_use]
pub fn settings_logged_in(#[cfg(feature = "scan")] show_scan: bool) -> Markup {
    #[cfg(feature = "scan")]
    let scan = if show_scan {
        html! {
            form
                hx-post="/admin/qobuz/run-scan"
                hx-target="#run-scan-button"
                hx-disabled-elt="#run-scan-button"
                hx-swap="none" {
                button id="run-scan-button" type="submit" { "Run Scan" }
            }
        }
    } else {
        html! {}
    };
    #[cfg(not(feature = "scan"))]
    let scan = html! {};

    html! {
        p { "Logged in!" }
        (scan)
    }
}

/// Renders the Qobuz settings UI when the user is not logged in.
#[must_use]
pub fn settings_logged_out(message: Option<Markup>) -> Markup {
    html! {
        form hx-post="/admin/qobuz/auth/user-login" hx-swap="outerHTML" {
            (message.unwrap_or_default())
            input type="text" name="username" placeholder="username..." autocomplete="username";
            input type="password" name="password" placeholder="password..." autocomplete="current-password";
            button type="submit" { "Login" }
        }
    }
}

/// Renders the Qobuz settings UI based on authentication status.
///
/// # Errors
///
/// * If fails to fetch the qobuz config from the database
pub async fn settings(
    db: &LibraryDatabase,
    #[cfg(feature = "scan")] show_scan: bool,
) -> Result<Markup, DatabaseFetchError> {
    let logged_in = moosicbox_qobuz::db::get_qobuz_config(db).await?.is_some();

    Ok(if logged_in {
        settings_logged_in(
            #[cfg(feature = "scan")]
            show_scan,
        )
    } else {
        settings_logged_out(None)
    })
}
