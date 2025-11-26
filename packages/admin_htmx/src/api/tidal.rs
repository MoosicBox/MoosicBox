//! Tidal music service integration endpoints for the admin interface.
//!
//! Provides endpoints for Tidal authentication (device authorization flow), settings
//! management, and library scanning.

use std::sync::LazyLock;

use actix_htmx::{Htmx, TriggerPayload, TriggerType};
use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web,
};
use base64::{Engine as _, engine::general_purpose};
use maud::{Markup, html};
#[cfg(feature = "scan")]
use moosicbox_music_api::MusicApis;
#[cfg(feature = "scan")]
use moosicbox_scan::ScanOrigin;
use moosicbox_tidal::db::GetTidalConfigError;
use serde::Deserialize;
use switchy_database::profiles::LibraryDatabase;
use urlencoding::encode;

static C1: &str = "elU0WEhWVms=";
static C2: &str = "YzJ0RFBvNHQ=";
static SP1: &str = "VkpLaERGcUpQcXZzUFZOQlY2dWtYVA==";
static SP2: &str = "Sm13bHZidHRQN3dsTWxyYzcyc2U0PQ==";

/// Binds Tidal authentication and settings endpoints to the provided Actix web scope.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    let nested = web::scope("/tidal")
        .service(get_settings_endpoint)
        .service(device_authorization_endpoint)
        .service(device_authorization_token_endpoint);

    #[cfg(feature = "scan")]
    let nested = nested.service(run_scan_endpoint);

    scope.service(nested)
}

static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}{}",
        std::str::from_utf8(&general_purpose::STANDARD.decode(C1).unwrap()).unwrap(),
        std::str::from_utf8(&general_purpose::STANDARD.decode(C2).unwrap()).unwrap()
    )
});

static CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}{}",
        std::str::from_utf8(&general_purpose::STANDARD.decode(SP1).unwrap()).unwrap(),
        std::str::from_utf8(&general_purpose::STANDARD.decode(SP2).unwrap()).unwrap()
    )
});

/// Endpoint that initiates Tidal device authorization flow.
///
/// # Errors
///
/// * If the device authorization request to Tidal API fails
/// * If the response is missing required fields
#[route("auth/device-authorization", method = "POST")]
pub async fn device_authorization_endpoint(
    htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    let response = moosicbox_tidal::device_authorization(CLIENT_ID.clone(), false)
        .await
        .map_err(ErrorInternalServerError)?;
    let device_code = response
        .get("deviceCode")
        .ok_or_else(|| ErrorInternalServerError("Missing device code"))?
        .as_str()
        .ok_or_else(|| ErrorInternalServerError("Invalid deviceCode"))?;
    let url = response
        .get("url")
        .ok_or_else(|| ErrorInternalServerError("Missing url"))?
        .as_str()
        .ok_or_else(|| ErrorInternalServerError("Invalid url"))?;

    device_authorization_token(&db, htmx, device_code, url)
        .await
        .map_err(ErrorInternalServerError)
}

/// Query parameters for polling device authorization token.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorizationTokenQuery {
    /// The device code from the authorization request.
    device_code: String,
    /// The URL the user should visit to authorize.
    url: String,
}

/// Endpoint that polls for Tidal device authorization token.
///
/// # Errors
///
/// * If the token request fails or token is not yet available
#[route("auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    htmx: Htmx,
    query: web::Query<DeviceAuthorizationTokenQuery>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    device_authorization_token(&db, htmx, &query.device_code, &query.url)
        .await
        .map_err(ErrorInternalServerError)
}

async fn device_authorization_token(
    db: &LibraryDatabase,
    htmx: Htmx,
    device_code: &str,
    url: &str,
) -> Result<Markup, moosicbox_tidal::Error> {
    let response = moosicbox_tidal::device_authorization_token(
        db,
        CLIENT_ID.clone(),
        CLIENT_SECRET.clone(),
        device_code.to_owned(),
        Some(true),
    )
    .await;

    if response.is_ok_and(|x| x.get("accessToken").is_some()) {
        htmx.trigger_event(
            "tidal-login-attempt".to_string(),
            Some(
                TriggerPayload::json(serde_json::json!({
                    "level": "info",
                    "message": "Successfully logged in to Tidal",
                    "success": true
                }))
                .unwrap(),
            ),
            Some(TriggerType::Standard),
        );

        Ok(settings_logged_in(
            #[cfg(feature = "scan")]
            false,
        ))
    } else {
        htmx.trigger_event(
            "tidal-login-attempt".to_string(),
            Some(
                TriggerPayload::json(serde_json::json!({
                    "level": "info",
                    "message": "Failed to login to Tidal",
                    "success": false
                }))
                .unwrap(),
            ),
            Some(TriggerType::Standard),
        );

        Ok(html! {
            div
                hx-post={"/admin/tidal/auth/device-authorization/token?deviceCode="(encode(device_code))"&url="(encode(url))}
                hx-swap="outerHTML"
                hx-trigger="every 1s" {
                p {
                    "Follow this link to authenticate with Tidal: " a href=(url) target="_blank" { (url) }
                }
            }
        })
    }
}

/// Query parameters for the Tidal settings endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSettingsQuery {
    /// Whether to show scan controls in the settings UI.
    #[cfg(feature = "scan")]
    show_scan: Option<bool>,
}

/// Endpoint that renders the Tidal settings and authentication status.
///
/// # Errors
///
/// * If fails to fetch the Tidal config from the database
#[route("settings", method = "GET")]
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
    .map_err(|e| ErrorInternalServerError(format!("Failed to get Tidal settings: {e:?}")))
}

/// Endpoint that triggers a scan of the Tidal music library.
///
/// # Errors
///
/// * If the Tidal API source is not registered
/// * If the scan fails to start or encounters errors during execution
#[cfg(feature = "scan")]
#[route("run-scan", method = "POST")]
pub async fn run_scan_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Markup, actix_web::Error> {
    let api_source = ScanOrigin::for_api_source("Tidal")
        .ok_or_else(|| ErrorInternalServerError("Tidal ApiSource is not registered"))?;

    moosicbox_scan::run_scan(Some(vec![api_source]), &db, music_apis)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))?;

    Ok(html! {})
}

/// Renders the Tidal settings UI when the user is logged in.
#[must_use]
pub fn settings_logged_in(#[cfg(feature = "scan")] show_scan: bool) -> Markup {
    #[cfg(feature = "scan")]
    let scan = if show_scan {
        html! {
            form
                hx-post="/admin/tidal/run-scan"
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

/// Renders the Tidal settings UI when the user is not logged in.
#[must_use]
pub fn settings_logged_out() -> Markup {
    html! {
        form hx-post="/admin/tidal/auth/device-authorization" hx-swap="outerHTML" {
            button type="submit" { "Start web authentication" }
        }
    }
}

/// Renders the Tidal settings UI based on authentication status.
///
/// # Errors
///
/// * If fails to fetch the Tidal config from the database
pub async fn settings(
    db: &LibraryDatabase,
    #[cfg(feature = "scan")] show_scan: bool,
) -> Result<Markup, GetTidalConfigError> {
    let logged_in = moosicbox_tidal::db::get_tidal_config(db).await?.is_some();

    Ok(if logged_in {
        settings_logged_in(
            #[cfg(feature = "scan")]
            show_scan,
        )
    } else {
        settings_logged_out()
    })
}
