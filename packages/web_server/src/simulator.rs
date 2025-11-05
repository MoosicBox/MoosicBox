//! Simulator backend for deterministic testing.
//!
//! This module provides a lightweight, in-memory HTTP server simulator that can be used
//! for testing without starting an actual HTTP server. It implements the same interfaces
//! as the Actix backend but runs entirely in-process.
//!
//! # Overview
//!
//! The simulator backend includes:
//!
//! * [`SimulatorWebServer`] - In-memory web server for testing
//! * [`SimulationRequest`] / [`SimulationResponse`] - Request/response types
//! * [`SimulationStub`] - Enhanced request stub with state support
//! * Path pattern matching with parameter extraction
//! * Application state management
//!
//! # Example
//!
//! ```rust
//! use moosicbox_web_server::simulator::{SimulatorWebServer, SimulationRequest};
//! use moosicbox_web_server::{Scope, Route, Method, HttpResponse};
//!
//! # async fn example() {
//! // Create a simulator server with routes
//! let server = SimulatorWebServer::with_test_routes();
//!
//! // Process a simulated request
//! let request = SimulationRequest::new(Method::Get, "/test");
//! let response = server.process_request(request).await;
//!
//! assert_eq!(response.status, 200);
//! # }
//! ```
//!
//! # Path Patterns
//!
//! The simulator supports parameterized routes using `{param}` syntax:
//!
//! ```rust
//! use moosicbox_web_server::simulator::{parse_path_pattern, match_path};
//!
//! let pattern = parse_path_pattern("/users/{id}/posts/{post_id}");
//! let params = match_path(&pattern, "/users/123/posts/456").unwrap();
//!
//! assert_eq!(params.get("id"), Some(&"123".to_string()));
//! assert_eq!(params.get("post_id"), Some(&"456".to_string()));
//! ```

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use moosicbox_web_server_core::WebServer;
use switchy_http_models::Method;

use crate::{PathParams, RouteHandler, WebServerBuilder};

/// Simulation-specific implementation of HTTP response data
#[derive(Debug, Clone)]
pub struct SimulationResponse {
    /// HTTP status code (e.g., 200, 404, 500)
    pub status: u16,
    /// Response headers as key-value pairs
    pub headers: BTreeMap<String, String>,
    /// Optional response body as a string
    pub body: Option<String>,
}

impl SimulationResponse {
    /// Create a new response with the specified status code
    #[must_use]
    pub const fn new(status: u16) -> Self {
        Self {
            status,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Create a new 200 OK response
    #[must_use]
    pub const fn ok() -> Self {
        Self::new(200)
    }

    /// Create a new 404 Not Found response
    #[must_use]
    pub const fn not_found() -> Self {
        Self::new(404)
    }

    /// Create a new 500 Internal Server Error response
    #[must_use]
    pub const fn internal_server_error() -> Self {
        Self::new(500)
    }

    /// Add a header to the response
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Set the response body
    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// Represents a segment in a URL path pattern
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathSegment {
    /// A literal path segment (e.g., "users" in "/users/profile")
    Literal(String),
    /// A parameter segment (e.g., "id" in "/users/{id}")
    Parameter(String),
}

/// Represents a parsed path pattern for route matching
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathPattern {
    segments: Vec<PathSegment>,
}

impl PathPattern {
    /// Create a new path pattern from the given segments
    #[must_use]
    pub const fn new(segments: Vec<PathSegment>) -> Self {
        Self { segments }
    }

    /// Get the segments of this path pattern
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }
}

/// Parses a path pattern string into a `PathPattern`
///
/// Supports both literal segments and parameter segments using `{param}` syntax.
///
/// # Examples
///
/// ```
/// use moosicbox_web_server::simulator::{parse_path_pattern, PathSegment};
///
/// let pattern = parse_path_pattern("/users/{id}/profile");
/// assert_eq!(pattern.segments().len(), 3);
/// assert_eq!(pattern.segments()[0], PathSegment::Literal("users".to_string()));
/// assert_eq!(pattern.segments()[1], PathSegment::Parameter("id".to_string()));
/// assert_eq!(pattern.segments()[2], PathSegment::Literal("profile".to_string()));
/// ```
#[must_use]
pub fn parse_path_pattern(path: &str) -> PathPattern {
    let path = path.strip_prefix('/').unwrap_or(path);

    if path.is_empty() {
        return PathPattern::new(Vec::new());
    }

    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.starts_with('{') && segment.ends_with('}') {
                let param_name = &segment[1..segment.len() - 1];
                PathSegment::Parameter(param_name.to_string())
            } else {
                PathSegment::Literal(segment.to_string())
            }
        })
        .collect();

    PathPattern::new(segments)
}

