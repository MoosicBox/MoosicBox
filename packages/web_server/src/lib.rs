//! A flexible, backend-agnostic HTTP web server framework for Rust.
//!
//! `moosicbox_web_server` provides a unified API for building HTTP web servers that can run on
//! different backends (Actix-web or a built-in simulator) without changing application code.
//!
//! # Features
//!
//! * **Multiple backends**: Switch between Actix-web (production) and Simulator (testing) with feature flags
//! * **Type-safe extractors**: Extract request data with compile-time type checking using [`FromRequest`]
//! * **Builder pattern**: Construct servers with a fluent API using [`WebServerBuilder`]
//! * **Route organization**: Group related routes into scopes with [`Scope`]
//! * **Testing utilities**: Built-in test client and simulator for deterministic testing
//! * **`OpenAPI` support**: Optional `OpenAPI` documentation generation with the `openapi` feature
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use moosicbox_web_server::{WebServerBuilder, Scope, Route, Method, HttpResponse};
//! use moosicbox_web_server_core::WebServer;
//!
//! # async fn example() {
//! // Create a simple handler
//! async fn hello_handler() -> Result<HttpResponse, moosicbox_web_server::Error> {
//!     Ok(HttpResponse::text("Hello, World!"))
//! }
//!
//! // Build and start the server
//! let server = WebServerBuilder::new()
//!     .with_addr("127.0.0.1")
//!     .with_port(8080_u16)
//!     .with_scope(
//!         Scope::new("/api")
//!             .route(Method::Get, "/hello", |_req| {
//!                 Box::pin(hello_handler())
//!             })
//!     )
//!     .build();
//!
//! server.start().await;
//! # }
//! ```
//!
//! # Using Extractors
//!
//! Extract typed data from requests using the extractor system:
//!
//! ```rust,ignore
//! use moosicbox_web_server::extractors::{Query, Json, Path};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct SearchParams {
//!     q: String,
//!     limit: Option<u32>,
//! }
//!
//! async fn search(Query(params): Query<SearchParams>) -> Result<HttpResponse, Error> {
//!     // Use params.q and params.limit
//!     Ok(HttpResponse::json(&format!("Searching for: {}", params.q))?)
//! }
//! ```
//!
//! # Feature Flags
//!
//! * `actix` - Enable Actix-web backend (production use)
//! * `simulator` - Enable built-in simulator backend (testing)
//! * `serde` - Enable JSON/query/form extractors
//! * `cors` - Enable CORS middleware support
//! * `openapi` - Enable `OpenAPI` documentation generation
//! * `compress` - Enable response compression
//!
//! # Architecture
//!
//! The crate provides several key abstractions:
//!
//! * [`WebServerBuilder`] - Configures and builds web servers
//! * [`Scope`] - Groups related routes under a common path prefix
//! * [`Route`] - Defines individual HTTP endpoints
//! * [`HttpRequest`] / [`HttpResponse`] - Backend-agnostic request/response types
//! * [`FromRequest`] - Trait for extracting typed data from requests
//! * [`handler::IntoHandler`] - Trait for converting functions into HTTP handlers
//!
//! # Testing
//!
//! The crate includes comprehensive testing support:
//!
//! ```rust,no_run
//! use moosicbox_web_server::test_client::{ConcreteTestClient, TestClient, TestResponseExt};
//!
//! # fn test_example() {
//! let client = ConcreteTestClient::new_with_test_routes();
//! let response = client.get("/test").send().expect("Request failed");
//! response.assert_status(200);
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{borrow::Cow, collections::BTreeMap, pin::Pin};

use bytes::Bytes;

/// Type alias for path parameters extracted from route matching
pub type PathParams = BTreeMap<String, String>;

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
#[cfg(feature = "serde")]
pub use extractors::Path;
pub use from_request::{FromRequest, Headers, IntoHandlerError, RequestData, RequestInfo};
#[cfg(feature = "serde")]
pub use from_request::{Json, Query};
pub use request_context::RequestContext;

#[cfg(feature = "actix")]
mod actix;

// New extractors module with enhanced functionality
pub mod extractors;

pub mod from_request;
pub mod handler;

#[cfg(feature = "openapi")]
pub mod openapi;

pub mod request_context;

pub mod simulator;

pub mod test_client;

