use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::Database;
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
    _htmx: Htmx,
    form: web::Form<UserLoginForm>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Markup, actix_web::Error> {
    let response = moosicbox_qobuz::user_login(
        data.database.clone(),
        &form.username,
        &form.password,
        None,
        Some(true),
    )
    .await;

    if response.is_ok_and(|x| x.get("accessToken").is_some()) {
        Ok(settings_logged_in())
    } else {
        Ok(settings_logged_out(Some(
            html! { p { "Invalid username/password" } },
        )))
    }
}

#[route("settings", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_settings_endpoint(
    _htmx: Htmx,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Markup, actix_web::Error> {
    settings(&**data.database)
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
            input type="text" name="username" placeholder="username..." {}
            input type="password" name="password" placeholder="password..." {}
            button type="submit" { "Login" }
        }
    }
}

pub async fn settings(db: &dyn Database) -> Result<Markup, DbError> {
    let logged_in = moosicbox_qobuz::db::get_qobuz_config(db).await?.is_some();

    Ok(if logged_in {
        settings_logged_in()
    } else {
        settings_logged_out(None)
    })
}