/// Matches a path pattern against an actual request path
///
/// Returns `Some(PathParams)` if the path matches the pattern, with any extracted parameters.
/// Returns `None` if the path does not match the pattern.
///
/// # Examples
///
/// ```
/// use moosicbox_web_server::simulator::{parse_path_pattern, match_path};
///
/// // Exact match
/// let pattern = parse_path_pattern("/users/profile");
/// let params = match_path(&pattern, "/users/profile").unwrap();
/// assert!(params.is_empty());
///
/// // Parameter extraction
/// let pattern = parse_path_pattern("/users/{id}");
/// let params = match_path(&pattern, "/users/123").unwrap();
/// assert_eq!(params.get("id"), Some(&"123".to_string()));
/// ```
#[must_use]
pub fn match_path(pattern: &PathPattern, actual_path: &str) -> Option<PathParams> {
    let actual_pattern = parse_path_pattern(actual_path);
    let actual_segments = actual_pattern.segments();
    let pattern_segments = pattern.segments();

    // Must have same number of segments
    if actual_segments.len() != pattern_segments.len() {
        return None;
    }

    let mut params = PathParams::new();

    for (pattern_segment, actual_segment) in pattern_segments.iter().zip(actual_segments.iter()) {
        match (pattern_segment, actual_segment) {
            // Both are literals - must match exactly
            (PathSegment::Literal(pattern_lit), PathSegment::Literal(actual_lit)) => {
                if pattern_lit != actual_lit {
                    return None;
                }
            }
            // Pattern has parameter, actual has literal - extract parameter
            (PathSegment::Parameter(param_name), PathSegment::Literal(actual_value)) => {
                params.insert(param_name.clone(), actual_value.clone());
            }
            // Pattern has literal, actual has parameter - no match
            // Both are parameters - this shouldn't happen in normal usage
            (PathSegment::Literal(_) | PathSegment::Parameter(_), PathSegment::Parameter(_)) => {
                return None;
            }
        }
    }

    Some(params)
}

/// Converts an `HttpResponse` to a `SimulationResponse`
///
/// Enhanced conversion that preserves all headers and handles different body types.
/// Implemented as part of Section 5.1.5.2.
fn convert_http_response_to_simulation_response(
    http_response: crate::HttpResponse,
) -> SimulationResponse {
    // Map status code to u16
    let status = status_code_to_u16(http_response.status_code);

    // Create response with direct header copy (no inference needed!)
    let mut response = SimulationResponse {
        status,
        headers: http_response.headers, // Direct BTreeMap copy
        body: None,
    };

    // Handle body conversion
    if let Some(body) = http_response.body {
        let body_string = match body {
            crate::HttpResponseBody::Bytes(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        };
        response.body = Some(body_string);
    }

    // Keep backwards compatibility with location field
    if let Some(location) = http_response.location {
        response.headers.insert("Location".to_string(), location);
    }

    response
}

/// Maps `StatusCode` enum to u16 for `SimulationResponse`
const fn status_code_to_u16(status_code: switchy_http_models::StatusCode) -> u16 {
    match status_code {
        switchy_http_models::StatusCode::Ok => 200,
        switchy_http_models::StatusCode::Created => 201,
        switchy_http_models::StatusCode::Accepted => 202,
        switchy_http_models::StatusCode::NoContent => 204,
        switchy_http_models::StatusCode::MovedPermanently => 301,
        switchy_http_models::StatusCode::Found => 302,
        switchy_http_models::StatusCode::SeeOther => 303,
        switchy_http_models::StatusCode::NotModified => 304,
        switchy_http_models::StatusCode::TemporaryRedirect => 307,
        switchy_http_models::StatusCode::PermanentRedirect => 308,
        switchy_http_models::StatusCode::BadRequest => 400,
        switchy_http_models::StatusCode::Unauthorized => 401,
        switchy_http_models::StatusCode::PaymentRequired => 402,
        switchy_http_models::StatusCode::Forbidden => 403,
        switchy_http_models::StatusCode::NotFound => 404,
        switchy_http_models::StatusCode::MethodNotAllowed => 405,
        switchy_http_models::StatusCode::NotAcceptable => 406,
        switchy_http_models::StatusCode::ProxyAuthenticationRequired => 407,
        switchy_http_models::StatusCode::RequestTimeout => 408,
        switchy_http_models::StatusCode::Conflict => 409,
        switchy_http_models::StatusCode::Gone => 410,
        switchy_http_models::StatusCode::LengthRequired => 411,
        switchy_http_models::StatusCode::PreconditionFailed => 412,
        switchy_http_models::StatusCode::ContentTooLarge => 413,
        switchy_http_models::StatusCode::URITooLong => 414,
        switchy_http_models::StatusCode::UnsupportedMediaType => 415,
        switchy_http_models::StatusCode::RangeNotSatisfiable => 416,
        switchy_http_models::StatusCode::ExpectationFailed => 417,
        switchy_http_models::StatusCode::ImATeapot => 418,
        switchy_http_models::StatusCode::MisdirectedRequest => 421,
        switchy_http_models::StatusCode::UncompressableContent => 422,
        switchy_http_models::StatusCode::Locked => 423,
        switchy_http_models::StatusCode::FailedDependency => 424,
        switchy_http_models::StatusCode::UpgradeRequired => 426,
        switchy_http_models::StatusCode::PreconditionRequired => 428,
        switchy_http_models::StatusCode::TooManyRequests => 429,
        switchy_http_models::StatusCode::RequestHeaderFieldsTooLarge => 431,
        switchy_http_models::StatusCode::UnavailableForLegalReasons => 451,
        switchy_http_models::StatusCode::NotImplemented => 501,
        switchy_http_models::StatusCode::BadGateway => 502,
        switchy_http_models::StatusCode::ServiceUnavailable => 503,
        switchy_http_models::StatusCode::GatewayTimeout => 504,
        switchy_http_models::StatusCode::HTTPVersionNotSupported => 505,
        switchy_http_models::StatusCode::VariantAlsoNegotiates => 506,
        switchy_http_models::StatusCode::InsufficientStorage => 507,
        switchy_http_models::StatusCode::LoopDetected => 508,
        switchy_http_models::StatusCode::NotExtended => 510,
        switchy_http_models::StatusCode::NetworkAuthenticationRequired => 511,
        // Handle any other status codes (including InternalServerError)
        _ => 500, // Default to Internal Server Error
    }
}

/// Simulation-specific implementation of HTTP request data
#[derive(Debug, Clone)]
pub struct SimulationRequest {
    /// HTTP method (GET, POST, etc.)
    pub method: Method,
    /// Request path (e.g., `/api/users`)
    pub path: String,
    /// Query string (e.g., `?page=1&limit=20`)
    pub query_string: String,
    /// Request headers as key-value pairs
    pub headers: BTreeMap<String, String>,
    /// Optional request body
    pub body: Option<Bytes>,
    /// Cookies as key-value pairs
    pub cookies: BTreeMap<String, String>,
    /// Optional remote address of the client
    pub remote_addr: Option<String>,
    /// Path parameters extracted from the route pattern
    pub path_params: PathParams,
}

impl SimulationRequest {
    /// Create a new simulation request with the given method and path
    #[must_use]
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query_string: String::new(),
            headers: BTreeMap::new(),
            body: None,
            cookies: BTreeMap::new(),
            remote_addr: None,
            path_params: PathParams::new(),
        }
    }

    /// Set the query string for this request
    #[must_use]
    pub fn with_query_string(mut self, query: impl Into<String>) -> Self {
        self.query_string = query.into();
        self
    }

    /// Add a header to this request
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Set the body for this request
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Add multiple cookies to this request
    #[must_use]
    pub fn with_cookies(mut self, cookies: impl IntoIterator<Item = (String, String)>) -> Self {
        self.cookies.extend(cookies);
        self
    }

    /// Add a single cookie to this request
    #[must_use]
    pub fn with_cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.insert(name.into(), value.into());
        self
    }

    /// Set the remote address for this request
    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    /// Set the path parameters for this request
    #[must_use]
    pub fn with_path_params(mut self, params: PathParams) -> Self {
        self.path_params = params;
        self
    }
}