/// Builder for configuring and creating web servers.
///
/// The `WebServerBuilder` uses a fluent API to configure server settings before building
/// the final [`WebServer`] instance. It supports different backends (Actix or Simulator)
/// selected via feature flags.
///
/// # Example
///
/// ```rust,no_run
/// use moosicbox_web_server::{WebServerBuilder, Scope, Method, HttpResponse};
/// use moosicbox_web_server_core::WebServer;
///
/// # async fn example() {
/// let server = WebServerBuilder::new()
///     .with_addr("127.0.0.1")
///     .with_port(8080_u16)
///     .with_scope(
///         Scope::new("/api")
///             .get("/hello", |_req| {
///                 Box::pin(async { Ok(HttpResponse::text("Hello!")) })
///             })
///     )
///     .build();
///
/// server.start().await;
/// # }
/// ```
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
    /// Creates a new `WebServerBuilder` with default settings.
    ///
    /// Default configuration:
    /// * Address: `0.0.0.0` (all interfaces)
    /// * Port: `8080`
    /// * No scopes configured
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

    /// Adds a [`Scope`] to the server configuration.
    ///
    /// Scopes group related routes under a common path prefix. Multiple scopes
    /// can be added by chaining calls to this method.
    #[must_use]
    pub fn with_scope<S: Into<Scope>>(mut self, scope: S) -> Self {
        self.scopes.push(scope.into());
        self
    }

    /// Sets the bind address for the server.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use moosicbox_web_server::WebServerBuilder;
    /// let builder = WebServerBuilder::new().with_addr("127.0.0.1");
    /// ```
    #[must_use]
    pub fn with_addr<T: Into<String>>(mut self, addr: T) -> Self {
        self.addr = addr.into();
        self
    }

    /// Sets the port number for the server.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use moosicbox_web_server::WebServerBuilder;
    /// let builder = WebServerBuilder::new().with_port(3000_u16);
    /// ```
    #[must_use]
    pub fn with_port<T: Into<u16>>(mut self, port: T) -> Self {
        self.port = port.into();
        self
    }
}

#[cfg(feature = "cors")]
impl WebServerBuilder {
    /// Configures CORS (Cross-Origin Resource Sharing) settings.
    ///
    /// Only available when the `cors` feature is enabled.
    #[must_use]
    pub fn with_cors(mut self, cors: cors::Cors) -> Self {
        self.cors = cors;
        self
    }
}

#[cfg(feature = "compress")]
impl WebServerBuilder {
    /// Enables or disables response compression.
    ///
    /// Only available when the `compress` feature is enabled.
    #[must_use]
    pub const fn with_compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }
}

/// Handle to a running web server instance.
///
/// Currently a placeholder for future functionality. In the future, this will
/// provide methods to control the running server (start, stop, restart).
pub struct WebServerHandle {}

impl WebServerHandle {
    // pub async fn start(&self) {}
    // pub async fn stop(&self) {}
    // pub async fn restart(&self) {
    //     self.stop().await;
    //     self.start().await;
    // }
}

/// Backend-agnostic HTTP request wrapper.
///
/// This enum wraps different backend request types (Actix or Stub) and provides
/// a unified interface for accessing request data. It includes request context
/// for storing path parameters and other request-scoped data.
///
/// # Variants
///
/// * `Actix` - Wraps an Actix web request (requires `actix` feature)
/// * `Stub` - Test/simulator request stub
#[derive(Debug, Clone)]
pub enum HttpRequest {
    /// Actix web request wrapper (requires `actix` feature)
    #[cfg(feature = "actix")]
    Actix {
        /// The underlying Actix `HttpRequest`
        inner: actix_web::HttpRequest,
        /// Request context holding path parameters and other request-scoped data
        context: std::sync::Arc<RequestContext>,
    },
    /// Test or simulator request stub
    Stub(Stub),
}

impl HttpRequest {
    /// Returns a reference to the request as an [`HttpRequestRef`].
    ///
    /// This method provides a borrowed view of the request that can be used
    /// to access request data without moving the request.
    #[must_use]
    pub const fn as_ref(&self) -> HttpRequestRef<'_> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => HttpRequestRef::Actix(inner),
            Self::Stub(x) => HttpRequestRef::Stub(x),
        }
    }
}

