use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, HttpRequest, HttpResponse, Responder, Scope,
};
use maud::{html, Markup, DOCTYPE};
use moosicbox_database::{
    config::ConfigDatabase,
    profiles::{LibraryDatabase, PROFILES},
};

mod info;
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
        qobuz::bind_services(scope.service(index_endpoint)),
    )))
}

#[route("", method = "GET")]
pub async fn index_endpoint(
    _htmx: Htmx,
    req: HttpRequest,
    config_db: ConfigDatabase,
) -> Result<impl Responder, actix_web::Error> {
    let mut response = HttpResponse::Ok();
    response.content_type("text/html");

    let profiles = PROFILES.names();

    let profile = req
        .headers()
        .get("moosicbox-profile")
        .and_then(|x| x.to_str().ok())
        .or_else(|| profiles.first().map(|x| x.as_str()));

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
                body hx-headers={"{'moosicbox-profile': '"(profile.unwrap_or_default())"'}"} {
                    h1 { "MoosicBox Admin" }
                    hr {}
                    select {
                        @for profile in profiles.iter() {
                            option { (profile) }
                        }
                    }
                    ({
                        if let Some(profile) = profile {
                            let library_db = PROFILES.get(profile)
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
