//! Actix backend integration for the web server.
//!
//! This module provides Actix-web integration, including:
//! - `ActixRequest` - Implementation of `HttpRequestTrait` for Actix
//! - `build_actix()` - Builder method for creating Actix-based servers
//! - Error conversions between the crate's error types and Actix errors

use std::{
    any::TypeId,
    collections::BTreeMap,
    future::{self},
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
    error::{self},
};
use bytes::Bytes;

use crate::{
    Method, PathParams,
    request::{ErasedState, HttpRequestTrait},
};
use switchy_http_models::{StatusCode, TryFromU16StatusCodeError};
use switchy_web_server_core::WebServer;
#[cfg(feature = "cors")]
use switchy_web_server_cors::AllOrSome;

/// Actix-specific HTTP request wrapper that implements `HttpRequestTrait`.
///
/// This struct extracts and stores data from an `actix_web::HttpRequest` in a
/// `Send + Sync` compatible way. The original Actix request cannot be stored
/// directly because it uses `Rc` internally.
#[derive(Clone, Debug)]
pub struct ActixRequest {
    /// Request path
    path: String,
    /// Query string without leading ?
    query_string: String,
    /// HTTP method
    method: Method,
    /// Request headers
    headers: BTreeMap<String, String>,
    /// Cookies
    cookies: BTreeMap<String, String>,
    /// Remote address
    remote_addr: Option<String>,
    /// Path parameters from route matching
    path_params: PathParams,
}