impl HttpRequest {
    /// Returns all path parameters extracted from route matching.
    ///
    /// Path parameters are extracted from dynamic route segments like `/users/{id}`.
    /// Returns an empty map if no path parameters are present.
    #[must_use]
    pub fn path_params(&self) -> &PathParams {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { context, .. } => &context.path_params,
            Self::Stub(Stub::Simulator(sim)) => &sim.request.path_params,
            Self::Stub(Stub::Empty) => {
                static EMPTY: PathParams = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Returns a specific path parameter by name.
    ///
    /// Returns `None` if the parameter doesn't exist.
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params().get(name).map(String::as_str)
    }

    /// Returns the request context for advanced use cases.
    ///
    /// The request context contains request-scoped data such as path parameters.
    /// Returns `None` for stub requests.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn context(&self) -> Option<&RequestContext> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { context, .. } => Some(context),
            _ => None,
        }
    }

    /// Returns a header value by name.
    ///
    /// Header name lookup is case-insensitive. Returns `None` if the header doesn't exist.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => inner.headers().get(name).and_then(|x| x.to_str().ok()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.header(name),
            },
        }
    }

    /// Returns the request path (e.g., `/api/users`).
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => inner.path(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                Stub::Simulator(sim) => sim.path(),
            },
        }
    }

    /// Returns the query string without the leading `?` (e.g., `name=john&age=30`).
    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => inner.query_string(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                Stub::Simulator(sim) => sim.query_string(),
            },
        }
    }

    /// Returns the HTTP method (GET, POST, etc.).
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn method(&self) -> Method {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => {
                use actix_web::http::Method as ActixMethod;
                match *inner.method() {
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
                Stub::Simulator(sim) => *sim.method(),
            },
        }
    }

    /// Returns the request body as bytes if available.
    ///
    /// Note: For Actix backend, the body is consumed during extraction and may not be available.
    /// For Simulator backend, the body is accessible.
    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { .. } => None, // Actix body is consumed during extraction
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.body(),
            },
        }
    }

    /// Returns a cookie value by name.
    ///
    /// Returns `None` if the cookie doesn't exist.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => inner.cookie(name).map(|c| c.value().to_string()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.cookie(name).map(std::string::ToString::to_string),
            },
        }
    }

    /// Returns all cookies as a map of name-value pairs.
    #[must_use]
    pub fn cookies(&self) -> std::collections::BTreeMap<String, String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => {
                let mut cookies = std::collections::BTreeMap::new();
                if let Ok(cookie_jar) = inner.cookies() {
                    for cookie in cookie_jar.iter() {
                        cookies.insert(cookie.name().to_string(), cookie.value().to_string());
                    }
                }
                cookies
            }
            Self::Stub(stub) => match stub {
                Stub::Empty => std::collections::BTreeMap::new(),
                Stub::Simulator(sim) => sim.cookies().clone(),
            },
        }
    }

    /// Returns the remote client address if available.
    #[must_use]
    pub fn remote_addr(&self) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { inner, .. } => inner
                .connection_info()
                .peer_addr()
                .map(std::string::ToString::to_string),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.remote_addr().map(std::string::ToString::to_string),
            },
        }
    }

    /// Parses the query string into a typed structure.
    ///
    /// # Errors
    ///
    /// * Returns `qs::Error` if the query string parsing fails
    pub fn parse_query<'a, T: serde::Deserialize<'a>>(&'a self) -> Result<T, qs::Error> {
        qs::from_str(self.query_string(), qs::ParseMode::UrlEncoded)
    }
}

/// Request stub for testing and simulation.
///
/// This enum provides different stub implementations for testing HTTP handlers
/// without requiring a real HTTP server.
///
/// # Variants
///
/// * `Empty` - Minimal stub with no data
/// * `Simulator` - Full simulator stub with request data
#[derive(Debug, Clone, Default)]
pub enum Stub {
    /// Minimal stub with no data
    #[default]
    Empty,
    /// Full simulator stub with request data
    Simulator(simulator::SimulationStub),
}

