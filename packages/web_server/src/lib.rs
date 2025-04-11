#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::pin::Pin;

use bytes::Bytes;
use thiserror::Error;

pub use moosicbox_http_models::Method;
pub use moosicbox_web_server_core as core;
#[cfg(feature = "cors")]
pub use moosicbox_web_server_cors as cors;
pub use paste;
pub use serde_querystring as qs;

#[derive(Debug)]
pub struct WebServerBuilder {
    addr: String,
    port: u16,
    scopes: Vec<Scope>,
    #[cfg(feature = "cors")]
    cors: cors::Cors,
    #[cfg(feature = "compress")]
    compress: bool,
}

impl Default for WebServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WebServerBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            addr: "0.0.0.0".to_string(),
            port: 8080,
            scopes: vec![],
            #[cfg(feature = "cors")]
            cors: cors::Cors::default(),
            #[cfg(feature = "compress")]
            compress: false,
        }
    }

    #[must_use]
    pub fn with_scope<S: Into<Scope>>(mut self, scope: S) -> Self {
        self.scopes.push(scope.into());
        self
    }

    #[must_use]
    pub fn with_addr<T: Into<String>>(mut self, addr: T) -> Self {
        self.addr = addr.into();
        self
    }

    #[must_use]
    pub fn with_port<T: Into<u16>>(mut self, port: T) -> Self {
        self.port = port.into();
        self
    }
}

#[cfg(feature = "cors")]
impl WebServerBuilder {
    #[must_use]
    pub fn with_cors(mut self, cors: cors::Cors) -> Self {
        self.cors = cors;
        self
    }
}

#[cfg(feature = "compress")]
impl WebServerBuilder {
    #[must_use]
    pub const fn with_compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }
}

pub struct WebServerHandle {}

impl WebServerHandle {
    // pub async fn start(&self) {}
    // pub async fn stop(&self) {}
    // pub async fn restart(&self) {
    //     self.stop().await;
    //     self.start().await;
    // }
}

#[derive(Debug)]
pub enum HttpRequest {
    #[cfg(feature = "actix")]
    Actix(actix_web::HttpRequest),
    Stub(Stub),
}

impl HttpRequest {
    #[must_use]
    pub const fn as_ref(&self) -> HttpRequestRef<'_> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => HttpRequestRef::Actix(x),
            Self::Stub(x) => HttpRequestRef::Stub(x),
        }
    }
}

impl HttpRequest {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.headers().get(name).and_then(|x| x.to_str().ok()),
            Self::Stub(..) => unimplemented!("Stub can't access header with name={name}"),
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.path(),
            Self::Stub(..) => unimplemented!(),
        }
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.query_string(),
            Self::Stub(..) => unimplemented!(),
        }
    }

    /// # Errors
    ///
    /// * If the query string parsing fails
    pub fn parse_query<'a, T: serde::Deserialize<'a>>(&'a self) -> Result<T, qs::Error> {
        qs::from_str(self.query_string(), qs::ParseMode::UrlEncoded)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Stub;

#[derive(Debug, Clone, Copy)]
pub enum HttpRequestRef<'a> {
    #[cfg(feature = "actix")]
    Actix(&'a actix_web::HttpRequest),
    Stub(&'a Stub),
}

impl<'a> HttpRequestRef<'a> {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.headers().get(name).and_then(|x| x.to_str().ok()),
            Self::Stub(..) => unimplemented!("Stub can't access header with name={name}"),
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.path(),
            Self::Stub(..) => unimplemented!(),
        }
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.query_string(),
            Self::Stub(..) => unimplemented!(),
        }
    }

    /// # Errors
    ///
    /// * If the query string parsing fails
    pub fn parse_query<T: serde::Deserialize<'a>>(&'a self) -> Result<T, qs::Error> {
        qs::from_str(self.query_string(), qs::ParseMode::UrlEncoded)
    }
}

