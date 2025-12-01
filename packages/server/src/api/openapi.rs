//! `OpenAPI` specification generation and documentation UI services.
//!
//! This module provides functions to initialize and serve `OpenAPI` documentation for the
//! `MoosicBox` server API. It aggregates documentation from all enabled API modules and
//! provides multiple UI interfaces (Swagger UI, Redoc, `RapiDoc`, Scalar) for browsing
//! the API documentation.

#![allow(clippy::needless_for_each)]

use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
};
use utoipa::{OpenApi as _, openapi::OpenApi};

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi()]
struct ApiDoc;

/// Initializes and returns the combined `OpenAPI` specification for all enabled API modules.
///
/// This function aggregates `OpenAPI` documentation from all feature-enabled API modules
/// (audio output, auth, config, player, etc.) into a single unified specification.
///
/// # Returns
///
/// A complete `OpenAPI` specification document that can be served via Swagger UI, Redoc,
/// `RapiDoc`, or Scalar interfaces.
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn init() -> OpenApi {
    #[allow(unused)]
    fn nest_api(api: OpenApi, path: &str, mut nested: OpenApi) -> OpenApi {
        nested.paths.paths.iter_mut().for_each(|(path, item)| {
            item.options.iter_mut().for_each(|operation| {
                operation.operation_id = Some(path.to_owned());
            });
        });

        api.nest(path, nested)
    }

    #[allow(clippy::let_and_return)]
    let api = ApiDoc::openapi();

    #[cfg(feature = "audio-output-api")]
    let api = nest_api(
        api,
        "/audio-output",
        moosicbox_audio_output::api::Api::openapi(),
    );
    #[cfg(feature = "audio-zone-api")]
    let api = nest_api(
        api,
        "/audio-zone",
        moosicbox_audio_zone::api::Api::openapi(),
    );
    #[cfg(feature = "auth-api")]
    let api = nest_api(api, "/auth", moosicbox_auth::api::Api::openapi());
    #[cfg(feature = "downloader-api")]
    let api = nest_api(
        api,
        "/downloader",
        moosicbox_downloader::api::Api::openapi(),
    );
    #[cfg(feature = "config-api")]
    let api = nest_api(api, "/config", moosicbox_config::api::Api::openapi());
    #[cfg(feature = "files-api")]
    let api = nest_api(api, "/files", moosicbox_files::api::Api::openapi());
    #[cfg(feature = "library-api")]
    let api = nest_api(api, "/library", moosicbox_library::api::Api::openapi());
    #[cfg(feature = "menu-api")]
    let api = nest_api(api, "/menu", moosicbox_menu::api::Api::openapi());
    #[cfg(feature = "music-api-api")]
    let api = nest_api(
        api,
        "/music-api",
        moosicbox_music_api_api::api::Api::openapi(),
    );
    #[cfg(feature = "player-api")]
    let api = nest_api(api, "/player", moosicbox_player::api::Api::openapi());
    #[cfg(all(feature = "qobuz", feature = "qobuz-api"))]
    let api = nest_api(api, "/qobuz", moosicbox_qobuz::api::Api::openapi());
    #[cfg(feature = "scan-api")]
    let api = nest_api(api, "/scan", moosicbox_scan::api::Api::openapi());
    #[cfg(feature = "session-api")]
    let api = nest_api(api, "/session", moosicbox_session::api::Api::openapi());
    #[cfg(all(feature = "tidal", feature = "tidal-api"))]
    let api = nest_api(api, "/tidal", moosicbox_tidal::api::Api::openapi());
    #[cfg(feature = "upnp-api")]
    let api = nest_api(api, "/upnp", switchy_upnp::api::Api::openapi());
    #[cfg(all(feature = "yt", feature = "yt-api"))]
    let api = nest_api(api, "/yt", moosicbox_yt::api::Api::openapi());

    api
}

/// Binds `OpenAPI` documentation services to an Actix-web scope.
///
/// This function registers multiple `OpenAPI` UI services (Swagger UI, Redoc, `RapiDoc`, Scalar)
/// that all serve the same `OpenAPI` specification document.
///
/// # Returns
///
/// The modified scope with all documentation UI services registered:
/// * `/swagger-ui/` - Interactive Swagger UI documentation
/// * `/redoc` - Redoc documentation viewer
/// * `/rapidoc` - `RapiDoc` documentation viewer
/// * `/scalar` - Scalar documentation viewer
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
    openapi: &OpenApi,
) -> Scope<T> {
    use utoipa_redoc::Servable as _;
    use utoipa_scalar::Servable as _;

    scope
        .service(utoipa_redoc::Redoc::with_url("redoc", openapi.clone()))
        .service(
            utoipa_swagger_ui::SwaggerUi::new("swagger-ui/{_:.*}")
                .url("api-docs/openapi.json", openapi.clone()),
        )
        // There is no need to create RapiDoc::with_openapi because the OpenApi is served
        // via SwaggerUi. Instead we only make rapidoc to point to the existing doc.
        //
        // If we wanted to serve the schema, the following would work:
        // .service(RapiDoc::with_openapi("api-docs/openapi2.json", openapi.clone()).path("rapidoc"))
        .service(utoipa_rapidoc::RapiDoc::new("api-docs/openapi.json").path("rapidoc"))
        .service(utoipa_scalar::Scalar::with_url("scalar", openapi.clone()))
}
