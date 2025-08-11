#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{borrow::Cow, pin::Pin};

use bytes::Bytes;

pub use moosicbox_web_server_core as core;
#[cfg(feature = "cors")]
pub use moosicbox_web_server_cors as cors;
pub use paste;
pub use serde_querystring as qs;
pub use switchy_http_models::Method;
use switchy_http_models::StatusCode;
#[cfg(feature = "openapi")]
pub use utoipa;

// Re-export from_request module and key types
pub use from_request::{FromRequest, Headers, IntoHandlerError, RequestData, RequestInfo};
#[cfg(feature = "serde")]
pub use from_request::{Json, Path, Query};

#[cfg(feature = "actix")]
mod actix;

pub mod from_request;
pub mod handler;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(any(feature = "simulator", not(feature = "actix")))]
pub mod simulator;

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

#[derive(Debug, Clone)]
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
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.header(name),
            },
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.path(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.path(),
            },
        }
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.query_string(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.query_string(),
            },
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn method(&self) -> Method {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => {
                use actix_web::http::Method as ActixMethod;
                match *x.method() {
                    ActixMethod::GET => Method::Get,
                    ActixMethod::POST => Method::Post,
                    ActixMethod::PUT => Method::Put,
                    ActixMethod::PATCH => Method::Patch,
                    ActixMethod::DELETE => Method::Delete,
                    ActixMethod::HEAD => Method::Head,
                    ActixMethod::OPTIONS => Method::Options,
                    ActixMethod::CONNECT => Method::Connect,
                    _ => Method::Trace, // Default fallback for unknown methods
                }
            }
            Self::Stub(stub) => match stub {
                Stub::Empty => Method::Get,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => *sim.method(),
            },
        }
    }

    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(_) => None, // Actix body is consumed during extraction
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.body(),
            },
        }
    }

    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.cookie(name).map(|c| c.value().to_string()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.cookie(name).map(std::string::ToString::to_string),
            },
        }
    }

    #[must_use]
    pub fn cookies(&self) -> std::collections::BTreeMap<String, String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => {
                let mut cookies = std::collections::BTreeMap::new();
                if let Ok(cookie_jar) = x.cookies() {
                    for cookie in cookie_jar.iter() {
                        cookies.insert(cookie.name().to_string(), cookie.value().to_string());
                    }
                }
                cookies
            }
            Self::Stub(stub) => match stub {
                Stub::Empty => std::collections::BTreeMap::new(),
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.cookies().clone(),
            },
        }
    }

    #[must_use]
    pub fn remote_addr(&self) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x
                .connection_info()
                .peer_addr()
                .map(std::string::ToString::to_string),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.remote_addr().map(std::string::ToString::to_string),
            },
        }
    }

    /// # Errors
    ///
    /// * If the query string parsing fails
    pub fn parse_query<'a, T: serde::Deserialize<'a>>(&'a self) -> Result<T, qs::Error> {
        qs::from_str(self.query_string(), qs::ParseMode::UrlEncoded)
    }
}

#[derive(Debug, Clone)]
pub enum Stub {
    Empty,
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    Simulator(simulator::SimulationStub),
}

impl Default for Stub {
    fn default() -> Self {
        Self::Empty
    }
}

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
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.header(name),
            },
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.path(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.path(),
            },
        }
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.query_string(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.query_string(),
            },
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn method(&self) -> Method {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => {
                use actix_web::http::Method as ActixMethod;
                match *x.method() {
                    ActixMethod::GET => Method::Get,
                    ActixMethod::POST => Method::Post,
                    ActixMethod::PUT => Method::Put,
                    ActixMethod::PATCH => Method::Patch,
                    ActixMethod::DELETE => Method::Delete,
                    ActixMethod::HEAD => Method::Head,
                    ActixMethod::OPTIONS => Method::Options,
                    ActixMethod::CONNECT => Method::Connect,
                    _ => Method::Trace, // Default fallback for unknown methods
                }
            }
            Self::Stub(stub) => match stub {
                Stub::Empty => Method::Get,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => *sim.method(),
            },
        }
    }

    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(_) => None, // Actix body is consumed during extraction
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.body(),
            },
        }
    }

    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.cookie(name).map(|c| c.value().to_string()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.cookie(name).map(std::string::ToString::to_string),
            },
        }
    }

    #[must_use]
    pub fn cookies(&self) -> std::collections::BTreeMap<String, String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => {
                let mut cookies = std::collections::BTreeMap::new();
                if let Ok(cookie_jar) = x.cookies() {
                    for cookie in cookie_jar.iter() {
                        cookies.insert(cookie.name().to_string(), cookie.value().to_string());
                    }
                }
                cookies
            }
            Self::Stub(stub) => match stub {
                Stub::Empty => std::collections::BTreeMap::new(),
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.cookies().clone(),
            },
        }
    }

    #[must_use]
    pub fn remote_addr(&self) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x
                .connection_info()
                .peer_addr()
                .map(std::string::ToString::to_string),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                #[cfg(any(feature = "simulator", not(feature = "actix")))]
                Stub::Simulator(sim) => sim.remote_addr().map(std::string::ToString::to_string),
            },
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