/// Enhanced Stub that can hold simulation data
#[derive(Debug, Clone)]
pub struct SimulationStub {
    /// The simulation request being processed
    pub request: SimulationRequest,
    /// State container for the simulation
    pub state_container: Option<Arc<RwLock<crate::extractors::state::StateContainer>>>,
}

impl SimulationStub {
    /// Create a new simulation stub from the given request
    #[must_use]
    pub const fn new(request: SimulationRequest) -> Self {
        Self {
            request,
            state_container: None,
        }
    }

    /// Attach a state container to this stub
    #[must_use]
    pub fn with_state_container(
        mut self,
        container: Arc<RwLock<crate::extractors::state::StateContainer>>,
    ) -> Self {
        self.state_container = Some(container);
        self
    }

    /// Get a header value by name
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.request.headers.get(name).map(String::as_str)
    }

    /// Get the request path
    #[must_use]
    pub fn path(&self) -> &str {
        &self.request.path
    }

    /// Get the query string
    #[must_use]
    pub fn query_string(&self) -> &str {
        &self.request.query_string
    }

    /// Get the HTTP method
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.request.method
    }

    /// Get the request body
    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        self.request.body.as_ref()
    }

    /// Get a cookie value by name
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.request.cookies.get(name).map(String::as_str)
    }

    /// Get all cookies
    #[must_use]
    pub const fn cookies(&self) -> &BTreeMap<String, String> {
        &self.request.cookies
    }

    /// Get the remote address of the client
    #[must_use]
    pub fn remote_addr(&self) -> Option<&str> {
        self.request.remote_addr.as_deref()
    }

    /// Get state of type T from the state container
    #[must_use]
    pub fn state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.state_container.as_ref().and_then(|container| {
            container
                .read()
                .map_or_else(|_| None, |state| state.get::<T>())
        })
    }

    /// Get a path parameter by name
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.request.path_params.get(name).map(String::as_str)
    }

    /// Access application state from the server
    ///
    /// This method provides access to the server's state container, which can be used
    /// by extractors like `State<T>` to retrieve typed state values.
    ///
    /// Returns `None` if no state container has been set on this stub.
    #[must_use]
    pub const fn app_state(
        &self,
    ) -> Option<&Arc<RwLock<crate::extractors::state::StateContainer>>> {
        self.state_container.as_ref()
    }
}

impl From<SimulationRequest> for SimulationStub {
    fn from(request: SimulationRequest) -> Self {
        Self::new(request)
    }
}

/// In-memory web server for deterministic testing and simulation.
///
/// This struct provides a lightweight, in-process HTTP server simulator that can be used
/// for testing without starting an actual HTTP server. It implements the [`WebServer`]
/// trait and stores routes, scopes, and application state for request processing.
///
/// # Structure
///
/// * `scopes` - All registered scopes with their routes
/// * `routes` - Flattened route map for efficient lookup
/// * `state` - Shared application state accessible via extractors
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::simulator::SimulatorWebServer;
/// use moosicbox_web_server::{Scope, Method, HttpResponse};
///
/// # async fn example() {
/// let server = SimulatorWebServer::with_test_routes();
/// // Use server for testing
/// # }
/// ```
pub struct SimulatorWebServer {
    /// Registered scopes containing routes and nested scopes
    pub scopes: Vec<crate::Scope>,
    /// Flat map of registered routes with their handlers
    pub routes: BTreeMap<(Method, String), RouteHandler>,
    /// Application state container for extractors
    pub state: Arc<RwLock<crate::extractors::state::StateContainer>>,
}