impl ActixRequest {
    /// Creates a new `ActixRequest` by extracting data from an Actix `HttpRequest`.
    #[must_use]
    pub fn new(inner: &actix_web::HttpRequest) -> Self {
        use actix_web::http::Method as ActixMethod;

        let method = match *inner.method() {
            ActixMethod::GET => Method::Get,
            ActixMethod::POST => Method::Post,
            ActixMethod::PUT => Method::Put,
            ActixMethod::PATCH => Method::Patch,
            ActixMethod::DELETE => Method::Delete,
            ActixMethod::HEAD => Method::Head,
            ActixMethod::OPTIONS => Method::Options,
            ActixMethod::CONNECT => Method::Connect,
            _ => Method::Trace,
        };

        let headers = inner
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|val| (k.as_str().to_string(), val.to_string()))
            })
            .collect();

        let cookies = inner
            .cookies()
            .map(|jar| {
                jar.iter()
                    .map(|c| (c.name().to_string(), c.value().to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let remote_addr = inner
            .connection_info()
            .peer_addr()
            .map(std::string::ToString::to_string);

        Self {
            path: inner.path().to_string(),
            query_string: inner.query_string().to_string(),
            method,
            headers,
            cookies,
            remote_addr,
            path_params: BTreeMap::new(),
        }
    }

    /// Creates a new `ActixRequest` with custom path parameters.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_path_params(mut self, path_params: PathParams) -> Self {
        self.path_params = path_params;
        self
    }
}

impl HttpRequestTrait for ActixRequest {
    fn path(&self) -> &str {
        &self.path
    }

    fn query_string(&self) -> &str {
        &self.query_string
    }

    fn method(&self) -> switchy_http_models::Method {
        self.method
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    fn headers(&self) -> BTreeMap<String, String> {
        self.headers.clone()
    }

    fn body(&self) -> Option<&Bytes> {
        // Actix body is consumed during extraction, not available here
        None
    }

    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies.get(name).cloned()
    }

    fn cookies(&self) -> BTreeMap<String, String> {
        self.cookies.clone()
    }

    fn remote_addr(&self) -> Option<String> {
        self.remote_addr.clone()
    }

    fn path_params(&self) -> &PathParams {
        &self.path_params
    }

    fn app_state_any(&self, type_id: TypeId) -> Option<ErasedState> {
        // Actix stores state differently - we need to use app_data
        // This is a simplified approach; for full support, the actix sub-crate
        // provides more comprehensive state handling
        let _ = type_id;
        None
    }
}

impl From<&actix_web::HttpRequest> for ActixRequest {
    fn from(inner: &actix_web::HttpRequest) -> Self {
        Self::new(inner)
    }
}

impl From<&actix_web::HttpRequest> for crate::HttpRequest {
    fn from(inner: &actix_web::HttpRequest) -> Self {
        Self::new(ActixRequest::new(inner))
    }
}

impl From<crate::Error> for Error {
    fn from(value: crate::Error) -> Self {
        match value {
            crate::Error::Http {
                status_code,
                source,
            } => match status_code {
                StatusCode::BadRequest => error::ErrorBadRequest(source),
                StatusCode::Unauthorized => error::ErrorUnauthorized(source),
                StatusCode::PaymentRequired => error::ErrorPaymentRequired(source),
                StatusCode::Forbidden => error::ErrorForbidden(source),
                StatusCode::NotFound => error::ErrorNotFound(source),
                StatusCode::MethodNotAllowed => error::ErrorMethodNotAllowed(source),
                StatusCode::NotAcceptable => error::ErrorNotAcceptable(source),
                StatusCode::ProxyAuthenticationRequired => {
                    error::ErrorProxyAuthenticationRequired(source)
                }
                StatusCode::RequestTimeout => error::ErrorRequestTimeout(source),
                StatusCode::Conflict => error::ErrorConflict(source),
                StatusCode::Gone => error::ErrorGone(source),
                StatusCode::LengthRequired => error::ErrorLengthRequired(source),
                StatusCode::PreconditionFailed => error::ErrorPreconditionFailed(source),
                StatusCode::ContentTooLarge => error::ErrorPayloadTooLarge(source),
                StatusCode::URITooLong => error::ErrorUriTooLong(source),
                StatusCode::UnsupportedMediaType => error::ErrorUnsupportedMediaType(source),
                StatusCode::RangeNotSatisfiable => error::ErrorRangeNotSatisfiable(source),
                StatusCode::ExpectationFailed => error::ErrorExpectationFailed(source),
                StatusCode::ImATeapot => error::ErrorImATeapot(source),
                StatusCode::MisdirectedRequest => error::ErrorMisdirectedRequest(source),
                StatusCode::UncompressableContent => error::ErrorUnprocessableEntity(source),
                StatusCode::Locked => error::ErrorLocked(source),
                StatusCode::FailedDependency => error::ErrorFailedDependency(source),
                StatusCode::UpgradeRequired => error::ErrorUpgradeRequired(source),
                StatusCode::PreconditionRequired => error::ErrorPreconditionRequired(source),
                StatusCode::TooManyRequests => error::ErrorTooManyRequests(source),
                StatusCode::RequestHeaderFieldsTooLarge => {
                    error::ErrorRequestHeaderFieldsTooLarge(source)
                }
                StatusCode::UnavailableForLegalReasons => {
                    error::ErrorUnavailableForLegalReasons(source)
                }
                StatusCode::Continue
                | StatusCode::SwitchingProtocols
                | StatusCode::Processing
                | StatusCode::EarlyHints
                | StatusCode::Ok
                | StatusCode::Created
                | StatusCode::Accepted
                | StatusCode::NonAuthoritativeInformation
                | StatusCode::NoContent
                | StatusCode::ResetContent
                | StatusCode::PartialContent
                | StatusCode::MultiStatus
                | StatusCode::AlreadyReported
                | StatusCode::IMUsed
                | StatusCode::MultipleChoices
                | StatusCode::MovedPermanently
                | StatusCode::Found
                | StatusCode::SeeOther
                | StatusCode::NotModified
                | StatusCode::UseProxy
                | StatusCode::TemporaryRedirect
                | StatusCode::PermanentRedirect
                | StatusCode::TooEarly
                | StatusCode::InternalServerError => error::ErrorInternalServerError(source),
                StatusCode::NotImplemented => error::ErrorNotImplemented(source),
                StatusCode::BadGateway => error::ErrorBadGateway(source),
                StatusCode::ServiceUnavailable => error::ErrorServiceUnavailable(source),
                StatusCode::GatewayTimeout => error::ErrorGatewayTimeout(source),
                StatusCode::HTTPVersionNotSupported => error::ErrorHttpVersionNotSupported(source),
                StatusCode::VariantAlsoNegotiates => error::ErrorVariantAlsoNegotiates(source),
                StatusCode::InsufficientStorage => error::ErrorInsufficientStorage(source),
                StatusCode::LoopDetected => error::ErrorLoopDetected(source),
                StatusCode::NotExtended => error::ErrorNotExtended(source),
                StatusCode::NetworkAuthenticationRequired => {
                    error::ErrorNetworkAuthenticationRequired(source)
                }
            },
        }
    }
}

impl TryFrom<Error> for crate::Error {
    type Error = TryFromU16StatusCodeError;

    /// # Errors
    ///
    /// Returns `TryFromU16StatusCodeError` if the Actix error's status code
    /// cannot be converted to a valid `StatusCode`.
    fn try_from(value: Error) -> Result<Self, Self::Error> {
        // Convert actix_web::Error to a Send + Sync error
        let status_code = StatusCode::try_from_u16(value.error_response().status().as_u16())?;
        let error_message = format!("Actix error: {value}");
        Ok(Self::from_http_status_code(
            status_code,
            std::io::Error::other(error_message),
        ))
    }
}

struct ActixWebServer<F, I, S, B>
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

use crate::{HttpRequest, WebServerBuilder};

impl WebServerBuilder {
    /// Build the web server using Actix backend
    ///
    /// This method constructs an Actix-based web server with all configured scopes,
    /// middleware, and settings. The server is ready to be started with `start()`.
    ///
    /// # Returns
    ///
    /// Returns a boxed `WebServer` trait object that can be used to start and stop the server.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn build_actix(self) -> Box<dyn WebServer> {
        #[cfg(feature = "cors")]
        let cors_builder = self.cors.clone();
        #[cfg(feature = "static-files")]
        let static_files = self.static_files.clone();
        let scopes = self.scopes.clone();
        let factory = move || {
            #[cfg(feature = "cors")]
            let cors = {
                let cors =
                    actix_cors::Cors::default().max_age(cors_builder.max_age.map(|x| x as usize));

                let cors = match &cors_builder.allowed_origins {
                    AllOrSome::All => cors.allow_any_origin(),
                    AllOrSome::Some(origins) => {
                        let mut cors = cors;
                        for origin in origins {
                            cors = cors.allowed_origin(origin);
                        }
                        cors
                    }
                };

                let cors = match &cors_builder.allowed_methods {
                    AllOrSome::All => cors.allow_any_method(),
                    AllOrSome::Some(methods) => {
                        cors.allowed_methods(methods.iter().map(AsRef::as_ref))
                    }
                };

                let cors = match &cors_builder.allowed_headers {
                    AllOrSome::All => cors.allow_any_header(),
                    AllOrSome::Some(headers) => cors.allowed_headers(headers),
                };

                let cors = match &cors_builder.expose_headers {
                    AllOrSome::All => cors.expose_any_header(),
                    AllOrSome::Some(headers) => cors.expose_headers(headers),
                };

                let mut cors = cors;

                if cors_builder.supports_credentials {
                    cors = cors.supports_credentials();
                }

                cors
            };

            #[allow(unused_mut)]
            let mut app = {
                let app = actix_web::App::new();

                #[cfg(feature = "htmx")]
                let app = app.wrap(actix_htmx::HtmxMiddleware {});

                #[cfg(feature = "cors")]
                let app = app.wrap(cors);

                app
            };

            // Register static file serving FIRST so scopes are checked first,
            // with static files as a fallback for unmatched routes.
            #[cfg(feature = "static-files")]
            #[allow(unused_mut)]
            let mut app = {
                if let Some(ref config) = static_files {
                    let mut files =
                        actix_files::Files::new(config.mount_path(), config.directory());

                    // Set index file if configured
                    if let Some(index) = config.effective_index_file() {
                        files = files.index_file(index);
                    }

                    files = files.prefer_utf8(true);

                    app.service(files)
                } else {
                    app
                }
            };

            for scope in &scopes {
                let mut actix_scope = actix_web::web::scope(&scope.path);
                for route in &scope.routes {
                    let path = route.path.clone();
                    let handler = route.handler.clone();
                    let method = route.method;

                    let actix_handler = move |req: actix_web::HttpRequest| {
                        let handler = handler.clone();
                        async move {
                            let result = handler(HttpRequest::from(&req)).await;
                            result.map(|resp| {
                                let mut actix_resp =
                                    actix_web::HttpResponseBuilder::new(resp.status_code.into());

                                // Insert all headers from the BTreeMap
                                for (name, value) in resp.headers {
                                    actix_resp.insert_header((name, value));
                                }

                                // Keep backwards compatibility with location field
                                if let Some(location) = resp.location {
                                    actix_resp
                                        .insert_header((actix_http::header::LOCATION, location));
                                }

                                match resp.body {
                                    Some(crate::HttpResponseBody::Bytes(bytes)) => {
                                        actix_resp.body(bytes)
                                    }
                                    None => actix_resp.finish(),
                                }
                            })
                        }
                    };

                    let resource = Resource::new(&path);
                    let resource = match method {
                        Method::Get => resource.route(actix_web::web::get().to(actix_handler)),
                        Method::Post => resource.route(actix_web::web::post().to(actix_handler)),
                        Method::Put => resource.route(actix_web::web::put().to(actix_handler)),
                        Method::Delete => {
                            resource.route(actix_web::web::delete().to(actix_handler))
                        }
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
                app = app.service(actix_scope);
            }

            app
        };

        Box::new(ActixWebServer {
            factory,
            _service: PhantomData,
            _body: PhantomData,
            handle: Arc::new(RwLock::new(None)),
            addr: format!("{}:{}", self.addr, self.port),
        })
    }
}
