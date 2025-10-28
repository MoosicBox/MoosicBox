//! Web server simulator for testing HTTP interactions without a real server.
//!
//! This crate provides a lightweight simulation framework for testing web server
//! behavior in unit tests. It allows you to define routes, mock responses, and
//! verify request handling without starting an actual HTTP server.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use moosicbox_web_server_core::WebServer;
use serde::Serialize;
use switchy::unsync::sync::RwLock;
use switchy_http_models::{Method as HttpMethod, StatusCode};
use thiserror::Error;

type HandlerFn = Arc<
    dyn Fn(
            SimulatedRequest,
        )
            -> Pin<Box<dyn std::future::Future<Output = Result<SimulatedResponse, Error>> + Send>>
        + Send
        + Sync,
>;

/// Errors that can occur during web server simulation.
#[derive(Debug, Error)]
pub enum Error {
    /// No route was found matching the request method and path.
    #[error("Route not found: {method} {path}")]
    RouteNotFound { method: HttpMethod, path: String },
    /// Handler execution failed.
    #[error("Handler execution failed: {0}")]
    HandlerFailed(String),
    /// Server is not running.
    #[error("Server not started")]
    ServerNotStarted,
    /// JSON serialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Represents a simulated HTTP request.
#[derive(Debug, Clone)]
pub struct SimulatedRequest {
    /// HTTP method (GET, POST, etc.).
    pub method: HttpMethod,
    /// Request path.
    pub path: String,
    /// Query string parameters.
    pub query_string: String,
    /// HTTP headers.
    pub headers: BTreeMap<String, String>,
    /// Request body.
    pub body: Option<Bytes>,
}

impl SimulatedRequest {
    /// Creates a new simulated request with the specified HTTP method and path.
    #[must_use]
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query_string: String::new(),
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Sets the query string for this request.
    #[must_use]
    pub fn with_query_string(mut self, query: impl Into<String>) -> Self {
        self.query_string = query.into();
        self
    }

    /// Adds a single header to this request.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Adds multiple headers to this request.
    #[must_use]
    pub fn with_headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Sets the body for this request.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Creates a new request with JSON body
    ///
    /// # Errors
    ///
    /// * Returns `Error::Serialization` if JSON serialization fails
    pub fn with_json_body<T: Serialize>(mut self, body: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(body)?;
        self.body = Some(json.into());
        self.headers
            .insert("content-type".to_string(), "application/json".to_string());
        Ok(self)
    }
}

/// Represents a simulated HTTP response.
#[derive(Debug, Clone)]
pub struct SimulatedResponse {
    /// HTTP status code.
    pub status_code: StatusCode,
    /// Response headers.
    pub headers: BTreeMap<String, String>,
    /// Response body.
    pub body: Option<Bytes>,
}

impl SimulatedResponse {
    /// Creates a new simulated response with the specified status code.
    #[must_use]
    pub const fn new(status_code: StatusCode) -> Self {
        Self {
            status_code,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Creates a new response with 200 OK status.
    #[must_use]
    pub const fn ok() -> Self {
        Self::new(StatusCode::Ok)
    }

    /// Creates a new response with 404 Not Found status.
    #[must_use]
    pub const fn not_found() -> Self {
        Self::new(StatusCode::NotFound)
    }

    /// Creates a new response with 500 Internal Server Error status.
    #[must_use]
    pub const fn internal_server_error() -> Self {
        Self::new(StatusCode::InternalServerError)
    }

    /// Adds a single header to this response.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Sets the body for this response.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Creates a new response with JSON body
    ///
    /// # Errors
    ///
    /// * Returns `Error::Serialization` if JSON serialization fails
    pub fn with_json_body<T: Serialize>(mut self, body: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(body)?;
        self.body = Some(json.into());
        self.headers
            .insert("content-type".to_string(), "application/json".to_string());
        Ok(self)
    }

    /// Sets the body as plain text and sets the appropriate content type header.
    #[must_use]
    pub fn with_text_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into().into_bytes().into());
        self.headers
            .insert("content-type".to_string(), "text/plain".to_string());
        self
    }

    /// Sets the body as HTML and sets the appropriate content type header.
    #[must_use]
    pub fn with_html_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into().into_bytes().into());
        self.headers
            .insert("content-type".to_string(), "text/html".to_string());
        self
    }
}

/// Route handler that processes simulated requests for a specific HTTP method and path.
pub struct RouteHandler {
    /// HTTP method this handler responds to.
    pub method: HttpMethod,
    /// Path pattern this handler matches.
    pub path_pattern: String,
    /// Handler function that processes the request.
    pub handler: HandlerFn,
}

impl std::fmt::Debug for RouteHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteHandler")
            .field("method", &self.method)
            .field("path_pattern", &self.path_pattern)
            .finish_non_exhaustive()
    }
}

