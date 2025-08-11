use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use moosicbox_web_server_core::WebServer;
use switchy_http_models::Method;

use crate::{RouteHandler, WebServerBuilder};

/// Simulation-specific implementation of HTTP response data
#[derive(Debug, Clone)]
pub struct SimulationResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

impl SimulationResponse {
    #[must_use]
    pub const fn new(status: u16) -> Self {
        Self {
            status,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    #[must_use]
    pub const fn ok() -> Self {
        Self::new(200)
    }

    #[must_use]
    pub const fn not_found() -> Self {
        Self::new(404)
    }

    #[must_use]
    pub const fn internal_server_error() -> Self {
        Self::new(500)
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

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
    #[must_use]
    pub const fn new(segments: Vec<PathSegment>) -> Self {
        Self { segments }
    }

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

/// Type alias for path parameters extracted from route matching
pub type PathParams = BTreeMap<String, String>;

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
/// This is a basic conversion that will be enhanced in Section 5.1.5
fn convert_http_response_to_simulation_response(
    http_response: crate::HttpResponse,
) -> SimulationResponse {
    let status = match http_response.status_code {
        switchy_http_models::StatusCode::NotFound => 404,
        switchy_http_models::StatusCode::InternalServerError => 500,
        switchy_http_models::StatusCode::BadRequest => 400,
        switchy_http_models::StatusCode::Unauthorized => 401,
        switchy_http_models::StatusCode::Forbidden => 403,
        switchy_http_models::StatusCode::TemporaryRedirect => 307,
        switchy_http_models::StatusCode::PermanentRedirect => 308,
        // Default to OK for all other status codes (including Ok)
        _ => 200,
    };

    let mut response = SimulationResponse::new(status);

    // Handle body conversion
    if let Some(body) = http_response.body {
        let body_string = match body {
            crate::HttpResponseBody::Bytes(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        };
        response = response.with_body(body_string);
    }

    // Handle location header for redirects
    if let Some(location) = http_response.location {
        response = response.with_header("Location", location);
    }

    response
}

/// Simulation-specific implementation of HTTP request data
#[derive(Debug, Clone)]
pub struct SimulationRequest {
    pub method: Method,
    pub path: String,
    pub query_string: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Bytes>,
    pub cookies: BTreeMap<String, String>,
    pub remote_addr: Option<String>,
    pub path_params: PathParams,
}

impl SimulationRequest {
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

    #[must_use]
    pub fn with_query_string(mut self, query: impl Into<String>) -> Self {
        self.query_string = query.into();
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    #[must_use]
    pub fn with_cookies(mut self, cookies: impl IntoIterator<Item = (String, String)>) -> Self {
        self.cookies.extend(cookies);
        self
    }

    #[must_use]
    pub fn with_cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    #[must_use]
    pub fn with_path_params(mut self, params: PathParams) -> Self {
        self.path_params = params;
        self
    }
}

/// Enhanced Stub that can hold simulation data
#[derive(Debug, Clone)]
pub struct SimulationStub {
    pub request: SimulationRequest,
    /// State container for the simulation
    pub state_container: Option<Arc<crate::extractors::state::StateContainer>>,
}

impl SimulationStub {
    #[must_use]
    pub const fn new(request: SimulationRequest) -> Self {
        Self {
            request,
            state_container: None,
        }
    }

    #[must_use]
    pub fn with_state_container(
        mut self,
        container: Arc<crate::extractors::state::StateContainer>,
    ) -> Self {
        self.state_container = Some(container);
        self
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.request.headers.get(name).map(String::as_str)
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.request.path
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        &self.request.query_string
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.request.method
    }

    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        self.request.body.as_ref()
    }

    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.request.cookies.get(name).map(String::as_str)
    }

    #[must_use]
    pub const fn cookies(&self) -> &BTreeMap<String, String> {
        &self.request.cookies
    }

    #[must_use]
    pub fn remote_addr(&self) -> Option<&str> {
        self.request.remote_addr.as_deref()
    }

    /// Get state of type T from the state container
    #[must_use]
    pub fn state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.state_container
            .as_ref()
            .and_then(|container| container.get::<T>())
    }

    /// Get a path parameter by name
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.request.path_params.get(name).map(String::as_str)
    }
}

impl From<SimulationRequest> for SimulationStub {
    fn from(request: SimulationRequest) -> Self {
        Self::new(request)
    }
}

struct SimulatorWebServer {
    scopes: Vec<crate::Scope>,
    routes: BTreeMap<(Method, String), RouteHandler>,
    #[allow(unused)] // TODO: Remove in 5.1.6 when state management methods are implemented
    state: Arc<RwLock<BTreeMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl SimulatorWebServer {
    #[allow(unused)] // TODO: Remove in 5.1.7 when register_scope() calls this method
    pub fn register_route(&mut self, method: Method, path: &str, handler: RouteHandler) {
        self.routes.insert((method, path.to_string()), handler);
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

        // Create HttpRequest::Stub from enhanced request
        let simulation_stub = SimulationStub::new(request);
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
    #[must_use]
    pub fn build_simulator(self) -> Box<dyn WebServer> {
        Box::new(SimulatorWebServer {
            scopes: self.scopes,
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
            state: Arc::new(RwLock::new(BTreeMap::new())),
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
}