impl SimulatorWebServer {
    /// Register a single route with the simulator
    #[allow(dead_code)] // Used in tests and scope processing
    pub fn register_route(&mut self, method: Method, path: &str, handler: RouteHandler) {
        self.routes.insert((method, path.to_string()), handler);
    }

    /// Register a scope and all its routes and nested scopes
    ///
    /// This method processes a scope recursively, registering all routes with their
    /// full paths (including scope prefixes) and handling nested scopes.
    ///
    /// # Arguments
    ///
    /// * `scope` - The scope to register
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut server = SimulatorWebServer::new();
    /// let scope = Scope::new("/api")
    ///     .route(Method::Get, "/users", handler);
    /// server.register_scope(scope);
    /// // This registers a route at "/api/users"
    /// ```
    #[allow(dead_code)] // Used in tests
    pub fn register_scope(&mut self, scope: &crate::Scope) {
        self.process_scope_recursive(scope, "");
    }

    /// Recursively process a scope and register all its routes
    ///
    /// This helper method handles the recursive processing of scopes, building
    /// the full path by combining parent prefixes with the current scope path.
    ///
    /// # Arguments
    ///
    /// * `scope` - The scope to process
    /// * `parent_prefix` - The accumulated path prefix from parent scopes
    fn process_scope_recursive(&mut self, scope: &crate::Scope, parent_prefix: &str) {
        // Build the full prefix for this scope
        let full_prefix = if parent_prefix.is_empty() {
            scope.path.clone()
        } else {
            format!("{}{}", parent_prefix, scope.path)
        };

        // Register all routes in this scope with the full prefix
        for route in &scope.routes {
            let full_path = if full_prefix.is_empty() {
                route.path.clone()
            } else {
                format!("{}{}", full_prefix, route.path)
            };

            // Clone the Arc<RouteHandler> and extract the inner RouteHandler
            let handler_arc = Arc::clone(&route.handler);
            // We need to create a new Box that wraps the Arc-ed handler
            let handler: RouteHandler = Box::new(move |req| {
                let handler_arc = Arc::clone(&handler_arc);
                handler_arc(req)
            });

            self.register_route(route.method, &full_path, handler);
        }

        // Recursively process nested scopes
        for nested_scope in &scope.scopes {
            self.process_scope_recursive(nested_scope, &full_prefix);
        }
    }

    /// Finds a route that matches the given method and path
    ///
    /// Returns the handler and extracted path parameters if a match is found.
    /// Implements route precedence: exact matches are preferred over parameterized matches.
    #[allow(unused)] // TODO: Remove in 5.1.4 when process_request() uses this method
    #[must_use]
    pub fn find_route(&self, method: Method, path: &str) -> Option<(&RouteHandler, PathParams)> {
        let mut exact_matches = Vec::new();
        let mut parameterized_matches = Vec::new();

        // Collect all potential matches
        for ((route_method, route_path), handler) in &self.routes {
            if *route_method != method {
                continue;
            }

            let route_pattern = parse_path_pattern(route_path);
            if let Some(params) = match_path(&route_pattern, path) {
                // Check if this is an exact match (no parameters extracted)
                if params.is_empty() {
                    exact_matches.push((handler, params));
                } else {
                    parameterized_matches.push((handler, params));
                }
            }
        }

        // Prefer exact matches over parameterized matches
        if let Some((handler, params)) = exact_matches.into_iter().next() {
            Some((handler, params))
        } else {
            parameterized_matches.into_iter().next()
        }
    }

    /// Processes a simulation request and returns a simulation response
    ///
    /// This method implements the complete request processing pipeline:
    /// 1. Find matching route using `find_route()`
    /// 2. Inject path parameters into request
    /// 3. Create `HttpRequest::Stub` from enhanced request
    /// 4. Execute matched handler with request
    /// 5. Convert `HttpResponse` to `SimulationResponse`
    /// 6. Return 404 response if no route matches
    #[allow(unused)] // TODO: Remove in 5.1.4 integration tests when this method is called
    pub async fn process_request(&self, mut request: SimulationRequest) -> SimulationResponse {
        // Find matching route using find_route()
        let route_result = self.find_route(request.method, &request.path);

        let Some((handler, path_params)) = route_result else {
            // Return 404 response if no route matches
            return SimulationResponse::not_found().with_body("Not Found");
        };

        // Inject path params into request
        request.path_params = path_params;

        // Create HttpRequest::Stub from enhanced request with state container
        let simulation_stub =
            SimulationStub::new(request).with_state_container(Arc::clone(&self.state));

        let http_request = crate::HttpRequest::Stub(crate::Stub::Simulator(simulation_stub));

        // Execute matched handler with request
        handler(http_request).await.map_or_else(
            |_| {
                // Return 500 response on handler error
                SimulationResponse::internal_server_error().with_body("Internal Server Error")
            },
            |http_response| {
                // Convert HttpResponse to SimulationResponse
                convert_http_response_to_simulation_response(http_response)
            },
        )
    }

