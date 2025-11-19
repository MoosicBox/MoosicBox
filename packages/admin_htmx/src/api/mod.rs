//! Actix-web endpoints for the `MoosicBox` admin interface.
//!
//! This module provides HTMX-based web endpoints for managing `MoosicBox` configuration
//! including server info, profiles, and music service integrations.
//!
//! # Main Entry Point
//!
//! Use [`bind_services`] to register all admin endpoints on an Actix-web scope.

#![allow(clippy::future_not_send)]

use actix_htmx::Htmx;
use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web,
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use moosicbox_profiles::api::ProfileName;
use serde::Deserialize;
use switchy_database::{
    config::ConfigDatabase,
    profiles::{LibraryDatabase, PROFILES},
};

pub mod info;
pub mod profiles;
#[cfg(feature = "qobuz")]
pub mod qobuz;
#[cfg(feature = "scan")]
pub mod scan;
#[cfg(feature = "tidal")]
pub mod tidal;
pub mod util;

/// Binds all admin HTMX endpoints to the provided Actix web scope.
///
/// This is the main entry point for integrating the admin UI into an Actix web application.
/// It registers all admin-related endpoints including server info, profiles management,
/// and optionally scan, Tidal, and Qobuz settings endpoints (depending on enabled features).
#[allow(clippy::let_and_return)]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    let scope = scope.service(index_endpoint);
    let scope = info::bind_services(scope);
    let scope = profiles::bind_services(scope);
    #[cfg(feature = "scan")]
    let scope = scan::bind_services(scope);
    #[cfg(feature = "tidal")]
    let scope = tidal::bind_services(scope);
    #[cfg(feature = "qobuz")]
    let scope = qobuz::bind_services(scope);

    scope
}

/// Query parameters for the admin index page.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexQuery {
    /// Whether to show scan-related UI elements.
    #[cfg(feature = "scan")]
    show_scan: Option<bool>,
}

/// Admin index page endpoint that renders the main admin interface.
///
/// # Errors
///
/// * If the requested profile does not exist in the database
/// * If any of the profile information sections fail to load
#[route("", method = "GET")]
pub async fn index_endpoint(
    htmx: Htmx,
    #[allow(unused)] query: web::Query<IndexQuery>,
    profile: Option<ProfileName>,
    config_db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    let profiles = PROFILES.names();
    let profile = profile
        .as_ref()
        .map(|x| &x.0)
        .or_else(|| profiles.first())
        .cloned();

    if htmx.is_htmx
        && let Some(profile) = &profile
    {
        htmx.push_url(format!("/admin?moosicboxProfile={profile}"));
    }

    let main = html! {
        main
            hx-ext="path-vars"
            hx-get="/admin?moosicboxProfile={event.profile}"
            hx-headers={"{\"moosicbox-profile\": \""(profile.as_deref().unwrap_or_default())"\"}"}
            hx-swap="outerHTML"
            hx-trigger="select-moosicbox-profile from:body, delete-current-moosicbox-profile-success from:body"
        {
            h1 { "MoosicBox Admin" }
            hr;
            (profiles::select_form(
                &profiles.iter().map(String::as_str).collect::<Vec<_>>(),
                profile.as_deref(),
                Some("delete-moosicbox-profile-success from:body, create-moosicbox-profile-success from:body")
            ))
            ({
                if let Some(profile) = profile {
                    let library_db = PROFILES.get(&profile)
                        .ok_or_else(|| ErrorInternalServerError("Missing profile"))?;

                    profile_info(&config_db, &library_db, #[cfg(feature = "scan")] query.show_scan.unwrap_or_default()).await?
                } else {
                    html! {}
                }
            })
        }
    };

    Ok(if htmx.is_htmx {
        main
    } else {
        html! {
            (DOCTYPE)
            html style="height:100%" lang="en-US" {
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
                body style="height:100%;overflow:auto;" {
                    (main)
                }
            }
        }
    })
}

/// Renders the complete profile information page including server info, profiles, and service integrations.
///
/// # Errors
///
/// * If any parts of the page fail to load
pub async fn profile_info(
    #[allow(unused)] config_db: &ConfigDatabase,
    #[allow(unused)] library_db: &LibraryDatabase,
    #[allow(unused)]
    #[cfg(feature = "scan")]
    show_scan: bool,
) -> Result<Markup, actix_web::Error> {
    #[cfg(feature = "scan")]
    let scan = html! {
        hr;
        h2 { "Scan" }
        (scan::scan(library_db).await.map_err(ErrorInternalServerError)?)
    };
    #[cfg(not(feature = "scan"))]
    let scan = html! {};

    #[cfg(feature = "qobuz")]
    let qobuz = html! {
        hr;
        h2 { "Qobuz" }
        (qobuz::settings(library_db, #[cfg(feature = "scan")] show_scan).await.map_err(ErrorInternalServerError)?)
    };
    #[cfg(not(feature = "qobuz"))]
    let qobuz = html! {};

    #[cfg(feature = "tidal")]
    let tidal = html! {
        hr;
        h2 { "Tidal" }
        (tidal::settings(library_db, #[cfg(feature = "scan")] show_scan).await.map_err(ErrorInternalServerError)?)
    };
    #[cfg(not(feature = "tidal"))]
    let tidal = html! {};

    Ok(html! {
        h2 { "Server Info" }
        (info::info(config_db).await.map_err(ErrorInternalServerError)?)
        hr;
        h2 { "Profiles" }
        div
            hx-get="/admin/profiles"
            hx-trigger="create-moosicbox-profile-success from:body"
            hx-swap="innerHTML"
        {
            (profiles::profiles(config_db).await.map_err(ErrorInternalServerError)?)
        }
        (profiles::new_profile_form(None, None, false))
        (scan)
        (tidal)
        (qobuz)
    })
}
