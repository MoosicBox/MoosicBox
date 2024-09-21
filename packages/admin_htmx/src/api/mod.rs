use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, HttpResponse, Responder, Scope,
};
use maud::{html, Markup, DOCTYPE};
use moosicbox_database::{
    config::ConfigDatabase,
    profiles::{api::ProfileName, LibraryDatabase, PROFILES},
};
use serde::Deserialize;

mod info;
mod profiles;
#[cfg(feature = "qobuz")]
mod qobuz;
mod scan;
#[cfg(feature = "tidal")]
mod tidal;
pub(crate) mod util;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    info::bind_services(scan::bind_services(tidal::bind_services(
        qobuz::bind_services(profiles::bind_services(
            scope
                .service(index_endpoint)
                .service(select_profile_endpoint),
        )),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProfileForm {
    profile: String,
}

#[route("select-profile", method = "POST")]
pub async fn select_profile_endpoint(
    htmx: Htmx,
    form: web::Form<SelectProfileForm>,
) -> Result<Markup, actix_web::Error> {
    htmx.redirect(format!("/admin?moosicboxProfile={}", form.profile));

    Ok(html! {})
}

#[route("", method = "GET")]
pub async fn index_endpoint(
    _htmx: Htmx,
    profile: Option<ProfileName>,
    config_db: ConfigDatabase,
) -> Result<impl Responder, actix_web::Error> {
    let mut response = HttpResponse::Ok();
    response.content_type("text/html");

    let profiles = PROFILES.names();
    let profile = profile.map(|x| x.0).or_else(|| profiles.first().cloned());

    Ok(response.body(
        html! {
            (DOCTYPE)
            html {
                head {
                    title { "MoosicBox Admin" }
                    script
                        src="https://unpkg.com/htmx.org@2.0.2"
                        integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ"
                        crossorigin="anonymous"
                        {}
                }
                body hx-headers={"{'moosicbox-profile': '"(profile.as_deref().unwrap_or_default())"'}"} {
                    h1 { "MoosicBox Admin" }
                    hr {}
                    form hx-post="/admin/select-profile" hx-trigger="change" {
                        select name="profile" {
                            @for p in profiles.iter() {
                                option value=(p) selected[profile.as_ref().is_some_and(|x| x == p)] { (p) }
                            }
                        }
                    }
                    ({
                        if let Some(profile) = profile {
                            let library_db = PROFILES.get(&profile)
                                .ok_or_else(|| ErrorInternalServerError("Missing profile"))?;

                            profile_info(&config_db, &library_db).await?
                        } else {
                            html! {}
                        }
                    })
                }
            }
        }
        .into_string(),
    ))
}

async fn profile_info(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    Ok(html! {
        h2 { "Server Info" }
        (info::info(config_db).await.map_err(ErrorInternalServerError)?)
        hr {}
        h2 { "Profiles" }
        div hx-get="/admin/profiles" hx-trigger="create-moosicbox-profile-success from:body" {
            (profiles::profiles(config_db).await.map_err(ErrorInternalServerError)?)
        }
        (profiles::new_profile_form(None, None, false))
        hr {}
        h2 { "Scan" }
        (scan::scan(library_db).await.map_err(ErrorInternalServerError)?)
        (if cfg!(feature = "tidal") { html! {
            hr {}
            h2 { "Tidal" }
            (tidal::settings(library_db).await.map_err(ErrorInternalServerError)?)
        } } else { html!{} })
        (if cfg!(feature = "qobuz") { html! {
            hr {}
            h2 { "Qobuz" }
            (qobuz::settings(library_db).await.map_err(ErrorInternalServerError)?)
        } } else { html!{} })
    })
}
