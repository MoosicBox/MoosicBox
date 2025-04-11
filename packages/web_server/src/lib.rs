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

#[cfg(feature = "actix")]
mod actix;

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
