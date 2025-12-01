//! Simulator-based test client implementation.
//!
//! This module provides `SimulatorTestClient`, a test client implementation that uses the
//! in-memory [`SimulatorWebServer`](crate::simulator::SimulatorWebServer) backend for fast,
//! deterministic testing without starting a real HTTP server.
//!
//! # Overview
//!
//! The simulator test client processes requests entirely in-memory, making it ideal for:
//!
//! * Unit testing HTTP handlers in isolation
//! * Fast test execution without network overhead
//! * Deterministic testing without port conflicts
//!
//! # Example
//!
//! ```rust,ignore
//! use moosicbox_web_server::test_client::simulator_impl::SimulatorTestClient;
//! use moosicbox_web_server::simulator::SimulatorWebServer;
//! use moosicbox_web_server::test_client::{TestClient, TestResponseExt};
//!
//! let server = SimulatorWebServer::with_test_routes();
//! let client = SimulatorTestClient::new(server);
//!
//! let response = client.get("/test").send().unwrap();
//! response.assert_status(200);
//! ```

use std::collections::BTreeMap;

use super::traits::GenericTestClient;
use super::{HttpMethod, TestClient, TestRequestBuilder, TestResponse};
use crate::{
    PathParams,
    simulator::{SimulationRequest, SimulatorWebServer},
};
use bytes::Bytes;
use switchy_http_models::Method;

/// Test client implementation for `SimulatorWebServer`
pub struct SimulatorTestClient {
    server: SimulatorWebServer,
}

impl SimulatorTestClient {
    /// Create a new simulator test client
    #[must_use]
    pub const fn new(server: SimulatorWebServer) -> Self {
        Self { server }
    }

    /// Get a reference to the underlying simulator server
    #[must_use]
    pub const fn server(&self) -> &SimulatorWebServer {
        &self.server
    }

    /// Get a mutable reference to the underlying simulator server
    #[must_use]
    pub const fn server_mut(&mut self) -> &mut SimulatorWebServer {
        &mut self.server
    }
}

/// Error type for simulator test client operations
#[derive(Debug, thiserror::Error)]
pub enum SimulatorTestClientError {
    /// Request processing error
    #[error("Request processing failed: {0}")]
    RequestProcessing(String),
    /// Invalid request data
    #[error("Invalid request data: {0}")]
    InvalidRequest(String),
}

impl TestClient for SimulatorTestClient {
    type Error = SimulatorTestClientError;

    fn get(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Get, path.to_string())
    }

    fn post(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Post, path.to_string())
    }

    fn put(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Put, path.to_string())
    }

    fn delete(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Delete, path.to_string())
    }

    fn execute_request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<TestResponse, Self::Error> {
        // Parse method string to Method enum
        let method_enum = match method.to_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            "PATCH" => Method::Patch,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            _ => {
                return Err(SimulatorTestClientError::InvalidRequest(format!(
                    "Unsupported HTTP method: {method}"
                )));
            }
        };

        // Split path and query string
        let (path_part, query_string) = path.find('?').map_or_else(
            || (path.to_string(), String::new()),
            |pos| (path[..pos].to_string(), path[pos + 1..].to_string()),
        );

        // Create simulation request
        let request = SimulationRequest {
            method: method_enum,
            path: path_part,
            query_string,
            headers: headers.clone(),
            body: body.map(|b| Bytes::from(b.to_vec())),
            cookies: BTreeMap::new(),
            remote_addr: None,
            path_params: PathParams::new(),
        };

        // Process request through simulator (this is async, but we need sync)
        // For now, we'll use a simple runtime to handle the async call
        let response = futures::executor::block_on(self.server.process_request(request));

        // Convert to TestResponse
        let response_body = response.body.unwrap_or_default().into_bytes();
        Ok(TestResponse::new(
            response.status,
            response.headers,
            response_body,
        ))
    }
}

impl GenericTestClient for SimulatorTestClient {
    type Error = SimulatorTestClientError;