impl From<Bytes> for HttpResponseBody {
    fn from(value: Bytes) -> Self {
        Self::Bytes(value)
    }
}

impl From<Vec<u8>> for HttpResponseBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value.into())
    }
}

impl From<&[u8]> for HttpResponseBody {
    fn from(value: &[u8]) -> Self {
        value.to_vec().into()
    }
}

impl<'a> From<Cow<'a, [u8]>> for HttpResponseBody {
    fn from(value: Cow<'a, [u8]>) -> Self {
        value.to_vec().into()
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
    pub status_code: StatusCode,
    pub location: Option<String>,
    pub body: Option<HttpResponseBody>,
}

impl HttpResponse {
    #[must_use]
    pub fn ok() -> Self {
        Self::new(StatusCode::Ok)
    }

    #[must_use]
    pub fn from_status_code(status_code: StatusCode) -> Self {
        Self::new(status_code)
    }

    #[must_use]
    pub fn temporary_redirect() -> Self {
        Self::new(StatusCode::TemporaryRedirect)
    }

    #[must_use]
    pub fn permanent_redirect() -> Self {
        Self::new(StatusCode::PermanentRedirect)
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NotFound)
    }
}

impl HttpResponse {
    #[must_use]
    pub fn new(status_code: impl Into<StatusCode>) -> Self {
        Self {
            status_code: status_code.into(),
            location: None,
            body: None,
        }
    }

    #[must_use]
    pub fn with_location<T: Into<String>, O: Into<Option<T>>>(mut self, location: O) -> Self {
        self.location = location.into().map(Into::into);
        self
    }

    #[must_use]
    pub fn with_body<T: Into<HttpResponseBody>, B: Into<Option<T>>>(mut self, body: B) -> Self {
        self.body = body.into().map(Into::into);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub path: String,
    pub routes: Vec<Route>,
    pub scopes: Vec<Scope>,
}

impl Scope {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            routes: vec![],
            scopes: vec![],
        }
    }

    #[must_use]
    pub fn with_route(mut self, route: Route) -> Self {
        self.routes.push(route);
        self
    }

    #[must_use]
    pub fn with_routes(mut self, routes: impl IntoIterator<Item = Route>) -> Self {
        self.routes.extend(routes);
        self
    }

    #[must_use]
    pub fn route<F>(mut self, method: Method, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.routes.push(Route::new(method, path, handler));
        self
    }

    #[must_use]
    pub fn get<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Get, path, handler)
    }

    #[must_use]
    pub fn post<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Post, path, handler)
    }

    #[must_use]
    pub fn put<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Put, path, handler)
    }

    #[must_use]
    pub fn delete<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Delete, path, handler)
    }

    #[must_use]
    pub fn patch<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Patch, path, handler)
    }

    #[must_use]
    pub fn head<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.route(Method::Head, path, handler)
    }

    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<Self>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    #[must_use]
    pub fn with_scopes<T: Into<Self>>(mut self, scopes: impl IntoIterator<Item = T>) -> Self {
        self.scopes.extend(scopes.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP Error {status_code}: {source:?}")]
    Http {
        status_code: StatusCode,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl Error {
    pub fn from_http_status_code(
        status_code: StatusCode,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Http {
            status_code,
            source: Box::new(source),
        }
    }

    pub fn from_http_status_code_u16(
        status_code: u16,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::from_http_status_code(StatusCode::from_u16(status_code), source)
    }

    pub fn bad_request(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::BadRequest,
            source: error.into(),
        }
    }

    pub fn unauthorized(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::Unauthorized,
            source: error.into(),
        }
    }

    pub fn not_found(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::NotFound,
            source: error.into(),
        }
    }

    pub fn internal_server_error(
        error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self::Http {
            status_code: StatusCode::InternalServerError,
            source: error.into(),
        }
    }
}

