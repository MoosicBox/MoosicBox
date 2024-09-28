use actix_htmx::{Htmx, TriggerType};
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::profiles::LibraryDatabase;
#[cfg(feature = "scan")]
use moosicbox_music_api::MusicApis;
#[cfg(feature = "scan")]
use moosicbox_scan::ScanOrigin;
use serde::Deserialize;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLoginForm {
    username: String,
    password: String,
}

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
                r#"{"level": "info", "message": "Successfully logged in to Qobuz", "success": true}"#
                    .to_string(),
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
                r#"{"level": "info", "message": "Failed to login to Qobuz", "success": false}"#
                    .to_string(),
            ),
            Some(TriggerType::Standard),
        );

        settings_logged_out(Some(html! { p { "Invalid username/password" } }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSettingsQuery {
    #[cfg(feature = "scan")]
    show_scan: Option<bool>,
}

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

#[cfg(feature = "scan")]
#[route("run-scan", method = "POST")]
pub async fn run_scan_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Markup, actix_web::Error> {
    moosicbox_scan::run_scan(Some(vec![ScanOrigin::Qobuz]), &db, music_apis)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to run scan: {e:?}")))?;

    Ok(html! {})
}

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

pub async fn settings(
    db: &LibraryDatabase,
    #[cfg(feature = "scan")] show_scan: bool,
) -> Result<Markup, DbError> {
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