/// Borrowed reference to an HTTP request.
///
/// This enum provides a lightweight, borrowed view of an [`HttpRequest`] that can be
/// used to access request data without moving the request. It mirrors the structure
/// of [`HttpRequest`] but holds references instead of owned values.
///
/// # Variants
///
/// * `Actix` - Reference to an Actix web request (requires `actix` feature)
/// * `Stub` - Reference to a test/simulator request stub
#[derive(Debug, Clone, Copy)]
pub enum HttpRequestRef<'a> {
    /// Reference to an Actix web request (requires `actix` feature)
    #[cfg(feature = "actix")]
    Actix(&'a actix_web::HttpRequest),
    /// Reference to a test/simulator request stub
    Stub(&'a Stub),
}

impl<'a> HttpRequestRef<'a> {
    /// Returns a header value by name.
    ///
    /// Header name lookup is case-insensitive. Returns `None` if the header doesn't exist.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.headers().get(name).and_then(|x| x.to_str().ok()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.header(name),
            },
        }
    }

    /// Returns the request path (e.g., `/api/users`).
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.path(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                Stub::Simulator(sim) => sim.path(),
            },
        }
    }

    /// Returns the query string without the leading `?` (e.g., `name=john&age=30`).
    #[must_use]
    pub fn query_string(&self) -> &str {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.query_string(),
            Self::Stub(stub) => match stub {
                Stub::Empty => "",
                Stub::Simulator(sim) => sim.query_string(),
            },
        }
    }

    /// Returns the HTTP method (GET, POST, etc.).
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
                Stub::Simulator(sim) => *sim.method(),
            },
        }
    }

    /// Returns the request body as bytes if available.
    ///
    /// Note: For Actix backend, the body is consumed during extraction and may not be available.
    /// For Simulator backend, the body is accessible.
    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix { .. } => None, // Actix body is consumed during extraction
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.body(),
            },
        }
    }

    /// Returns a cookie value by name.
    ///
    /// Returns `None` if the cookie doesn't exist.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        match self {
            #[cfg(feature = "actix")]
            Self::Actix(x) => x.cookie(name).map(|c| c.value().to_string()),
            Self::Stub(stub) => match stub {
                Stub::Empty => None,
                Stub::Simulator(sim) => sim.cookie(name).map(std::string::ToString::to_string),
            },
        }
    }

    /// Returns all cookies as a map of name-value pairs.
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
                Stub::Simulator(sim) => sim.cookies().clone(),
            },
        }
    }

    /// Returns the remote client address if available.
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
                Stub::Simulator(sim) => sim.remote_addr().map(std::string::ToString::to_string),
            },
        }
    }

    /// Parses the query string into a typed structure.
    ///
    /// # Errors
    ///
    /// * Returns `qs::Error` if the query string parsing fails
    pub fn parse_query<T: serde::Deserialize<'a>>(&'a self) -> Result<T, qs::Error> {
        qs::from_str(self.query_string(), qs::ParseMode::UrlEncoded)
    }
}

/// HTTP response body container.
///
/// Wraps response body data in a backend-agnostic way. Currently only supports
/// byte sequences, but designed to be extensible for streaming responses in the future.
///
/// # Variants
///
/// * `Bytes` - Raw byte sequence response body
#[derive(Debug)]
pub enum HttpResponseBody {
    /// Raw byte sequence response body
    Bytes(Bytes),
}

impl HttpResponseBody {
    /// Creates a response body from a static string slice.
    ///
    /// This is more efficient than converting through `String` for static content.
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

/// Backend-agnostic HTTP response.
///
/// Represents an HTTP response with status code, headers, and optional body.
/// Provides a fluent builder API for constructing responses.
///
/// # Fields
///
/// * `status_code` - HTTP status code (200, 404, etc.)
/// * `location` - Optional location header for redirects
/// * `headers` - Response headers as key-value pairs
/// * `body` - Optional response body
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::HttpResponse;
///
/// let response = HttpResponse::ok()
///     .with_header("X-Custom", "value")
///     .with_body("Hello, World!");
/// ```
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code (200, 404, etc.)
    pub status_code: StatusCode,
    /// Optional location header for redirects (backwards compatibility)
    pub location: Option<String>,
    /// Response headers as key-value pairs
    pub headers: BTreeMap<String, String>,
    /// Optional response body
    pub body: Option<HttpResponseBody>,
}

impl HttpResponse {
    /// Creates a response with HTTP 200 OK status.
    #[must_use]
    pub fn ok() -> Self {
        Self::new(StatusCode::Ok)
    }

    /// Creates a response with the specified status code.
    #[must_use]
    pub fn from_status_code(status_code: StatusCode) -> Self {
        Self::new(status_code)
    }

    /// Creates a response with HTTP 307 Temporary Redirect status.
    #[must_use]
    pub fn temporary_redirect() -> Self {
        Self::new(StatusCode::TemporaryRedirect)
    }

    /// Creates a response with HTTP 308 Permanent Redirect status.
    #[must_use]
    pub fn permanent_redirect() -> Self {
        Self::new(StatusCode::PermanentRedirect)
    }

    /// Creates a response with HTTP 404 Not Found status.
    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NotFound)
    }
}