impl From<qs::Error> for Error {
    fn from(value: qs::Error) -> Self {
        Self::bad_request(value)
    }
}

// FromRequest trait moved to from_request.rs module

pub type RouteHandler = Box<
    dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
        + Send
        + Sync
        + 'static,
>;

#[derive(Clone)]
pub struct Route {
    pub path: String,
    pub method: Method,
    pub handler: std::sync::Arc<RouteHandler>,
}

impl Route {
    #[must_use]
    pub fn new<F>(method: Method, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self {
            path: path.into(),
            method,
            handler: std::sync::Arc::new(Box::new(handler)),
        }
    }

    #[must_use]
    pub fn with_handler<H>(method: Method, path: impl Into<String>, handler: H) -> Self
    where
        H: crate::handler::IntoHandler<()> + Send + Sync + 'static,
        H::Future: Send + 'static,
    {
        let handler_fn = handler.into_handler();
        Self {
            path: path.into(),
            method,
            handler: std::sync::Arc::new(Box::new(move |req| Box::pin(handler_fn(req)))),
        }
    }

    // TODO: Remove this method once Step 8 (Routing Macro System) is complete
    // This is technical debt - should be replaced with clean macro API
    #[must_use]
    pub fn with_handler1<H, T1>(method: Method, path: impl Into<String>, handler: H) -> Self
    where
        H: crate::handler::IntoHandler<(T1,)> + Send + Sync + 'static,
        H::Future: Send + 'static,
        T1: crate::from_request::FromRequest + Send + 'static,
    {
        let handler_fn = handler.into_handler();
        Self {
            path: path.into(),
            method,
            handler: std::sync::Arc::new(Box::new(move |req| Box::pin(handler_fn(req)))),
        }
    }

    // TODO: Remove this method once Step 8 (Routing Macro System) is complete
    // This is technical debt - should be replaced with clean macro API
    #[must_use]
    pub fn with_handler2<H, T1, T2>(method: Method, path: impl Into<String>, handler: H) -> Self
    where
        H: crate::handler::IntoHandler<(T1, T2)> + Send + Sync + 'static,
        H::Future: Send + 'static,
        T1: crate::from_request::FromRequest + Send + 'static,
        T2: crate::from_request::FromRequest + Send + 'static,
    {
        let handler_fn = handler.into_handler();
        Self {
            path: path.into(),
            method,
            handler: std::sync::Arc::new(Box::new(move |req| Box::pin(handler_fn(req)))),
        }
    }

    #[must_use]
    pub fn get<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Get, path, handler)
    }

    #[must_use]
    pub fn post<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Post, path, handler)
    }

    #[must_use]
    pub fn put<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Put, path, handler)
    }

    #[must_use]
    pub fn delete<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Delete, path, handler)
    }

    #[must_use]
    pub fn patch<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Patch, path, handler)
    }

    #[must_use]
    pub fn head<F>(path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self::new(Method::Head, path, handler)
    }
}

impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("path", &self.path)
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

#[allow(unused)]
macro_rules! impl_web_server {
    ($module:ident $(,)?) => {
        use moosicbox_web_server_core::WebServer;

        impl WebServerBuilder {
            /// # Errors
            ///
            /// * If the underlying `WebServer` fails to build
            #[must_use]
            pub fn build(self) -> Box<dyn WebServer> {
                paste::paste! {
                    Self::[< build_ $module >](self)
                }
            }
        }
    };
}

#[cfg(any(feature = "simulator", not(feature = "actix")))]
impl_web_server!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "actix"))]
impl_web_server!(actix);