    fn execute_request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<TestResponse, Self::Error> {
        // Delegate to the existing TestClient implementation
        <Self as TestClient>::execute_request(self, method, path, headers, body)
    }

    fn base_url(&self) -> String {
        // Simulator doesn't have a real URL, so return a placeholder
        "http://simulator".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::SimulatorWebServer;
    use crate::test_client::TestResponseExt;

    #[test]
    fn test_simulator_test_client_get() {
        let mut server = create_test_server();

        // Register a simple GET route
        server.register_route(
            switchy_http_models::Method::Get,
            "/test",
            Box::new(|_req| {
                Box::pin(async { Ok(crate::HttpResponse::ok().with_body("Hello, World!")) })
            }),
        );

        let client = SimulatorTestClient::new(server);
        let response = client.get("/test").send().expect("Request should succeed");

        response
            .assert_status(200)
            .assert_text_equals("Hello, World!");
    }

    // Helper function to create a test server
    fn create_test_server() -> SimulatorWebServer {
        use std::sync::Arc;
        SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(std::sync::RwLock::new(
                crate::extractors::state::StateContainer::new(),
            )),
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_simulator_test_client_post_json() {
        let mut server = create_test_server();

        // Register a POST route that echoes JSON
        server.register_route(
            switchy_http_models::Method::Post,
            "/echo",
            Box::new(|req| {
                let body_str = req.body().map_or_else(
                    || "{}".to_string(),
                    |body| String::from_utf8_lossy(body).to_string(),
                );

                Box::pin(async move {
                    Ok(crate::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body(body_str))
                })
            }),
        );

        let client = SimulatorTestClient::new(server);
        let test_data = serde_json::json!({"message": "test"});

        let response = client
            .post("/echo")
            .json(&test_data)
            .send()
            .expect("Request should succeed");

        response
            .assert_status(200)
            .assert_header("Content-Type", "application/json")
            .assert_json_equals(&test_data);
    }

    #[test]
    fn test_simulator_test_client_with_headers() {
        let mut server = create_test_server();

        // Register a route that returns the authorization header
        server.register_route(
            switchy_http_models::Method::Get,
            "/auth",
            Box::new(|req| {
                let auth_header = req
                    .header("authorization")
                    .unwrap_or("No auth header")
                    .to_string();

                Box::pin(async move { Ok(crate::HttpResponse::ok().with_body(auth_header)) })
            }),
        );

        let client = SimulatorTestClient::new(server);

        let response = client
            .get("/auth")
            .bearer_token("test-token")
            .send()
            .expect("Request should succeed");

        response
            .assert_status(200)
            .assert_text_equals("Bearer test-token");
    }

    #[test]
    fn test_simulator_test_client_404() {
        let server = create_test_server();
        let client = SimulatorTestClient::new(server);

        let response = client
            .get("/nonexistent")
            .send()
            .expect("Request should succeed");

        response.assert_status(404);
    }

    #[test]
    fn test_simulator_test_client_form_data() {
        let mut server = create_test_server();

        // Register a POST route that processes form data
        server.register_route(
            switchy_http_models::Method::Post,
            "/form",
            Box::new(|req| {
                let body_str = req.body().map_or_else(String::new, |body| {
                    String::from_utf8_lossy(body).to_string()
                });

                Box::pin(async move {
                    // Simple form parsing for test
                    let response_body =
                        if body_str.contains("name=test") && body_str.contains("value=123") {
                            "Form processed successfully"
                        } else {
                            "Invalid form data"
                        };

                    Ok(crate::HttpResponse::ok().with_body(response_body.to_string()))
                })
            }),
        );

        let client = SimulatorTestClient::new(server);

        let response = client
            .post("/form")
            .form([("name", "test"), ("value", "123")])
            .send()
            .expect("Request should succeed");

        response
            .assert_status(200)
            .assert_text_equals("Form processed successfully");
    }
}