    /// Insert state of type T into the server's state container
    ///
    /// This state can later be retrieved by handlers using the `State<T>` extractor.
    /// The state is stored in a thread-safe manner and can be accessed concurrently.
    ///
    /// # Arguments
    ///
    /// * `state` - The state value to store
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // SimulatorWebServer is an internal implementation detail
    /// let mut server = SimulatorWebServer::new();
    /// server.insert_state("Hello, World!".to_string());
    /// ```
    #[allow(dead_code)] // Used in tests
    pub fn insert_state<T: Send + Sync + 'static>(&self, state: T) {
        if let Ok(mut state_container) = self.state.write() {
            state_container.insert(state);
        }
    }

    /// Retrieve state of type T from the server's state container
    ///
    /// Returns `Some(Arc<T>)` if state of the requested type exists,
    /// or `None` if no state of that type has been inserted.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // SimulatorWebServer is an internal implementation detail
    /// let mut server = SimulatorWebServer::new();
    /// server.insert_state("Hello, World!".to_string());
    ///
    /// let state: Option<std::sync::Arc<String>> = server.get_state();
    /// assert!(state.is_some());
    /// ```
    #[must_use]
    #[allow(dead_code)] // Used in tests
    pub fn get_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.state
            .read()
            .map_or_else(|_| None, |state_container| state_container.get::<T>())
    }

    /// Create a new simulator web server with the given scopes
    #[must_use]
    pub fn new(scopes: Vec<crate::Scope>) -> Self {
        Self {
            scopes,
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        }
    }

    /// Create a server with test routes
    #[must_use]
    pub fn with_test_routes() -> Self {
        let mut server = Self::new(Vec::new());

        // Register test routes similar to Actix implementation
        server.register_route(
            Method::Get,
            "/test",
            Box::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_header("content-type", "application/json")
                        .with_body(r#"{"message":"Hello from test route!"}"#))
                })
            }),
        );

        server.register_route(
            Method::Get,
            "/health",
            Box::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_header("content-type", "application/json")
                        .with_body(r#"{"status":"ok"}"#))
                })
            }),
        );

        server
    }

    /// Create a server with API routes
    #[must_use]
    pub fn with_api_routes() -> Self {
        let mut server = Self::new(Vec::new());

        // Register API routes similar to Actix implementation
        server.register_route(
            Method::Get,
            "/api/status",
            Box::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_header("content-type", "application/json")
                        .with_body(r#"{"service":"running"}"#))
                })
            }),
        );

        server.register_route(
            Method::Post,
            "/api/echo",
            Box::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_header("content-type", "application/json")
                        .with_body(r#"{"echoed":"data"}"#))
                })
            }),
        );

        server
    }
}

/// Error type for simulator web server operations
#[derive(Debug, thiserror::Error)]
pub enum SimulatorWebServerError {
    /// Server startup error
    #[error("Server startup failed: {0}")]
    Startup(String),
    /// Server shutdown error
    #[error("Server shutdown failed: {0}")]
    Shutdown(String),
}

impl crate::test_client::GenericTestServer for SimulatorWebServer {
    type Error = SimulatorWebServerError;

    fn url(&self) -> String {
        // Simulator doesn't have a real URL, so return a placeholder
        "http://simulator".to_string()
    }

    fn port(&self) -> u16 {
        // Simulator doesn't use a real port, so return a placeholder
        8080
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        // Simulator doesn't need to start, so this is a no-op
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        // Simulator doesn't need to stop, so this is a no-op
        Ok(())
    }
}

impl WebServer for SimulatorWebServer {
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let scopes = self.scopes.clone();
        Box::pin(async move {
            log::info!("Simulator web server started with {} scopes", scopes.len());
            for scope in &scopes {
                log::debug!("Scope '{}' has {} routes", scope.path, scope.routes.len());
                for route in &scope.routes {
                    log::debug!("  {:?} {}{}", route.method, scope.path, route.path);
                }
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async {
            log::info!("Simulator web server stopped");
        })
    }
}

impl WebServerBuilder {
    /// Build a simulator web server instance
    #[must_use]
    pub fn build_simulator(self) -> Box<dyn WebServer> {
        Box::new(SimulatorWebServer {
            scopes: self.scopes,
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpRequest, HttpResponse};

    fn create_test_handler() -> RouteHandler {
        Box::new(|_req: HttpRequest| {
            Box::pin(async move { Ok(HttpResponse::ok().with_body("test response")) })
        })
    }

    #[test]
    fn test_route_registration_stores_handler_correctly() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/test", handler);

        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/test".to_string()))
        );
        assert_eq!(server.routes.len(), 1);
    }

    #[test]
    fn test_multiple_routes_can_be_registered_without_conflict() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler1 = create_test_handler();
        let handler2 = create_test_handler();
        let handler3 = create_test_handler();