#[derive(Debug)]
pub enum HttpResponseBody {
    Bytes(Bytes),
}

impl HttpResponseBody {
    #[must_use]
    pub fn from_static(value: &'static str) -> Self {
        Self::Bytes(Bytes::from(value.as_bytes()))
    }
}

impl From<&str> for HttpResponseBody {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

#[cfg(feature = "serde")]
#[allow(clippy::fallible_impl_from)]
impl From<serde_json::Value> for HttpResponseBody {
    fn from(value: serde_json::Value) -> Self {
        (&value).into()
    }
}

#[cfg(feature = "serde")]
#[allow(clippy::fallible_impl_from)]
impl From<&serde_json::Value> for HttpResponseBody {
    fn from(value: &serde_json::Value) -> Self {
        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(&mut bytes, value).unwrap();
        Self::Bytes(Bytes::from(bytes))
    }
}

impl From<String> for HttpResponseBody {
    fn from(value: String) -> Self {
        Self::Bytes(Bytes::from(value.into_bytes()))
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub body: HttpResponseBody,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub path: &'static str,
    pub routes: Vec<Route>,
    pub scopes: Vec<Scope>,
}

impl Scope {
    #[must_use]
    pub const fn new(path: &'static str) -> Self {
        Self {
            path,
            routes: vec![],
            scopes: vec![],
        }
    }

    #[must_use]
    pub fn with_route(mut self, route: impl Into<Route>) -> Self {
        self.routes.push(route.into());
        self
    }

    #[must_use]
    pub fn with_routes<T: Into<Route>>(mut self, route: impl IntoIterator<Item = T>) -> Self {
        self.routes.extend(route.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<Self>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    #[must_use]
    pub fn with_scopes<T: Into<Self>>(mut self, scope: impl IntoIterator<Item = T>) -> Self {
        self.scopes.extend(scope.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, Error)]
pub enum WebServerError {
    #[error("400 Bad Request")]
    BadRequest,
    #[error("404 Not Found")]
    NotFound,
    #[error("500 Internal Server Error")]
    InternalServerError,
}

impl From<qs::Error> for WebServerError {
    fn from(_value: qs::Error) -> Self {
        Self::BadRequest
    }
}

pub trait FromRequest {
    type Error;
    type Future;

    fn from_request(req: HttpRequestRef) -> Self::Future;
}

#[macro_export]
macro_rules! route {
    ($method:ident, $name:ident, $path:expr, $func:expr $(,)?) => {
        $crate::paste::paste! {
            pub const [< $method:upper _ $name:upper >]: $crate::Route = $crate::Route {
                path: $path,
                method: $crate::Method::[< $method:camel >],
                handler: &$func,
            };
        }
    };
}

#[derive(Clone)]
pub struct Route {
    pub path: &'static str,
    pub method: Method,
    #[allow(clippy::type_complexity)]
    pub handler: &'static (
                 dyn Fn(
        HttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, WebServerError>>>>
                     + Send
                     + Sync
                     + 'static
             ),
}

impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "actix")]
mod actix {
    use std::{
        future::{self},
        marker::PhantomData,
        pin::Pin,
        sync::{Arc, RwLock},
    };

    use actix_http::{Request, Response, StatusCode};
    use actix_service::{IntoServiceFactory, Service, ServiceFactory, fn_factory};
    use actix_web::{
        Error, HttpServer, Resource,
        body::MessageBody,
        dev::{self, AppConfig, ServerHandle, ServiceRequest, ServiceResponse},
        error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    };
    use futures_util::{FutureExt, future::LocalBoxFuture};
    use moosicbox_web_server_core::WebServer;
    #[cfg(feature = "cors")]
    use moosicbox_web_server_cors::AllOrSome;

    #[allow(clippy::fallible_impl_from)]
    impl From<HttpRequest> for actix_web::HttpRequest {
        fn from(value: HttpRequest) -> Self {
            #[allow(clippy::match_wildcard_for_single_variants)]
            match value {
                HttpRequest::Actix(x) => x,
                _ => panic!("Invalid HttpRequest"),
            }
        }
    }