impl HttpResponse {
    /// Creates a new response with the specified status code.
    #[must_use]
    pub fn new(status_code: impl Into<StatusCode>) -> Self {
        Self {
            status_code: status_code.into(),
            location: None,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Sets the Location header for redirects.
    ///
    /// The location is set both in the `location` field and as a header for compatibility.
    #[must_use]
    pub fn with_location<T: Into<String>, O: Into<Option<T>>>(mut self, location: O) -> Self {
        if let Some(loc) = location.into() {
            let loc_string = loc.into();
            self.location = Some(loc_string.clone()); // Keep for backwards compatibility
            self.headers.insert("Location".to_string(), loc_string); // Also set in headers
        }
        self
    }

    /// Sets the response body.
    #[must_use]
    pub fn with_body<T: Into<HttpResponseBody>, B: Into<Option<T>>>(mut self, body: B) -> Self {
        self.body = body.into().map(Into::into);
        self
    }

    /// Add a single header to the response
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Set the Content-Type header
    #[must_use]
    pub fn with_content_type(self, content_type: impl Into<String>) -> Self {
        self.with_header("Content-Type", content_type)
    }

    /// Add multiple headers to the response
    #[must_use]
    pub fn with_headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Create a JSON response with automatic Content-Type header
    ///
    /// # Errors
    ///
    /// * Returns `Error::Http` with `StatusCode::InternalServerError` if JSON serialization fails
    #[cfg(feature = "serde")]
    pub fn json<T: serde::Serialize>(value: &T) -> Result<Self, crate::Error> {
        let body = serde_json::to_string(value).map_err(|e| crate::Error::Http {
            status_code: StatusCode::InternalServerError,
            source: Box::new(e),
        })?;

        Ok(Self::ok()
            .with_content_type("application/json")
            .with_body(HttpResponseBody::from(body)))
    }

    /// Create an HTML response with automatic Content-Type header
    #[must_use]
    pub fn html(body: impl Into<String>) -> Self {
        Self::ok()
            .with_content_type("text/html; charset=utf-8")
            .with_body(HttpResponseBody::from(body.into()))
    }

    /// Create a plain text response with automatic Content-Type header
    #[must_use]
    pub fn text(body: impl Into<String>) -> Self {
        Self::ok()
            .with_content_type("text/plain; charset=utf-8")
            .with_body(HttpResponseBody::from(body.into()))
    }
}

/// Groups related routes under a common path prefix.
///
/// Scopes provide a way to organize routes hierarchically and apply common
/// path prefixes. Scopes can contain routes and nested sub-scopes.
///
/// # Fields
///
/// * `path` - Path prefix for all routes in this scope (e.g., `/api`)
/// * `routes` - Routes directly under this scope
/// * `scopes` - Nested sub-scopes
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::{Scope, Method, HttpResponse};
///
/// let api_scope = Scope::new("/api")
///     .get("/users", |_req| {
///         Box::pin(async { Ok(HttpResponse::ok()) })
///     })
///     .with_scope(
///         Scope::new("/admin")
///             .get("/dashboard", |_req| {
///                 Box::pin(async { Ok(HttpResponse::ok()) })
///             })
///     );
/// ```
#[derive(Debug, Clone)]
pub struct Scope {
    /// Path prefix for all routes in this scope (e.g., `/api`)
    pub path: String,
    /// Routes directly under this scope
    pub routes: Vec<Route>,
    /// Nested sub-scopes
    pub scopes: Vec<Scope>,
}

impl Scope {
    /// Creates a new scope with the specified path prefix.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            routes: vec![],
            scopes: vec![],
        }
    }

    /// Adds a single route to this scope.
    #[must_use]
    pub fn with_route(mut self, route: Route) -> Self {
        self.routes.push(route);
        self
    }

    /// Adds multiple routes to this scope.
    #[must_use]
    pub fn with_routes(mut self, routes: impl IntoIterator<Item = Route>) -> Self {
        self.routes.extend(routes);
        self
    }

    /// Adds a route with the specified method, path, and handler.
    ///
    /// This is a convenience method that creates and adds a route in one call.
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

    /// Adds a GET route to this scope.
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

    /// Adds a POST route to this scope.
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

    /// Adds a PUT route to this scope.
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

    /// Adds a DELETE route to this scope.
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

    /// Adds a PATCH route to this scope.
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

    /// Adds a HEAD route to this scope.
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

    /// Adds a nested sub-scope to this scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<Self>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    /// Adds multiple nested sub-scopes to this scope.
    #[must_use]
    pub fn with_scopes<T: Into<Self>>(mut self, scopes: impl IntoIterator<Item = T>) -> Self {
        self.scopes.extend(scopes.into_iter().map(Into::into));
        self
    }
}

/// Web server error types.
///
/// This enum represents errors that can occur during HTTP request processing.
/// All variants include a status code and source error for detailed error reporting.
///
/// # Variants
///
/// * `Http` - HTTP error with associated status code and source error
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP error with associated status code and source error
    #[error("HTTP Error {status_code}: {source:?}")]
    Http {
        /// HTTP status code for the error response
        status_code: StatusCode,
        /// The underlying source error
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl Error {
    /// Creates an HTTP error with the specified status code and source error.
    pub fn from_http_status_code(
        status_code: StatusCode,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Http {
            status_code,
            source: Box::new(source),
        }
    }

    /// Creates an HTTP error with the specified status code (as u16) and source error.
    pub fn from_http_status_code_u16(
        status_code: u16,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::from_http_status_code(StatusCode::from_u16(status_code), source)
    }

    /// Creates a 400 Bad Request error.
    pub fn bad_request(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::BadRequest,
            source: error.into(),
        }
    }

    /// Creates a 401 Unauthorized error.
    pub fn unauthorized(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::Unauthorized,
            source: error.into(),
        }
    }

    /// Creates a 404 Not Found error.
    pub fn not_found(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Http {
            status_code: StatusCode::NotFound,
            source: error.into(),
        }
    }

    /// Creates a 500 Internal Server Error.
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

