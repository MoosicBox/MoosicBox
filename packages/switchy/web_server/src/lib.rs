//! A flexible, backend-agnostic HTTP web server framework for Rust.
//!
//! `switchy_web_server` provides a unified API for building HTTP web servers that can run on
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
//! use switchy_web_server::{WebServerBuilder, Scope, Route, Method, HttpResponse};
//! use switchy_web_server_core::WebServer;
//!
//! # async fn example() {
//! // Create a simple handler
//! async fn hello_handler() -> Result<HttpResponse, switchy_web_server::Error> {
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
//! use switchy_web_server::extractors::{Query, Json, Path};
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
//! use switchy_web_server::test_client::{ConcreteTestClient, TestClient, TestResponseExt};
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

pub use paste;
pub use serde_querystring as qs;
pub use switchy_http_models::Method;
use switchy_http_models::StatusCode;
pub use switchy_web_server_core as core;
#[cfg(feature = "cors")]
pub use switchy_web_server_cors as cors;
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

pub mod request;
pub mod static_files;

pub use request::{EmptyRequest, HttpRequest, HttpRequestTrait};
pub use static_files::StaticFiles;

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
/// use switchy_web_server::{WebServerBuilder, Scope, Method, HttpResponse};
/// use switchy_web_server_core::WebServer;
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
    static_files: Option<StaticFiles>,
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
            static_files: None,
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
    /// # use switchy_web_server::WebServerBuilder;
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
    /// # use switchy_web_server::WebServerBuilder;
    /// let builder = WebServerBuilder::new().with_port(3000_u16);
    /// ```
    #[must_use]
    pub fn with_port<T: Into<u16>>(mut self, port: T) -> Self {
        self.port = port.into();
        self
    }

    /// Returns the configured bind address.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Returns the configured port number.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns a reference to the configured scopes.
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Configures static file serving.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use switchy_web_server::{WebServerBuilder, StaticFiles};
    /// let builder = WebServerBuilder::new()
    ///     .with_static_files(StaticFiles::new("/static", "./public"));
    /// ```
    #[must_use]
    pub fn with_static_files(mut self, config: StaticFiles) -> Self {
        self.static_files = Some(config);
        self
    }

    /// Returns the configured static files, if any.
    #[must_use]
    pub const fn static_files(&self) -> Option<&StaticFiles> {
        self.static_files.as_ref()
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

    /// Returns a reference to the configured CORS settings.
    ///
    /// Only available when the `cors` feature is enabled.
    #[must_use]
    pub const fn cors(&self) -> &cors::Cors {
        &self.cors
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

// NOTE: The old HttpRequest enum, Stub enum, and HttpRequestRef enum have been removed.
// Use the trait-based HttpRequest from the request module instead.
// For testing, use: HttpRequest::new(SimulationStub::new(sim_req))
// See request.rs for the HttpRequestTrait-based implementation.

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
/// use switchy_web_server::HttpResponse;
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
/// use switchy_web_server::{Scope, Method, HttpResponse};
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
    pub scopes: Vec<Self>,
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

    /// Returns the path prefix for this scope.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the routes in this scope.
    #[must_use]
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Returns the nested scopes in this scope.
    #[must_use]
    pub fn scopes(&self) -> &[Self] {
        &self.scopes
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
/// use switchy_web_server::{Route, Method, HttpRequest, HttpResponse, Error};
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

    /// Returns the path pattern for this route.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the HTTP method for this route.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// Returns the handler for this route.
    #[must_use]
    pub fn handler(&self) -> &std::sync::Arc<RouteHandler> {
        &self.handler
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

/// Macro to implement the default `build()` method based on enabled features.
///
/// # Feature Priority
///
/// The default backend is selected based on the following priority:
/// 1. **Simulator** - If `simulator` feature is enabled (takes precedence for deterministic testing)
/// 2. **Actix** - If `actix` feature is enabled and `simulator` is not
/// 3. **Simulator fallback** - If neither feature is explicitly enabled
#[allow(unused)]
macro_rules! impl_web_server {
    ($module:ident $(,)?) => {
        use switchy_web_server_core::WebServer;

        impl WebServerBuilder {
            /// Builds the web server using the default backend based on enabled features.
            ///
            /// # Backend Selection Priority
            ///
            /// 1. **Simulator** - If `simulator` feature is enabled (takes precedence)
            /// 2. **Actix** - If only `actix` feature is enabled
            /// 3. **Simulator fallback** - If neither feature is explicitly enabled
            ///
            /// This method is an alias for `build_default()`.
            ///
            /// # Returns
            ///
            /// Returns a boxed `WebServer` trait object that can be started with `start()`.
            #[must_use]
            pub fn build(self) -> Box<dyn WebServer> {
                paste::paste! {
                    Self::[< build_ $module >](self)
                }
            }

            /// Builds the web server using the default backend based on enabled features.
            ///
            /// # Backend Selection Priority
            ///
            /// 1. **Simulator** - If `simulator` feature is enabled (takes precedence)
            /// 2. **Actix** - If only `actix` feature is enabled
            /// 3. **Simulator fallback** - If neither feature is explicitly enabled
            ///
            /// Use `build_simulator()` or `build_actix()` to explicitly select a backend.
            ///
            /// # Returns
            ///
            /// Returns a boxed `WebServer` trait object that can be started with `start()`.
            #[must_use]
            pub fn build_default(self) -> Box<dyn WebServer> {
                paste::paste! {
                    Self::[< build_ $module >](self)
                }
            }
        }
    };
}

// Simulator takes priority when enabled (for deterministic testing)
#[cfg(any(feature = "simulator", not(feature = "actix")))]
impl_web_server!(simulator);

// Actix is used only when simulator is not enabled
#[cfg(all(not(feature = "simulator"), feature = "actix"))]
impl_web_server!(actix);

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_with_header() {
        let response = HttpResponse::ok().with_header("X-Custom-Header", "custom-value");

        assert_eq!(
            response.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_with_content_type() {
        let response = HttpResponse::ok().with_content_type("application/json");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_html_response() {
        let response = HttpResponse::html("<h1>Hello, World!</h1>");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.status_code, StatusCode::Ok);
    }

    #[test_log::test]
    fn test_text_response() {
        let response = HttpResponse::text("Hello, World!");

        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"text/plain; charset=utf-8".to_string())
        );
        assert_eq!(response.status_code, StatusCode::Ok);
    }

    #[test_log::test]
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
    #[test_log::test]
    fn test_response_body_from_str() {
        let body = HttpResponseBody::from("Hello, World!");
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    fn test_response_body_from_static() {
        let body = HttpResponseBody::from_static("Static content");
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    fn test_response_body_from_string() {
        let body = HttpResponseBody::from(String::from("Owned string"));
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    fn test_response_body_from_vec_u8() {
        let vec = vec![72, 101, 108, 108, 111]; // "Hello"
        let body = HttpResponseBody::from(vec);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    fn test_response_body_from_slice() {
        let slice: &[u8] = &[72, 101, 108, 108, 111]; // "Hello"
        let body = HttpResponseBody::from(slice);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    fn test_response_body_from_bytes() {
        let bytes = Bytes::from("test");
        let body = HttpResponseBody::from(bytes);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_response_body_from_json_value() {
        let json = serde_json::json!({"key": "value"});
        let body = HttpResponseBody::from(json);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_response_body_from_json_value_ref() {
        let json = serde_json::json!({"key": "value"});
        let body = HttpResponseBody::from(&json);
        assert!(matches!(body, HttpResponseBody::Bytes(_)));
    }

    // Error creation methods tests
    #[test_log::test]
    fn test_error_bad_request() {
        let err = Error::bad_request("Invalid input");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::BadRequest);
            }
        }
    }

    #[test_log::test]
    fn test_error_unauthorized() {
        let err = Error::unauthorized("Not authenticated");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::Unauthorized);
            }
        }
    }

    #[test_log::test]
    fn test_error_not_found() {
        let err = Error::not_found("Resource not found");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::NotFound);
            }
        }
    }

    #[test_log::test]
    fn test_error_internal_server_error() {
        let err = Error::internal_server_error("Something went wrong");
        match err {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, StatusCode::InternalServerError);
            }
        }
    }

    #[test_log::test]
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

    #[test_log::test]
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
    #[test_log::test]
    fn test_http_response_new() {
        let response = HttpResponse::new(StatusCode::Ok);
        assert_eq!(response.status_code, StatusCode::Ok);
        assert!(response.location.is_none());
        assert!(response.headers.is_empty());
        assert!(response.body.is_none());
    }

    #[test_log::test]
    fn test_http_response_from_status_code() {
        let response = HttpResponse::from_status_code(StatusCode::Created);
        assert_eq!(response.status_code, StatusCode::Created);
    }

    #[test_log::test]
    fn test_http_response_temporary_redirect() {
        let response = HttpResponse::temporary_redirect();
        assert_eq!(response.status_code, StatusCode::TemporaryRedirect);
    }

    #[test_log::test]
    fn test_http_response_permanent_redirect() {
        let response = HttpResponse::permanent_redirect();
        assert_eq!(response.status_code, StatusCode::PermanentRedirect);
    }

    #[test_log::test]
    fn test_http_response_not_found() {
        let response = HttpResponse::not_found();
        assert_eq!(response.status_code, StatusCode::NotFound);
    }

    #[test_log::test]
    fn test_http_response_with_location() {
        let response = HttpResponse::temporary_redirect().with_location("/new-url");
        assert_eq!(response.location, Some("/new-url".to_string()));
        assert_eq!(
            response.headers.get("Location"),
            Some(&"/new-url".to_string())
        );
    }

    #[test_log::test]
    fn test_http_response_with_location_none() {
        let response = HttpResponse::ok().with_location::<String, Option<String>>(None);
        assert!(response.location.is_none());
        assert!(!response.headers.contains_key("Location"));
    }

    #[test_log::test]
    fn test_http_response_with_body() {
        let response = HttpResponse::ok().with_body("Response body");
        assert!(response.body.is_some());
    }

    #[test_log::test]
    fn test_http_response_with_body_none() {
        let response = HttpResponse::ok().with_body::<&str, Option<&str>>(None);
        assert!(response.body.is_none());
    }

    // WebServerBuilder tests
    #[test_log::test]
    fn test_web_server_builder_new() {
        let builder = WebServerBuilder::new();
        assert_eq!(builder.addr, "0.0.0.0");
        assert_eq!(builder.port, 8080);
        assert!(builder.scopes.is_empty());
    }

    #[test_log::test]
    fn test_web_server_builder_default() {
        let builder = WebServerBuilder::default();
        assert_eq!(builder.addr, "0.0.0.0");
        assert_eq!(builder.port, 8080);
    }

    #[test_log::test]
    fn test_web_server_builder_with_addr() {
        let builder = WebServerBuilder::new().with_addr("127.0.0.1");
        assert_eq!(builder.addr, "127.0.0.1");
    }

    #[test_log::test]
    fn test_web_server_builder_with_port() {
        let builder = WebServerBuilder::new().with_port(3000_u16);
        assert_eq!(builder.port, 3000);
    }

    #[test_log::test]
    fn test_web_server_builder_with_scope() {
        let scope = Scope::new("/api");
        let builder = WebServerBuilder::new().with_scope(scope);
        assert_eq!(builder.scopes.len(), 1);
        assert_eq!(builder.scopes[0].path, "/api");
    }

    #[test_log::test]
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

    #[test_log::test]
    #[cfg(feature = "compress")]
    fn test_web_server_builder_with_compress() {
        let builder = WebServerBuilder::new().with_compress(true);
        assert!(builder.compress);

        let builder = WebServerBuilder::new().with_compress(false);
        assert!(!builder.compress);
    }

    // Scope builder tests
    #[test_log::test]
    fn test_scope_new() {
        let scope = Scope::new("/api");
        assert_eq!(scope.path, "/api");
        assert!(scope.routes.is_empty());
        assert!(scope.scopes.is_empty());
    }

    #[test_log::test]
    fn test_scope_with_route() {
        let route = Route::new(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        let scope = Scope::new("/api").with_route(route);

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].path, "/test");
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_scope_route_method() {
        let scope = Scope::new("/api").route(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Get);
        assert_eq!(scope.routes[0].path, "/test");
    }

    #[test_log::test]
    fn test_scope_get_method() {
        let scope =
            Scope::new("/api").get("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Get);
    }

    #[test_log::test]
    fn test_scope_post_method() {
        let scope =
            Scope::new("/api").post("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Post);
    }

    #[test_log::test]
    fn test_scope_put_method() {
        let scope =
            Scope::new("/api").put("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Put);
    }

    #[test_log::test]
    fn test_scope_delete_method() {
        let scope =
            Scope::new("/api").delete("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Delete);
    }

    #[test_log::test]
    fn test_scope_patch_method() {
        let scope =
            Scope::new("/api").patch("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Patch);
    }

    #[test_log::test]
    fn test_scope_head_method() {
        let scope =
            Scope::new("/api").head("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));

        assert_eq!(scope.routes.len(), 1);
        assert_eq!(scope.routes[0].method, Method::Head);
    }

    #[test_log::test]
    fn test_scope_with_scope() {
        let inner_scope = Scope::new("/users");
        let outer_scope = Scope::new("/api").with_scope(inner_scope);

        assert_eq!(outer_scope.scopes.len(), 1);
        assert_eq!(outer_scope.scopes[0].path, "/users");
    }

    #[test_log::test]
    fn test_scope_with_scopes() {
        let scopes = vec![Scope::new("/users"), Scope::new("/posts")];
        let scope = Scope::new("/api").with_scopes(scopes);

        assert_eq!(scope.scopes.len(), 2);
    }

    #[test_log::test]
    fn test_scope_builder_chaining() {
        let scope = Scope::new("/api")
            .get("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }))
            .post("/users", |_req| Box::pin(async { Ok(HttpResponse::ok()) }))
            .with_scope(Scope::new("/admin"));

        assert_eq!(scope.routes.len(), 2);
        assert_eq!(scope.scopes.len(), 1);
    }

    // Route builder tests
    #[test_log::test]
    fn test_route_new() {
        let route = Route::new(Method::Get, "/test", |_req| {
            Box::pin(async { Ok(HttpResponse::ok()) })
        });
        assert_eq!(route.method, Method::Get);
        assert_eq!(route.path, "/test");
    }

    #[test_log::test]
    fn test_route_get() {
        let route = Route::get("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Get);
    }

    #[test_log::test]
    fn test_route_post() {
        let route = Route::post("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Post);
    }

    #[test_log::test]
    fn test_route_put() {
        let route = Route::put("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Put);
    }

    #[test_log::test]
    fn test_route_delete() {
        let route = Route::delete("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Delete);
    }

    #[test_log::test]
    fn test_route_patch() {
        let route = Route::patch("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Patch);
    }

    #[test_log::test]
    fn test_route_head() {
        let route = Route::head("/test", |_req| Box::pin(async { Ok(HttpResponse::ok()) }));
        assert_eq!(route.method, Method::Head);
    }

    // ==================== HttpRequest Stub Tests ====================

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_cookies_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/test")
            .with_cookie("session_id", "abc123")
            .with_cookie("user_pref", "dark_mode");

        let req = HttpRequest::new(SimulationStub::new(sim_req));
        let cookies = req.cookies();

        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies.get("session_id"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user_pref"), Some(&"dark_mode".to_string()));
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_cookies_empty_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/test");
        let req = HttpRequest::new(SimulationStub::new(sim_req));
        let cookies = req.cookies();

        assert!(cookies.is_empty());
    }

    #[test_log::test]
    fn test_http_request_cookies_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);
        let cookies = req.cookies();

        assert!(cookies.is_empty());
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_remote_addr_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req =
            SimulationRequest::new(Method::Get, "/test").with_remote_addr("192.168.1.100:54321");

        let req = HttpRequest::new(SimulationStub::new(sim_req));
        let remote_addr = req.remote_addr();

        assert_eq!(remote_addr, Some("192.168.1.100:54321".to_string()));
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_remote_addr_none_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/test");
        let req = HttpRequest::new(SimulationStub::new(sim_req));
        let remote_addr = req.remote_addr();

        assert_eq!(remote_addr, None);
    }

    #[test_log::test]
    fn test_http_request_remote_addr_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);
        let remote_addr = req.remote_addr();

        assert_eq!(remote_addr, None);
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_header_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/test")
            .with_header("X-Custom-Header", "custom-value")
            .with_header("Authorization", "Bearer token");

        let req = HttpRequest::new(SimulationStub::new(sim_req));

        assert_eq!(req.header("X-Custom-Header"), Some("custom-value"));
        assert_eq!(req.header("Authorization"), Some("Bearer token"));
        assert_eq!(req.header("Non-Existent"), None);
    }

    #[test_log::test]
    fn test_http_request_header_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);

        assert_eq!(req.header("Any-Header"), None);
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_method_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        for method in [
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Delete,
            Method::Patch,
        ] {
            let sim_req = SimulationRequest::new(method, "/test");
            let req = HttpRequest::new(SimulationStub::new(sim_req));

            assert_eq!(req.method(), method);
        }
    }

    #[test_log::test]
    fn test_http_request_method_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);

        // Empty stub defaults to GET
        assert_eq!(req.method(), Method::Get);
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_path_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/api/v1/users/123");
        let req = HttpRequest::new(SimulationStub::new(sim_req));

        assert_eq!(req.path(), "/api/v1/users/123");
    }

    #[test_log::test]
    fn test_http_request_path_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);

        assert_eq!(req.path(), "");
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_query_string_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let sim_req = SimulationRequest::new(Method::Get, "/search")
            .with_query_string("q=rust&limit=10&sort=desc");
        let req = HttpRequest::new(SimulationStub::new(sim_req));

        assert_eq!(req.query_string(), "q=rust&limit=10&sort=desc");
    }

    #[test_log::test]
    fn test_http_request_query_string_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);

        assert_eq!(req.query_string(), "");
    }

    #[test_log::test]
    #[cfg(feature = "simulator")]
    fn test_http_request_body_from_simulator_stub() {
        use simulator::{SimulationRequest, SimulationStub};

        let body_content = r#"{"name": "test", "value": 42}"#;
        let sim_req = SimulationRequest::new(Method::Post, "/api/data").with_body(body_content);
        let req = HttpRequest::new(SimulationStub::new(sim_req));

        let body = req.body();
        assert!(body.is_some());
        assert_eq!(body.unwrap().as_ref(), body_content.as_bytes());
    }

    #[test_log::test]
    fn test_http_request_body_empty_stub() {
        let req = HttpRequest::new(EmptyRequest);

        assert!(req.body().is_none());
    }

    #[test_log::test]
    #[cfg(all(feature = "simulator", feature = "serde"))]
    fn test_http_request_parse_query() {
        use serde::Deserialize;
        use simulator::{SimulationRequest, SimulationStub};

        #[derive(Debug, Deserialize, PartialEq)]
        struct QueryParams {
            page: u32,
            limit: Option<u32>,
            sort: Option<String>,
        }

        let sim_req = SimulationRequest::new(Method::Get, "/items")
            .with_query_string("page=5&limit=20&sort=name");
        let req = HttpRequest::new(SimulationStub::new(sim_req));

        let params: QueryParams = req.parse_query().unwrap();

        assert_eq!(params.page, 5);
        assert_eq!(params.limit, Some(20));
        assert_eq!(params.sort, Some("name".to_string()));
    }

    #[test_log::test]
    #[cfg(all(feature = "simulator", feature = "serde"))]
    fn test_http_request_parse_query_optional_fields() {
        use serde::Deserialize;
        use simulator::{SimulationRequest, SimulationStub};

        #[derive(Debug, Deserialize, PartialEq)]
        struct QueryParams {
            page: Option<u32>,
            limit: Option<u32>,
        }

        let sim_req = SimulationRequest::new(Method::Get, "/items").with_query_string("");
        let req = HttpRequest::new(SimulationStub::new(sim_req));

        let params: QueryParams = req.parse_query().unwrap();

        assert_eq!(params.page, None);
        assert_eq!(params.limit, None);
    }

    // ==================== HttpResponseBody Cow Conversion Tests ====================

    #[test_log::test]
    fn test_http_response_body_from_cow_borrowed() {
        use std::borrow::Cow;

        let borrowed_data: &[u8] = b"borrowed data";
        let cow: Cow<'_, [u8]> = Cow::Borrowed(borrowed_data);
        let body = HttpResponseBody::from(cow);

        match body {
            HttpResponseBody::Bytes(bytes) => {
                assert_eq!(bytes.as_ref(), b"borrowed data");
            }
        }
    }

    #[test_log::test]
    fn test_http_response_body_from_cow_owned() {
        use std::borrow::Cow;

        let owned_data = b"owned data".to_vec();
        let cow: Cow<'_, [u8]> = Cow::Owned(owned_data);
        let body = HttpResponseBody::from(cow);

        match body {
            HttpResponseBody::Bytes(bytes) => {
                assert_eq!(bytes.as_ref(), b"owned data");
            }
        }
    }

    // ==================== Error Status Code Tests ====================

    #[test_log::test]
    fn test_error_from_http_status_code_u16_various_codes() {
        #[derive(Debug)]
        struct TestError;
        impl std::fmt::Display for TestError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Test error")
            }
        }
        impl std::error::Error for TestError {}

        // Test various HTTP status codes
        let test_cases = [
            (200, StatusCode::Ok),
            (201, StatusCode::Created),
            (204, StatusCode::NoContent),
            (400, StatusCode::BadRequest),
            (401, StatusCode::Unauthorized),
            (403, StatusCode::Forbidden),
            (404, StatusCode::NotFound),
            (500, StatusCode::InternalServerError),
            (502, StatusCode::BadGateway),
            (503, StatusCode::ServiceUnavailable),
        ];

        for (code, expected_status) in test_cases {
            let err = Error::from_http_status_code_u16(code, TestError);
            match err {
                Error::Http { status_code, .. } => {
                    assert_eq!(status_code, expected_status, "Failed for code {code}");
                }
            }
        }
    }
}
