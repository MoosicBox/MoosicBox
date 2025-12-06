//! Actix Web server implementation.

use std::{
    future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, RwLock},
};

use actix_http::{Request, Response};
use actix_service::{IntoServiceFactory, ServiceFactory};
use actix_web::{
    Error, HttpServer, Resource,
    body::MessageBody,
    dev::{AppConfig, ServerHandle},
};
use moosicbox_web_server::{HttpResponse, HttpResponseBody, Method, Scope, request::HttpRequest};
use moosicbox_web_server_core::WebServer;

#[cfg(feature = "cors")]
use moosicbox_web_server_cors::AllOrSome;

use crate::request::ActixRequest;

/// Actix Web server wrapper.
///
/// This struct wraps an Actix `HttpServer` and provides an implementation of
/// the [`WebServer`] trait for integration with the `moosicbox_web_server` framework.
pub struct ActixWebServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig>,
    S::Error: Into<Error>,
    S::InitError: std::fmt::Debug,
    S::Response: Into<Response<B>>,
    B: MessageBody,
{
    factory: F,
    _service: PhantomData<S>,
    _body: PhantomData<B>,
    handle: Arc<RwLock<Option<ServerHandle>>>,
    addr: String,
}

impl<F, I, S, B> WebServer for ActixWebServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig> + 'static,
    S::Error: Into<Error>,
    S::InitError: std::fmt::Debug,
    S::Response: Into<Response<B>>,
    B: MessageBody + 'static,
{
    fn start(&self) -> Pin<Box<dyn std::future::Future<Output = ()>>> {
        log::debug!("Starting actix server on '{}'", self.addr);
        let server = HttpServer::new(self.factory.clone());
        let server = server.bind(&self.addr).unwrap();
        let server = server.run();
        *self.handle.write().unwrap() = Some(server.handle());
        Box::pin(async move {
            if let Err(e) = server.await {
                log::error!("Error running actix server: {e:?}");
            }
            log::debug!("Actix server stopped");
        })
    }

    fn stop(&self) -> Pin<Box<dyn std::future::Future<Output = ()>>> {
        log::debug!("Stopping actix server");
        let handle = self.handle.write().unwrap().take();
        if let Some(handle) = handle {
            return Box::pin(handle.stop(true));
        }
        Box::pin(future::ready(()))
    }
}

/// Extension trait for building Actix web servers from `WebServerBuilder`.
pub trait WebServerBuilderActixExt {
    /// Builds an Actix web server from the builder configuration.
    ///
    /// # Returns
    ///
    /// Returns a boxed [`WebServer`] trait object that can be started and stopped.
    #[must_use]
    fn build_actix(self) -> Box<dyn WebServer>;

    /// Builds an Actix web server with static file serving.
    ///
    /// # Arguments
    ///
    /// * `static_files` - List of static file configurations to serve
    ///
    /// # Returns
    ///
    /// Returns a boxed [`WebServer`] trait object that can be started and stopped.
    #[cfg(feature = "static-files")]
    #[must_use]
    fn build_actix_with_static(
        self,
        static_files: Vec<moosicbox_web_server::StaticFiles>,
    ) -> Box<dyn WebServer>;
}

impl WebServerBuilderActixExt for moosicbox_web_server::WebServerBuilder {
    #[allow(clippy::too_many_lines)]
    fn build_actix(self) -> Box<dyn WebServer> {
        build_actix_server(&self, &[])
    }

    #[cfg(feature = "static-files")]
    fn build_actix_with_static(
        self,
        static_files: Vec<moosicbox_web_server::StaticFiles>,
    ) -> Box<dyn WebServer> {
        build_actix_server(&self, &static_files)
    }
}

/// Internal function to build an Actix web server.
#[allow(clippy::too_many_lines)]
fn build_actix_server(
    builder: &moosicbox_web_server::WebServerBuilder,
    #[allow(unused_variables)] additional_static_files: &[moosicbox_web_server::StaticFiles],
) -> Box<dyn WebServer> {
    let addr = builder.addr().to_string();
    let port = builder.port();
    let scopes = builder.scopes().to_vec();

    // Combine static files from builder with additional ones
    #[cfg(feature = "static-files")]
    let all_static_files: Vec<_> = builder
        .static_files()
        .cloned()
        .into_iter()
        .chain(additional_static_files.iter().cloned())
        .collect();

    #[cfg(not(feature = "static-files"))]
    let _ = additional_static_files; // Suppress unused warning

    #[cfg(feature = "cors")]
    let cors_config = builder.cors().clone();

    let factory = move || {
        #[cfg(feature = "cors")]
        let cors = build_cors(&cors_config);

        #[allow(unused_mut)]
        let mut app = {
            let app = actix_web::App::new();

            #[cfg(feature = "htmx")]
            let app = app.wrap(actix_htmx::HtmxMiddleware {});

            #[cfg(feature = "cors")]
            let app = app.wrap(cors);

            app
        };

        // Register scopes and routes
        for scope in &scopes {
            app = register_scope(app, scope);
        }

        // Register static files
        #[cfg(feature = "static-files")]
        for config in &all_static_files {
            app = crate::static_files::register_static_files(app, config);
        }

        app
    };

    Box::new(ActixWebServer {
        factory,
        _service: PhantomData,
        _body: PhantomData,
        handle: Arc::new(RwLock::new(None)),
        addr: format!("{addr}:{port}"),
    })
}