        server.register_route(Method::Get, "/users", handler1);
        server.register_route(Method::Post, "/users", handler2);
        server.register_route(Method::Get, "/posts", handler3);

        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Post, "/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/posts".to_string()))
        );
        assert_eq!(server.routes.len(), 3);
    }

    #[test]
    fn test_parse_literal_path_pattern() {
        let pattern = parse_path_pattern("/users/profile");

        assert_eq!(pattern.segments().len(), 2);
        assert_eq!(
            pattern.segments()[0],
            PathSegment::Literal("users".to_string())
        );
        assert_eq!(
            pattern.segments()[1],
            PathSegment::Literal("profile".to_string())
        );
    }

    #[test]
    fn test_parse_parameterized_path_pattern() {
        let pattern = parse_path_pattern("/{id}");

        assert_eq!(pattern.segments().len(), 1);
        assert_eq!(
            pattern.segments()[0],
            PathSegment::Parameter("id".to_string())
        );
    }

    #[test]
    fn test_parse_mixed_literal_and_parameter_path_pattern() {
        let pattern = parse_path_pattern("/users/{id}/posts/{post_id}");

        assert_eq!(pattern.segments().len(), 4);
        assert_eq!(
            pattern.segments()[0],
            PathSegment::Literal("users".to_string())
        );
        assert_eq!(
            pattern.segments()[1],
            PathSegment::Parameter("id".to_string())
        );
        assert_eq!(
            pattern.segments()[2],
            PathSegment::Literal("posts".to_string())
        );
        assert_eq!(
            pattern.segments()[3],
            PathSegment::Parameter("post_id".to_string())
        );
    }

    #[test]
    fn test_parse_empty_path_pattern() {
        let pattern = parse_path_pattern("");
        assert_eq!(pattern.segments().len(), 0);

        let pattern = parse_path_pattern("/");
        assert_eq!(pattern.segments().len(), 0);
    }

    #[test]
    fn test_parse_path_pattern_without_leading_slash() {
        let pattern = parse_path_pattern("users/{id}");

        assert_eq!(pattern.segments().len(), 2);
        assert_eq!(
            pattern.segments()[0],
            PathSegment::Literal("users".to_string())
        );
        assert_eq!(
            pattern.segments()[1],
            PathSegment::Parameter("id".to_string())
        );
    }

    #[test]
    fn test_match_path_exact_route() {
        let pattern = parse_path_pattern("/api/users");
        let params = match_path(&pattern, "/api/users").unwrap();

        assert!(params.is_empty());
    }

    #[test]
    fn test_match_path_parameterized_route() {
        let pattern = parse_path_pattern("/users/{id}");
        let params = match_path(&pattern, "/users/123").unwrap();

        assert_eq!(params.len(), 1);
        assert_eq!(params.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_match_path_multiple_parameters() {
        let pattern = parse_path_pattern("/users/{id}/posts/{post_id}");
        let params = match_path(&pattern, "/users/123/posts/456").unwrap();

        assert_eq!(params.len(), 2);
        assert_eq!(params.get("id"), Some(&"123".to_string()));
        assert_eq!(params.get("post_id"), Some(&"456".to_string()));
    }

    #[test]
    fn test_match_path_no_match_different_segments() {
        let pattern = parse_path_pattern("/users/{id}");
        let result = match_path(&pattern, "/posts/123");

        assert!(result.is_none());
    }

    #[test]
    fn test_match_path_no_match_different_length() {
        let pattern = parse_path_pattern("/users/{id}");
        let result = match_path(&pattern, "/users/123/extra");

        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_exact_match() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/api/users", handler);

        let result = server.find_route(Method::Get, "/api/users");
        assert!(result.is_some());

        let (_, params) = result.unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_find_route_parameterized_match() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/users/{id}", handler);

        let result = server.find_route(Method::Get, "/users/123");
        assert!(result.is_some());

        let (_, params) = result.unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_find_route_method_discrimination() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let get_handler = create_test_handler();
        let post_handler = create_test_handler();
        server.register_route(Method::Get, "/users", get_handler);
        server.register_route(Method::Post, "/users", post_handler);

        // GET request should match GET route
        let get_result = server.find_route(Method::Get, "/users");
        assert!(get_result.is_some());

        // POST request should match POST route
        let post_result = server.find_route(Method::Post, "/users");
        assert!(post_result.is_some());

        // PUT request should not match any route
        let put_result = server.find_route(Method::Put, "/users");
        assert!(put_result.is_none());
    }

    #[test]
    fn test_find_route_no_match_404() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/users", handler);

        // Different path should not match
        let result = server.find_route(Method::Get, "/posts");
        assert!(result.is_none());

        // Different method should not match
        let result = server.find_route(Method::Post, "/users");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_precedence_exact_over_parameterized() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let exact_handler = create_test_handler();
        let param_handler = create_test_handler();

        // Register parameterized route first
        server.register_route(Method::Get, "/users/{id}", param_handler);
        // Register exact route second
        server.register_route(Method::Get, "/users/profile", exact_handler);

        // Request for "/users/profile" should match exact route (empty params)
        let result = server.find_route(Method::Get, "/users/profile");
        assert!(result.is_some());

        let (_, params) = result.unwrap();
        assert!(params.is_empty()); // Exact match should have no parameters
    }

    #[test]
    fn test_process_request_integration_setup() {
        // This test validates that the process_request method can be set up correctly
        // Full async integration tests will be added when tokio dependency is available
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/hello", handler);

        // Verify the route was registered
        let route_result = server.find_route(Method::Get, "/hello");
        assert!(route_result.is_some());

        // Verify 404 case
        let not_found_result = server.find_route(Method::Get, "/nonexistent");
        assert!(not_found_result.is_none());
    }

    #[test]
    fn test_simulation_response_builders() {
        let response = SimulationResponse::ok()
            .with_header("Content-Type", "application/json")
            .with_body("{}");

        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(response.body, Some("{}".to_string()));
    }

    #[test]
    fn test_simulation_request_with_path_params() {
        let mut params = PathParams::new();
        params.insert("id".to_string(), "123".to_string());

        let request = SimulationRequest::new(Method::Get, "/users/123").with_path_params(params);

        assert_eq!(request.path_params.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_simulation_stub_path_param() {
        let mut params = PathParams::new();
        params.insert("id".to_string(), "456".to_string());
        params.insert("name".to_string(), "john".to_string());

        let request = SimulationRequest::new(Method::Get, "/users/456").with_path_params(params);
        let stub = SimulationStub::new(request);

        assert_eq!(stub.path_param("id"), Some("456"));
        assert_eq!(stub.path_param("name"), Some("john"));
        assert_eq!(stub.path_param("nonexistent"), None);
    }

    // Response Generation Tests (Section 5.1.5.2)

    #[test]
    #[cfg(feature = "serde")]
    fn test_json_response_conversion_preserves_content_type() {
        use serde_json::json;

        let test_data = json!({"message": "Hello, World!", "status": "success"});
        let http_response = HttpResponse::json(&test_data).unwrap();

        let simulation_response = convert_http_response_to_simulation_response(http_response);

        assert_eq!(simulation_response.status, 200);
        assert_eq!(
            simulation_response.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert!(simulation_response.body.is_some());

        // Verify the JSON content is preserved
        let body = simulation_response.body.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["message"], "Hello, World!");
        assert_eq!(parsed["status"], "success");
    }

    #[test]
    fn test_status_codes_are_preserved() {
        // Test 200 OK
        let ok_response = HttpResponse::ok();
        let sim_response = convert_http_response_to_simulation_response(ok_response);
        assert_eq!(sim_response.status, 200);

        // Test 404 Not Found
        let not_found_response = HttpResponse::not_found();
        let sim_response = convert_http_response_to_simulation_response(not_found_response);
        assert_eq!(sim_response.status, 404);

        // Test 500 Internal Server Error
        let error_response =
            HttpResponse::from_status_code(switchy_http_models::StatusCode::InternalServerError);
        let sim_response = convert_http_response_to_simulation_response(error_response);
        assert_eq!(sim_response.status, 500);

        // Test 201 Created
        let created_response =
            HttpResponse::from_status_code(switchy_http_models::StatusCode::Created);
        let sim_response = convert_http_response_to_simulation_response(created_response);
        assert_eq!(sim_response.status, 201);

        // Test 401 Unauthorized
        let unauthorized_response =
            HttpResponse::from_status_code(switchy_http_models::StatusCode::Unauthorized);
        let sim_response = convert_http_response_to_simulation_response(unauthorized_response);
        assert_eq!(sim_response.status, 401);
    }

    #[test]
    fn test_custom_headers_are_preserved() {
        let http_response = HttpResponse::ok()
            .with_header("X-Custom-Header", "custom-value")
            .with_header("X-Another-Header", "another-value")
            .with_content_type("text/plain")
            .with_body("Hello, World!");

        let simulation_response = convert_http_response_to_simulation_response(http_response);

        assert_eq!(simulation_response.status, 200);
        assert_eq!(
            simulation_response.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
        assert_eq!(
            simulation_response.headers.get("X-Another-Header"),
            Some(&"another-value".to_string())
        );
        assert_eq!(
            simulation_response.headers.get("Content-Type"),
            Some(&"text/plain".to_string())
        );
        assert_eq!(simulation_response.body, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_html_response_conversion() {
        let html_content = "<h1>Hello, World!</h1><p>This is a test.</p>";
        let http_response = HttpResponse::html(html_content);

        let simulation_response = convert_http_response_to_simulation_response(http_response);

        assert_eq!(simulation_response.status, 200);
        assert_eq!(
            simulation_response.headers.get("Content-Type"),
            Some(&"text/html; charset=utf-8".to_string())
        );
        assert_eq!(simulation_response.body, Some(html_content.to_string()));
    }

    #[test]
    fn test_text_response_conversion() {
        let text_content = "This is plain text content.";
        let http_response = HttpResponse::text(text_content);

        let simulation_response = convert_http_response_to_simulation_response(http_response);

        assert_eq!(simulation_response.status, 200);
        assert_eq!(
            simulation_response.headers.get("Content-Type"),
            Some(&"text/plain; charset=utf-8".to_string())
        );
        assert_eq!(simulation_response.body, Some(text_content.to_string()));
    }

    #[test]
    fn test_location_header_backwards_compatibility() {
        let redirect_response =
            HttpResponse::temporary_redirect().with_location("https://example.com/new-location");

        let simulation_response = convert_http_response_to_simulation_response(redirect_response);

        assert_eq!(simulation_response.status, 307);
        assert_eq!(
            simulation_response.headers.get("Location"),
            Some(&"https://example.com/new-location".to_string())
        );
    }

    // State Management Tests (Section 5.1.6)

    #[test]
    fn test_simulator_state_management_string_state() {
        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Insert string state
        let test_string = "Hello, World!".to_string();
        server.insert_state(test_string.clone());

        // Retrieve string state
        let retrieved_state: Option<Arc<String>> = server.get_state();
        assert!(retrieved_state.is_some());
        assert_eq!(*retrieved_state.unwrap(), test_string);
    }

    #[test]
    fn test_simulator_state_management_custom_struct_state() {
        #[derive(Debug, Clone, PartialEq)]
        struct AppConfig {
            name: String,
            version: u32,
            debug: bool,
        }

        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Insert custom struct state
        let config = AppConfig {
            name: "TestApp".to_string(),
            version: 42,
            debug: true,
        };
        server.insert_state(config.clone());

        // Retrieve custom struct state
        let retrieved_config: Option<Arc<AppConfig>> = server.get_state();
        assert!(retrieved_config.is_some());
        assert_eq!(*retrieved_config.unwrap(), config);
    }

    #[test]
    fn test_simulator_state_management_multiple_types() {
        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Insert multiple different types
        server.insert_state("Hello".to_string());
        server.insert_state(42u32);
        server.insert_state(true);

        // Retrieve each type independently
        let string_state: Option<Arc<String>> = server.get_state();
        let u32_state: Option<Arc<u32>> = server.get_state();
        let bool_state: Option<Arc<bool>> = server.get_state();

        assert!(string_state.is_some());
        assert!(u32_state.is_some());
        assert!(bool_state.is_some());

        assert_eq!(*string_state.unwrap(), "Hello");
        assert_eq!(*u32_state.unwrap(), 42);
        assert!(*bool_state.unwrap());
    }

    #[test]
    fn test_simulator_state_management_shared_across_requests() {
        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Insert shared state
        let shared_data = "Shared across requests".to_string();
        server.insert_state(shared_data.clone());

        // Create multiple requests
        let request1 = SimulationRequest::new(Method::Get, "/test1");
        let request2 = SimulationRequest::new(Method::Get, "/test2");

        // Create simulation stubs with state container
        let stub1 = SimulationStub::new(request1).with_state_container(Arc::clone(&server.state));
        let stub2 = SimulationStub::new(request2).with_state_container(Arc::clone(&server.state));

        // Both stubs should access the same state
        let state1: Option<Arc<String>> = stub1.state();
        let state2: Option<Arc<String>> = stub2.state();

        assert!(state1.is_some());
        assert!(state2.is_some());
        assert_eq!(*state1.unwrap(), shared_data);
        assert_eq!(*state2.unwrap(), shared_data);

        // Verify they're actually the same Arc (same memory location)
        let state1_again: Option<Arc<String>> = stub1.state();
        let state2_again: Option<Arc<String>> = stub2.state();
        assert!(Arc::ptr_eq(&state1_again.unwrap(), &state2_again.unwrap()));
    }

    #[test]
    fn test_simulator_state_management_handler_extraction() {
        use crate::{extractors::state::State, from_request::FromRequest};

        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Insert state that will be extracted by handler
        let app_name = "TestApplication".to_string();
        server.insert_state(app_name.clone());

        // Create a request and simulation stub with state
        let request = SimulationRequest::new(Method::Get, "/app-info");
        let simulation_stub =
            SimulationStub::new(request).with_state_container(Arc::clone(&server.state));
        let http_request = crate::HttpRequest::Stub(crate::Stub::Simulator(simulation_stub));

        // Test that State<T> extractor works with the simulator backend
        let state_result: Result<State<String>, _> = State::from_request_sync(&http_request);

        assert!(state_result.is_ok());
        let State(extracted_name) = state_result.unwrap();
        assert_eq!(*extracted_name, app_name);
    }

    // Scope Processing Tests (Section 5.1.7)

    #[test]
    fn test_register_scope_with_single_route() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let scope = crate::Scope::new("/api").with_route(crate::Route::new(
            Method::Get,
            "/users",
            |_req: crate::HttpRequest| {
                Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("test response")) })
            },
        ));

        server.register_scope(&scope);

        // Verify the route was registered with the full path
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/api/users".to_string()))
        );
        assert_eq!(server.routes.len(), 1);
    }

    #[test]
    fn test_register_scope_with_multiple_routes() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let scope = crate::Scope::new("/api")
            .with_route(crate::Route::new(
                Method::Get,
                "/users",
                |_req: crate::HttpRequest| {
                    Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("get users")) })
                },
            ))
            .with_route(crate::Route::new(
                Method::Post,
                "/users",
                |_req: crate::HttpRequest| {
                    Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("create user")) })
                },
            ));

        server.register_scope(&scope);

        // Verify both routes were registered with the full path
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/api/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Post, "/api/users".to_string()))
        );
        assert_eq!(server.routes.len(), 2);
    }

    #[test]
    fn test_register_scope_with_nested_scopes() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let nested_scope = crate::Scope::new("/v1").with_route(crate::Route::new(
            Method::Get,
            "/users",
            |_req: crate::HttpRequest| {
                Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("v1 users")) })
            },
        ));

        let scope = crate::Scope::new("/api")
            .with_route(crate::Route::new(
                Method::Get,
                "/health",
                |_req: crate::HttpRequest| {
                    Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("healthy")) })
                },
            ))
            .with_scope(nested_scope)
            .with_route(crate::Route::new(
                Method::Post,
                "/auth",
                |_req: crate::HttpRequest| {
                    Box::pin(
                        async move { Ok(crate::HttpResponse::ok().with_body("authenticated")) },
                    )
                },
            ));

        server.register_scope(&scope);

        // Verify all routes were registered with correct full paths
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/api/health".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/api/v1/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Post, "/api/auth".to_string()))
        );
        assert_eq!(server.routes.len(), 3);
    }

    #[test]
    fn test_register_scope_with_empty_prefix() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        let scope = crate::Scope::new("").with_route(crate::Route::new(
            Method::Get,
            "/users",
            |_req: crate::HttpRequest| {
                Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("users")) })
            },
        ));

        server.register_scope(&scope);

        // Verify the route was registered without prefix
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/users".to_string()))
        );
        assert_eq!(server.routes.len(), 1);
    }

    #[test]
    fn test_register_scope_with_deeply_nested_scopes() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(crate::extractors::state::StateContainer::new())),
        };

        // Create deeply nested scopes: /api/v1/admin/users
        let admin_scope = crate::Scope::new("/admin").with_route(crate::Route::new(
            Method::Delete,
            "/users/{id}",
            |_req: crate::HttpRequest| {
                Box::pin(async move { Ok(crate::HttpResponse::ok().with_body("user deleted")) })
            },
        ));

        let v1_scope = crate::Scope::new("/v1").with_scope(admin_scope);
        let api_scope = crate::Scope::new("/api").with_scope(v1_scope);

        server.register_scope(&api_scope);

        // Verify the deeply nested route was registered correctly
        assert!(
            server
                .routes
                .contains_key(&(Method::Delete, "/api/v1/admin/users/{id}".to_string()))
        );
        assert_eq!(server.routes.len(), 1);
    }
}
