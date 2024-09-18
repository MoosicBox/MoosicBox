use actix_htmx::{Htmx, TriggerType};
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::profiles::LibraryDatabase;
use serde::Deserialize;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(
        web::scope("/qobuz")
            .service(get_settings_endpoint)
            .service(user_login_endpoint),
    )
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

        settings_logged_in()
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

#[route("settings", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_settings_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    settings(&db)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get Qobuz settings: {e:?}")))
}

pub fn settings_logged_in() -> Markup {
    html! {
        p { "Logged in!" }
    }
}

pub fn settings_logged_out(message: Option<Markup>) -> Markup {
    html! {
        form hx-post="/admin/qobuz/auth/user-login" hx-swap="outerHTML" {
            (message.unwrap_or_default())
            input type="text" name="username" placeholder="username..." autocomplete="username" {}
            input type="password" name="password" placeholder="password..." autocomplete="current-password" {}
            button type="submit" { "Login" }
        }
    }
}

pub async fn settings(db: &LibraryDatabase) -> Result<Markup, DbError> {
    let logged_in = moosicbox_qobuz::db::get_qobuz_config(db).await?.is_some();

    Ok(if logged_in {
        settings_logged_in()
    } else {
        settings_logged_out(None)
    })
}