/// Register a scope and its routes with the Actix app.
fn register_scope<T>(app: actix_web::App<T>, scope: &Scope) -> actix_web::App<T>
where
    T: actix_service::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Error = actix_web::Error,
            InitError = (),
        >,
{
    let mut actix_scope = actix_web::web::scope(scope.path());

    for route in scope.routes() {
        let path = route.path().to_string();
        let handler = route.handler().clone();
        let method = route.method();

        let actix_handler = move |req: actix_web::HttpRequest| {
            let handler = handler.clone();
            async move {
                // Convert actix request to our HttpRequest
                let http_request = HttpRequest::new(ActixRequest::from(&req));

                // Call the handler
                let result = handler(http_request).await;

                // Convert the result to actix response, mapping both success and error
                result
                    .map(convert_response)
                    .map_err(crate::error::into_actix_error)
            }
        };

        let resource = Resource::new(&path);
        let resource = match method {
            Method::Get => resource.route(actix_web::web::get().to(actix_handler)),
            Method::Post => resource.route(actix_web::web::post().to(actix_handler)),
            Method::Put => resource.route(actix_web::web::put().to(actix_handler)),
            Method::Delete => resource.route(actix_web::web::delete().to(actix_handler)),
            Method::Patch => resource.route(actix_web::web::patch().to(actix_handler)),
            Method::Head => resource.route(actix_web::web::head().to(actix_handler)),
            Method::Options => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::OPTIONS)
                    .to(actix_handler),
            ),
            Method::Trace => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::TRACE)
                    .to(actix_handler),
            ),
            Method::Connect => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::CONNECT)
                    .to(actix_handler),
            ),
        };

        actix_scope = actix_scope.service(resource);
    }

    // Recursively register nested scopes
    for nested_scope in scope.scopes() {
        actix_scope = register_nested_scope(actix_scope, nested_scope);
    }

    app.service(actix_scope)
}

/// Register a nested scope with the Actix scope.
fn register_nested_scope(parent_scope: actix_web::Scope, scope: &Scope) -> actix_web::Scope {
    let mut actix_scope = actix_web::web::scope(scope.path());

    for route in scope.routes() {
        let path = route.path().to_string();
        let handler = route.handler().clone();
        let method = route.method();

        let actix_handler = move |req: actix_web::HttpRequest| {
            let handler = handler.clone();
            async move {
                let http_request = HttpRequest::new(ActixRequest::from(&req));
                let result = handler(http_request).await;
                result
                    .map(convert_response)
                    .map_err(crate::error::into_actix_error)
            }
        };

        let resource = Resource::new(&path);
        let resource = match method {
            Method::Get => resource.route(actix_web::web::get().to(actix_handler)),
            Method::Post => resource.route(actix_web::web::post().to(actix_handler)),
            Method::Put => resource.route(actix_web::web::put().to(actix_handler)),
            Method::Delete => resource.route(actix_web::web::delete().to(actix_handler)),
            Method::Patch => resource.route(actix_web::web::patch().to(actix_handler)),
            Method::Head => resource.route(actix_web::web::head().to(actix_handler)),
            Method::Options => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::OPTIONS)
                    .to(actix_handler),
            ),
            Method::Trace => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::TRACE)
                    .to(actix_handler),
            ),
            Method::Connect => resource.route(
                actix_web::web::route()
                    .method(actix_web::http::Method::CONNECT)
                    .to(actix_handler),
            ),
        };

        actix_scope = actix_scope.service(resource);
    }

    // Recursively register nested scopes
    for nested_scope in scope.scopes() {
        actix_scope = register_nested_scope(actix_scope, nested_scope);
    }

    parent_scope.service(actix_scope)
}

/// Convert an `HttpResponse` to an Actix response.
fn convert_response(resp: HttpResponse) -> actix_web::HttpResponse {
    let mut actix_resp = actix_web::HttpResponseBuilder::new(resp.status_code.into());

    // Insert all headers
    for (name, value) in &resp.headers {
        actix_resp.insert_header((name.clone(), value.clone()));
    }

    // Keep backwards compatibility with location field
    if let Some(ref location) = resp.location {
        actix_resp.insert_header((actix_http::header::LOCATION, location.clone()));
    }

    match resp.body {
        Some(HttpResponseBody::Bytes(bytes)) => actix_resp.body(bytes),
        None => actix_resp.finish(),
    }
}

/// Build CORS configuration for Actix.
#[cfg(feature = "cors")]
fn build_cors(config: &moosicbox_web_server_cors::Cors) -> actix_cors::Cors {
    let cors = actix_cors::Cors::default().max_age(config.max_age().map(|x| x as usize));

    let cors = match config.allowed_origins() {
        AllOrSome::All => cors.allow_any_origin(),
        AllOrSome::Some(origins) => {
            let mut cors = cors;
            for origin in origins {
                cors = cors.allowed_origin(origin);
            }
            cors
        }
    };

    let cors = match config.allowed_methods() {
        AllOrSome::All => cors.allow_any_method(),
        AllOrSome::Some(methods) => cors.allowed_methods(methods.iter().map(AsRef::as_ref)),
    };

    let cors = match config.allowed_headers() {
        AllOrSome::All => cors.allow_any_header(),
        AllOrSome::Some(headers) => cors.allowed_headers(headers),
    };

    let cors = match config.expose_headers() {
        AllOrSome::All => cors.expose_any_header(),
        AllOrSome::Some(headers) => cors.expose_headers(headers),
    };

    let mut cors = cors;

    if config.supports_credentials() {
        cors = cors.supports_credentials();
    }

    cors
}