impl Clone for RouteHandler {
    fn clone(&self) -> Self {
        Self {
            method: self.method,
            path_pattern: self.path_pattern.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

impl RouteHandler {
    /// Creates a new route handler for the specified method, path pattern, and handler function.
    #[must_use]
    pub fn new<F, Fut>(method: HttpMethod, path_pattern: impl Into<String>, handler: F) -> Self
    where
        F: Fn(SimulatedRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<SimulatedResponse, Error>> + Send + 'static,
    {
        Self {
            method,
            path_pattern: path_pattern.into(),
            handler: Arc::new(move |req| Box::pin(handler(req))),
        }
    }

    /// Handles a simulated request
    ///
    /// # Errors
    ///
    /// * Returns `Error::HandlerFailed` if the handler execution fails
    pub async fn handle(&self, request: SimulatedRequest) -> Result<SimulatedResponse, Error> {
        (self.handler)(request).await
    }

    /// Checks if this handler matches the given HTTP method and path.
    #[must_use]
    pub fn matches(&self, method: &HttpMethod, path: &str) -> bool {
        self.method == *method && self.path_matches(path)
    }

    fn path_matches(&self, path: &str) -> bool {
        // Simple exact match for now
        // TODO: Implement proper path pattern matching with parameters
        self.path_pattern == path
    }
}

/// Simulated web server for testing HTTP interactions without a real server.
#[derive(Debug)]
pub struct SimulationWebServer {
    routes: Arc<RwLock<Vec<RouteHandler>>>,
    mock_responses: Arc<RwLock<BTreeMap<String, SimulatedResponse>>>,
    request_log: Arc<Mutex<Vec<SimulatedRequest>>>,
    is_running: Arc<RwLock<bool>>,
}

impl SimulationWebServer {
    /// Creates a new simulation web server.
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: Arc::new(RwLock::new(Vec::new())),
            mock_responses: Arc::new(RwLock::new(BTreeMap::new())),
            request_log: Arc::new(Mutex::new(Vec::new())),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a route handler to the simulation server
    pub async fn add_route(&self, handler: RouteHandler) {
        let mut routes = self.routes.write().await;
        routes.push(handler);
    }

    /// Add a mock response for a specific request pattern
    pub async fn add_mock_response(&self, key: impl Into<String>, response: SimulatedResponse) {
        let mut mocks = self.mock_responses.write().await;
        mocks.insert(key.into(), response);
    }

    /// Handle a simulated HTTP request
    ///
    /// # Errors
    ///
    /// * Returns `Error::ServerNotStarted` if the server is not running
    /// * Returns `Error::RouteNotFound` if no matching route or mock response is found
    /// * Returns errors from the handler if handler execution fails
    ///
    /// # Panics
    ///
    /// * If the request log mutex is poisoned
    pub async fn handle_request(
        &self,
        request: SimulatedRequest,
    ) -> Result<SimulatedResponse, Error> {
        // Check if server is running
        if !*self.is_running.read().await {
            return Err(Error::ServerNotStarted);
        }

        // Log the request
        {
            let mut log = self.request_log.lock().unwrap();
            log.push(request.clone());
        }

        // Check for mock responses first
        let mock_key = format!("{} {}", request.method, request.path);
        {
            let mocks = self.mock_responses.read().await;
            if let Some(response) = mocks.get(&mock_key) {
                log::debug!("Returning mock response for {mock_key}");
                return Ok(response.clone());
            }
        }

        // Find matching route handler
        {
            let routes = self.routes.read().await;
            for route in routes.iter() {
                if route.matches(&request.method, &request.path) {
                    log::debug!(
                        "Found matching route for {} {}",
                        request.method,
                        request.path
                    );
                    return route.handle(request).await;
                }
            }
        }

        // No route found
        Err(Error::RouteNotFound {
            method: request.method,
            path: request.path,
        })
    }

    /// Get all logged requests
    ///
    /// # Panics
    ///
    /// * If the request log mutex is poisoned
    #[must_use]
    pub fn get_request_log(&self) -> Vec<SimulatedRequest> {
        let log = self.request_log.lock().unwrap();
        log.clone()
    }

    /// Clear the request log
    ///
    /// # Panics
    ///
    /// * If the request log mutex is poisoned
    pub fn clear_request_log(&self) {
        let mut log = self.request_log.lock().unwrap();
        log.clear();
    }

    /// Check if the server is running
    #[must_use]
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Start the simulation server
    ///
    /// # Errors
    ///
    /// * Currently never fails, but signature matches `WebServer` trait
    pub async fn start(&self) -> Result<(), Error> {
        *self.is_running.write().await = true;
        log::info!("Simulation web server started");
        Ok(())
    }

    /// Stop the simulation server
    pub async fn stop(&self) {
        let mut running = self.is_running.write().await;
        *running = false;
        log::info!("Simulation web server stopped");
    }
}

impl Default for SimulationWebServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebServer for SimulationWebServer {
    fn start(&self) -> Pin<Box<dyn std::future::Future<Output = ()>>> {
        let server = self.clone();
        Box::pin(async move {
            if let Err(e) = server.start().await {
                log::error!("Failed to start simulation web server: {e}");
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn std::future::Future<Output = ()>>> {
        let server = self.clone();
        Box::pin(async move {
            server.stop().await;
        })
    }
}

impl Clone for SimulationWebServer {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
            mock_responses: Arc::clone(&self.mock_responses),
            request_log: Arc::clone(&self.request_log),
            is_running: Arc::clone(&self.is_running),
        }
    }
}

/// Helper functions for creating common route handlers.
pub mod handlers {
    use super::{Arc, HttpMethod, RouteHandler, Serialize, SimulatedResponse};

    /// Creates a route handler that returns a JSON response.
    #[must_use]
    pub fn json_response<T: Serialize + Send + Sync + 'static>(
        method: HttpMethod,
        path: impl Into<String>,
        data: T,
    ) -> RouteHandler {
        let data = Arc::new(data);
        RouteHandler::new(method, path, move |_req| {
            let data = Arc::clone(&data);
            async move { SimulatedResponse::ok().with_json_body(&*data) }
        })
    }

    /// Creates a route handler that returns a plain text response.
    #[must_use]
    pub fn text_response(
        method: HttpMethod,
        path: impl Into<String>,
        text: impl Into<String>,
    ) -> RouteHandler {
        let text = text.into();
        RouteHandler::new(method, path, move |_req| {
            let text = text.clone();
            async move { Ok(SimulatedResponse::ok().with_text_body(text)) }
        })
    }

    /// Creates a route handler that returns an HTML response.
    #[must_use]
    pub fn html_response(
        method: HttpMethod,
        path: impl Into<String>,
        html: impl Into<String>,
    ) -> RouteHandler {
        let html = html.into();
        RouteHandler::new(method, path, move |_req| {
            let html = html.clone();
            async move { Ok(SimulatedResponse::ok().with_html_body(html)) }
        })
    }

    /// Creates a health check route handler that returns a success status.
    #[must_use]
    pub fn health_check(path: impl Into<String>) -> RouteHandler {
        json_response(
            HttpMethod::Get,
            path,
            serde_json::json!({"status": "ok", "timestamp": "simulation"}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Bytes, HttpMethod, SimulatedRequest, SimulatedResponse, SimulationWebServer, StatusCode,
        handlers,
    };

    #[switchy_async::test]
    async fn test_simulation_server_creation() {
        let server = SimulationWebServer::new();
        assert!(!server.is_running().await);
    }

    #[switchy_async::test]
    async fn test_server_start_stop() {
        let server = SimulationWebServer::new();

        assert!(!server.is_running().await);

        server.start().await.unwrap();
        assert!(server.is_running().await);

        server.stop().await;
        assert!(!server.is_running().await);
    }

    #[switchy_async::test]
    async fn test_mock_response() {
        let server = SimulationWebServer::new();
        server.start().await.unwrap();

        // Add mock response
        server
            .add_mock_response(
                "GET /test",
                SimulatedResponse::ok().with_text_body("Hello, World!"),
            )
            .await;

        // Test request
        let request = SimulatedRequest::new(HttpMethod::Get, "/test");
        let response = server.handle_request(request).await.unwrap();

        assert_eq!(response.status_code, StatusCode::Ok);
        assert_eq!(response.body.unwrap(), Bytes::from("Hello, World!"));
    }

    #[switchy_async::test]
    async fn test_route_handler() {
        let server = SimulationWebServer::new();
        server.start().await.unwrap();

        // Add route handler
        let handler = handlers::text_response(HttpMethod::Get, "/hello", "Hello from handler!");
        server.add_route(handler).await;

        // Test request
        let request = SimulatedRequest::new(HttpMethod::Get, "/hello");
        let response = server.handle_request(request).await.unwrap();

        assert_eq!(response.status_code, StatusCode::Ok);
        assert_eq!(response.body.unwrap(), Bytes::from("Hello from handler!"));
    }

    #[switchy_async::test]
    async fn test_request_logging() {
        let server = SimulationWebServer::new();
        server.start().await.unwrap();

        // Add mock response
        server
            .add_mock_response("GET /test", SimulatedResponse::ok())
            .await;

        // Make requests
        let request1 = SimulatedRequest::new(HttpMethod::Get, "/test");
        let request2 = SimulatedRequest::new(HttpMethod::Post, "/test");

        let _ = server.handle_request(request1).await;
        let _ = server.handle_request(request2).await;

        // Check log
        let log = server.get_request_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].method, HttpMethod::Get);
        assert_eq!(log[1].method, HttpMethod::Post);
    }
}