/// Type alias for route handler functions.
///
/// A route handler is an async function that takes an [`HttpRequest`] and returns
/// a future that resolves to a `Result<HttpResponse, Error>`. Handlers must be
/// `Send + Sync + 'static` to work across different async runtimes.
pub type RouteHandler = Box<
    dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>
        + Send
        + Sync
        + 'static,
>;

/// Represents an HTTP route with method, path, and handler.
///
/// Routes define individual endpoints in the web server. Each route specifies
/// an HTTP method, a path pattern, and a handler function that processes requests.
///
/// # Fields
///
/// * `path` - URL path pattern (e.g., `/users/{id}`)
/// * `method` - HTTP method (GET, POST, etc.)
/// * `handler` - Function that processes requests to this route
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::{Route, Method, HttpRequest, HttpResponse, Error};
/// use std::future::Future;
/// use std::pin::Pin;
///
/// let route = Route::new(
///     Method::Get,
///     "/hello",
///     |_req: HttpRequest| -> Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>> {
///         Box::pin(async { Ok(HttpResponse::text("Hello!")) })
///     }
/// );
/// ```
#[derive(Clone)]
pub struct Route {
    /// URL path pattern (e.g., `/users/{id}`)
    pub path: String,
    /// HTTP method (GET, POST, etc.)
    pub method: Method,
    /// Function that processes requests to this route
    pub handler: std::sync::Arc<RouteHandler>,
}

impl Route {
    /// Creates a new route with the specified method, path, and handler.
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

    /// Creates a new route using the `IntoHandler` trait for type-safe extraction.
    ///
    /// This method allows handlers to use extractors for automatic request data extraction.
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

    /// Creates a new route with a handler that extracts one parameter.
    ///
    /// TODO: Remove this method once Step 8 (Routing Macro System) is complete.
    /// This is technical debt - should be replaced with clean macro API.
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

    /// Creates a new route with a handler that extracts two parameters.
    ///
    /// TODO: Remove this method once Step 8 (Routing Macro System) is complete.
    /// This is technical debt - should be replaced with clean macro API.
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

    /// Creates a GET route with the specified path and handler.
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

    /// Creates a POST route with the specified path and handler.
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

    /// Creates a PUT route with the specified path and handler.
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

    /// Creates a DELETE route with the specified path and handler.
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

    /// Creates a PATCH route with the specified path and handler.
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

    /// Creates a HEAD route with the specified path and handler.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_header() {
        let response = HttpResponse::ok().with_header("X-Custom-Header", "custom-value");

        assert_eq!(
            response.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
    }

    #[test]
    fn test_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("X-Header-1".to_string(), "value-1".to_string());
        headers.insert("X-Header-2".to_string(), "value-2".to_string());

        let response = HttpResponse::ok().with_headers(headers);