    #[allow(clippy::fallible_impl_from)]
    impl<'a> From<HttpRequestRef<'a>> for &'a actix_web::HttpRequest {
        fn from(value: HttpRequestRef<'a>) -> Self {
            #[allow(clippy::match_wildcard_for_single_variants)]
            match value {
                HttpRequestRef::Actix(x) => x,
                _ => panic!("Invalid HttpRequest"),
            }
        }
    }

    impl From<actix_web::HttpRequest> for HttpRequest {
        fn from(value: actix_web::HttpRequest) -> Self {
            Self::Actix(value)
        }
    }

    impl<'a> From<&'a actix_web::HttpRequest> for HttpRequestRef<'a> {
        fn from(value: &'a actix_web::HttpRequest) -> Self {
            Self::Actix(value)
        }
    }

    impl From<WebServerError> for Error {
        fn from(value: WebServerError) -> Self {
            match value {
                WebServerError::BadRequest => ErrorBadRequest(value),
                WebServerError::NotFound => ErrorNotFound(value),
                WebServerError::InternalServerError => ErrorInternalServerError(value),
            }
        }
    }

    impl From<Error> for WebServerError {
        fn from(_value: Error) -> Self {
            Self::InternalServerError
        }
    }

    impl Service<ServiceRequest> for crate::Route {
        type Response = ServiceResponse;
        type Error = WebServerError;
        type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

        dev::always_ready!();

        #[allow(clippy::similar_names)]
        fn call(&self, req: ServiceRequest) -> Self::Future {
            let (req, _) = req.into_parts();

            let handler = self.handler;

            Box::pin(async move {
                handler(req.clone().into())
                    .map(|x| {
                        x.map(|x| {
                            let res = actix_web::HttpResponseBuilder::new(StatusCode::OK).body(
                                match x.body {
                                    crate::HttpResponseBody::Bytes(bytes) => bytes,
                                },
                            );
                            ServiceResponse::new(req, res)
                        })
                    })
                    .await
            })
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

    // #[async_trait]
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
        fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
            let server = HttpServer::new(self.factory.clone());
            let server = server.bind(&self.addr).unwrap();
            let server = server.run();
            *self.handle.write().unwrap() = Some(server.handle());
            Box::pin(async move {
                if let Err(e) = server.await {
                    log::error!("Error running actix server: {e:?}");
                }
            })
        }

        fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
            let handle = self.handle.write().unwrap().take();
            if let Some(handle) = handle {
                return Box::pin(handle.stop(true));
            }
            Box::pin(future::ready(()))
        }
    }

    use crate::{HttpRequest, HttpRequestRef, WebServerBuilder, WebServerError};

    impl WebServerBuilder {
        #[must_use]
        pub fn build_actix(self) -> Box<dyn WebServer> {
            #[cfg(feature = "cors")]
            let cors_builder = self.cors.clone();
            let factory = move || {
                #[cfg(feature = "cors")]
                let cors = {
                    let cors = actix_cors::Cors::default()
                        .max_age(cors_builder.max_age.map(|x| x as usize));

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
                    let mut app = actix_web::App::new();

                    #[cfg(feature = "htmx")]
                    let mut app = app.wrap(actix_htmx::HtmxMiddleware {});

                    #[cfg(feature = "cors")]
                    let mut app = app.wrap(cors);

                    app
                };

                for scope in &self.scopes {
                    let mut actix_scope = actix_web::web::scope(scope.path);
                    for route in &scope.routes {
                        let path = route.path;
                        let route = route.clone();
                        let factory = fn_factory(move || {
                            let route = route.clone();
                            async { Ok(route) }
                        });
                        actix_scope = actix_scope.service(
                            Resource::new(path).route(actix_web::Route::new().service(factory)),
                        );
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
}
