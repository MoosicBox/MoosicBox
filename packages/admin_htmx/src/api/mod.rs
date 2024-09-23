use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup, PreEscaped, DOCTYPE};
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
        qobuz::bind_services(profiles::bind_services(scope.service(index_endpoint))),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexQuery {
    show_scan: Option<bool>,
}

#[route("", method = "GET")]
pub async fn index_endpoint(
    htmx: Htmx,
    query: web::Query<IndexQuery>,
    profile: Option<ProfileName>,
    config_db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    let profiles = PROFILES.names();
    let profile = profile
        .as_ref()
        .map(|x| &x.0)
        .or_else(|| profiles.first())
        .cloned();

    let body = html! {
        body
            hx-ext="path-vars"
            hx-get="/admin?moosicboxProfile={event.profile}"
            hx-headers={"{\"moosicbox-profile\": \""(profile.as_deref().unwrap_or_default())"\"}"}
            hx-swap="outerHTML"
            hx-push-url="true"
            hx-trigger="select-moosicbox-profile from:body"
        {
            h1 { "MoosicBox Admin" }
            hr;
            (
                profiles::select(
                    &profiles.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
                    profile.as_deref(),
                )
            )
            ({
                if let Some(profile) = profile {
                    let library_db = PROFILES.get(&profile)
                        .ok_or_else(|| ErrorInternalServerError("Missing profile"))?;

                    profile_info(&config_db, &library_db, query.show_scan.unwrap_or_default()).await?
                } else {
                    html! {}
                }
            })
        }
    };

    Ok(if htmx.is_htmx {
        body
    } else {
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
                    script {
                        (PreEscaped(r#"
                            htmx.defineExtension('path-vars', {
                                onEvent: function (name, evt) {
                                    if (name === "htmx:configRequest") {
                                        let sourceEventData = evt.detail ? (evt.detail.triggeringEvent ? evt.detail.triggeringEvent.detail : null) : null;
                                        if (sourceEventData) {
                                            evt.detail.path = evt.detail.path.replace(/{event\.(\w+)}/g, function (_, k) {
                                                return sourceEventData[k];
                                            });
                                        }
                                    }
                                }
                            });
                        "#))
                    }
                }
                (body)
            }
        }
    })
}

async fn profile_info(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
    show_scan: bool,
) -> Result<Markup, actix_web::Error> {
    Ok(html! {
        h2 { "Server Info" }
        (info::info(config_db).await.map_err(ErrorInternalServerError)?)
        hr;
        h2 { "Profiles" }
        div hx-get="/admin/profiles" hx-trigger="create-moosicbox-profile-success from:body" {
            (profiles::profiles(config_db).await.map_err(ErrorInternalServerError)?)
        }
        (profiles::new_profile_form(None, None, false))
        hr;
        h2 { "Scan" }
        (scan::scan(library_db).await.map_err(ErrorInternalServerError)?)
        (if cfg!(feature = "tidal") { html! {
            hr;
            h2 { "Tidal" }
            (tidal::settings(library_db, show_scan).await.map_err(ErrorInternalServerError)?)
        } } else { html!{} })
        (if cfg!(feature = "qobuz") { html! {
            hr;
            h2 { "Qobuz" }
            (qobuz::settings(library_db, show_scan).await.map_err(ErrorInternalServerError)?)
        } } else { html!{} })
    })
}