        assert_eq!(
            response.headers.get("X-Header-1"),
            Some(&"value-1".to_string())
        );
        assert_eq!(
            response.headers.get("X-Header-2"),
            Some(&"value-2".to_string())
        );
    }

    #[test]
    fn test_with_content_type() {
        let response = HttpResponse::ok().with_content_type("application/json");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_json_response() {
        let data = serde_json::json!({
            "message": "Hello, World!"
        });

        let response = HttpResponse::json(&data).unwrap();

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(response.status_code, StatusCode::Ok);
    }

    #[test]
    fn test_html_response() {
        let response = HttpResponse::html("<h1>Hello, World!</h1>");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.status_code, StatusCode::Ok);
    }

    #[test]
    fn test_text_response() {
        let response = HttpResponse::text("Hello, World!");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"text/plain; charset=utf-8".to_string())
        );
        assert_eq!(response.status_code, StatusCode::Ok);
    }

    #[test]
    fn test_header_chaining() {
        let response = HttpResponse::ok()
            .with_header("X-First", "first-value")
            .with_content_type("application/json")
            .with_header("X-Second", "second-value");

        assert_eq!(
            response.headers.get("X-First"),
            Some(&"first-value".to_string())
        );
        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            response.headers.get("X-Second"),
            Some(&"second-value".to_string())
        );
    }

    // HttpResponseBody From implementations tests
    #[test]
    fn test_response_body_from_str() {
        let body = HttpResponseBody::from("Hello, World!");
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    fn test_response_body_from_static() {
        let body = HttpResponseBody::from_static("Static content");
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    fn test_response_body_from_string() {
        let body = HttpResponseBody::from(String::from("Owned string"));
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    fn test_response_body_from_vec_u8() {
        let vec = vec![72, 101, 108, 108, 111]; // "Hello"
        let body = HttpResponseBody::from(vec);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    fn test_response_body_from_slice() {
        let slice: &[u8] = &[72, 101, 108, 108, 111]; // "Hello"
        let body = HttpResponseBody::from(slice);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    fn test_response_body_from_bytes() {
        let bytes = Bytes::from("test");
        let body = HttpResponseBody::from(bytes);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_response_body_from_json_value() {
        let json = serde_json::json!({"key": "value"});
        let body = HttpResponseBody::from(json);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_response_body_from_json_value_ref() {
        let json = serde_json::json!({"key": "value"});
        let body = HttpResponseBody::from(&json);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    // Error creation methods tests
    #[test]
    fn test_error_bad_request() {
        let err = Error::bad_request("Invalid input");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::BadRequest);
            }
        }
    }

    #[test]
    fn test_error_unauthorized() {
        let err = Error::unauthorized("Not authenticated");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::Unauthorized);
            }
        }
    }

    #[test]
    fn test_error_not_found() {
        let err = Error::not_found("Resource not found");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::NotFound);
            }
        }
    }

    #[test]
    fn test_error_internal_server_error() {
        let err = Error::internal_server_error("Something went wrong");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::InternalServerError);
            }
        }
    }

    #[test]
    fn test_error_from_http_status_code() {
        #[derive(Debug)]
        struct CustomError;
        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Custom error")
            }
        }
        impl std::error::Error for CustomError {}

        let err = Error::from_http_status_code(StatusCode::Forbidden, CustomError);
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::Forbidden);
            }
        }
    }

    #[test]
    fn test_error_from_http_status_code_u16() {
        #[derive(Debug)]
        struct CustomError;
        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Custom error")
            }
        }
        impl std::error::Error for CustomError {}

        let err = Error::from_http_status_code_u16(403, CustomError);
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::Forbidden);
            }
        }
    }

    // HttpResponse builder methods tests
    #[test]
    fn test_http_response_new() {
        let response = HttpResponse::new(StatusCode::Ok);
        assert_eq!(response.status_code, StatusCode::Ok);
        assert!(response.location.is_none());
        assert!(response.headers.is_empty());
        assert!(response.body.is_none());
    }

    #[test]
    fn test_http_response_from_status_code() {
        let response = HttpResponse::from_status_code(StatusCode::Created);
        assert_eq!(response.status_code, StatusCode::Created);
    }

    #[test]
    fn test_http_response_temporary_redirect() {
        let response = HttpResponse::temporary_redirect();
        assert_eq!(response.status_code, StatusCode::TemporaryRedirect);
    }

    #[test]
    fn test_http_response_permanent_redirect() {
        let response = HttpResponse::permanent_redirect();
        assert_eq!(response.status_code, StatusCode::PermanentRedirect);
    }

    #[test]
    fn test_http_response_not_found() {
        let response = HttpResponse::not_found();
        assert_eq!(response.status_code, StatusCode::NotFound);
    }

    #[test]
    fn test_http_response_with_location() {
        let response = HttpResponse::temporary_redirect().with_location("/new-url");
        assert_eq!(response.location, Some("/new-url".to_string()));
        assert_eq!(
            response.headers.get("Location"),
            Some(&"/new-url".to_string())
        );
    }

    #[test]
    fn test_http_response_with_location_none() {
        let response = HttpResponse::ok().with_location::<String, Option<String>>(None);
        assert!(response.location.is_none());
        assert!(!response.headers.contains_key("Location"));
    }

    #[test]
    fn test_http_response_with_body() {
        let response = HttpResponse::ok().with_body("Response body");
        assert!(response.body.is_some());
    }

    #[test]
    fn test_http_response_with_body_none() {
        let response = HttpResponse::ok().with_body::<&str, Option<&str>>(None);
        assert!(response.body.is_none());
    }

    // WebServerBuilder tests
    #[test]
    fn test_web_server_builder_new() {
        let builder = WebServerBuilder::new();
        assert_eq!(builder.addr, "0.0.0.0");
        assert_eq!(builder.port, 8080);
        assert!(builder.scopes.is_empty());
    }

    #[test]
    fn test_web_server_builder_default() {
        let builder = WebServerBuilder::default();
        assert_eq!(builder.addr, "0.0.0.0");
        assert_eq!(builder.port, 8080);
    }

    #[test]
    fn test_web_server_builder_with_addr() {
        let builder = WebServerBuilder::new().with_addr("127.0.0.1");
        assert_eq!(builder.addr, "127.0.0.1");
    }

    #[test]
    fn test_web_server_builder_with_port() {
        let builder = WebServerBuilder::new().with_port(3000_u16);
        assert_eq!(builder.port, 3000);
    }

    #[test]
    fn test_web_server_builder_with_scope() {
        let scope = Scope::new("/api");
        let builder = WebServerBuilder::new().with_scope(scope);
        assert_eq!(builder.scopes.len(), 1);
        assert_eq!(builder.scopes[0].path, "/api");
    }

    #[test]
    fn test_web_server_builder_chaining() {
        let builder = WebServerBuilder::new()
            .with_addr("localhost")
            .with_port(8000_u16)
            .with_scope(Scope::new("/api"))
            .with_scope(Scope::new("/admin"));

        assert_eq!(builder.addr, "localhost");
        assert_eq!(builder.port, 8000);
        assert_eq!(builder.scopes.len(), 2);
    }

    #[test]
    #[cfg(feature = "compress")]
    fn test_web_server_builder_with_compress() {
        let builder = WebServerBuilder::new().with_compress(true);
        assert!(builder.compress);

        let builder = WebServerBuilder::new().with_compress(false);
        assert!(!builder.compress);
    }

    // Scope builder tests
    #[test]
    fn test_scope_new() {
        let scope = Scope::new("/api");
        assert_eq!(scope.path, "/api");
        assert!(scope.routes.is_empty());
        assert!(scope.scopes.is_empty());
    }

    #[test]
    fn test_scope_with_route() {
        let route = Route::new(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        let scope = Scope::new("/api").with_route(route);

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].path, "/test");
    }

    #[test]
    fn test_scope_with_routes() {
        let routes = vec![
            Route::new(Method::Get, "/route1", |_req| {
                Box::pin(async { Ok(HttpResponse::ok()) })
            }),
            Route::new(Method::Post, "/route2", |_req| {
                Box::pin(async { Ok(HttpResponse::ok()) })
            }),
        ];
        let scope = Scope::new("/api").with_routes(routes);

        assert_eq!(scope.routes.len(), 2);
    }

    #[test]
    fn test_scope_route_method() {
        let scope = Scope::new("/api").route(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Get);
        assert_eq!(scope.routes[0].path, "/test");
    }

    #[test]
    fn test_scope_get_method() {
        let scope = Scope::new("/api").get("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Get);
    }

    #[test]
    fn test_scope_post_method() {
        let scope = Scope::new("/api").post("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Post);
    }

    #[test]
    fn test_scope_put_method() {
        let scope = Scope::new("/api").put("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Put);
    }

    #[test]
    fn test_scope_delete_method() {
        let scope = Scope::new("/api").delete("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Delete);
    }

    #[test]
    fn test_scope_patch_method() {
        let scope = Scope::new("/api").patch("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Patch);
    }

    #[test]
    fn test_scope_head_method() {
        let scope = Scope::new("/api").head("/users", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Head);
    }

    #[test]
    fn test_scope_with_scope() {
        let inner_scope = Scope::new("/users");
        let outer_scope = Scope::new("/api").with_scope(inner_scope);

        assert_eq!(outer_scope.scopes.len(), 1);
        assert_eq!(outer_scope.scopes[0].path, "/users");
    }

    #[test]
    fn test_scope_with_scopes() {
        let scopes = vec![Scope::new("/users"), Scope::new("/posts")];
        let scope = Scope::new("/api").with_scopes(scopes);

        assert_eq!(scope.scopes.len(), 2);
    }

    #[test]
    fn test_scope_builder_chaining() {
        let scope = Scope::new("/api")
            .get("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }))
            .post("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }))
            .with_scope(Scope::new("/admin"));

        assert_eq!(scope.routes.len(), 2);
        assert_eq!(scope.scopes.len(), 1);
    }

    // Route builder tests
    #[test]
    fn test_route_new() {
        let route = Route::new(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Get);
        assert_eq!(route.path, "/test");
    }

    #[test]
    fn test_route_get() {
        let route = Route::get("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Get);
    }

    #[test]
    fn test_route_post() {
        let route = Route::post("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Post);
    }

    #[test]
    fn test_route_put() {
        let route = Route::put("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Put);
    }

    #[test]
    fn test_route_delete() {
        let route = Route::delete("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Delete);
    }

    #[test]
    fn test_route_patch() {
        let route = Route::patch("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Patch);
    }

    #[test]
    fn test_route_head() {
        let route = Route::head("/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Head);
    }
}
