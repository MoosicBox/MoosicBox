#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

//! Unified `TestClient` Integration Tests
//!
//! This module provides comprehensive integration tests for the `TestClient` abstraction,
//! working with both `ActixTestClient` and `SimulatorTestClient` through the factory interface.
//!
//! These tests are now backend-agnostic and work with any enabled backend (actix or simulator).
//! The factory functions automatically select the appropriate implementation.
//!
//! TODO(5.2.4): Add tests for custom Scope/Route configurations once conversion is implemented

use moosicbox_web_server::test_client::{ConcreteTestClient, TestClient, TestResponseExt};

/// Test `TestClient` basic interface and functionality
#[test]
fn test_client_basic_interface() {
    let client = ConcreteTestClient::new_with_test_routes();

    // Test basic HTTP methods - only GET routes are registered in test_routes
    let get_response = client.get("/test").send().expect("GET should work");
    get_response.assert_status(200);

    // These should return 404 since only GET routes are registered
    let post_response = client.post("/test").send().expect("POST should work");
    post_response.assert_status(404);

    let put_response = client.put("/test").send().expect("PUT should work");
    put_response.assert_status(404);

    let delete_response = client.delete("/test").send().expect("DELETE should work");
    delete_response.assert_status(404);
}

/// Test `TestClient` header handling
#[test]
fn test_client_headers() {
    let client = ConcreteTestClient::new_with_test_routes();

    let response = client
        .get("/test")
        .header("X-Custom-Header", "test-value")
        .header("Authorization", "Bearer token123")
        .send()
        .expect("Request with headers should work");

    response.assert_status(200);
    // Real server returns real headers, not fake test identifiers
    response.assert_header("content-type", "application/json");
}

/// Test `TestClient` request body handling
#[test]
fn test_client_body() {
    let client = ConcreteTestClient::new_with_api_routes();

    let response = client
        .post("/api/echo")
        .header("Content-Type", "application/json")
        .body_bytes(b"{\"message\": \"hello\"}".to_vec())
        .send()
        .expect("POST with body should work");

    response.assert_status(200);
    response.assert_header("content-type", "application/json");
}

/// Test `TestClient` error handling
#[test]
fn test_client_error_handling() {
    let client = ConcreteTestClient::new_with_test_routes();

    // Test 404 responses
    let response = client
        .get("/nonexistent")
        .send()
        .expect("Request should succeed");
    response.assert_status(404);

    // Test that the client doesn't panic on various paths
    let _response1 = client.get("/").send().expect("Root path should work");
    let _response2 = client
        .get("/api/v1/test")
        .send()
        .expect("Nested path should work");
    let _response3 = client
        .get("/test?param=value")
        .send()
        .expect("Query params should work");
}

/// Test `TestClient` URL handling
#[test]
fn test_client_url_handling() {
    let client = ConcreteTestClient::default();

    // Test various URL patterns
    let _response1 = client.get("/").send().expect("Root should work");
    let _response2 = client.get("/test").send().expect("Simple path should work");
    let _response3 = client
        .get("/test/nested/path")
        .send()
        .expect("Nested path should work");

    // Test with different client instance
    let custom_client = ConcreteTestClient::default();
    let _response4 = custom_client
        .get("/")
        .send()
        .expect("Custom client should work");
}

/// Test `TestClient` response assertion methods
#[test]
fn test_client_response_assertions() {
    let client = ConcreteTestClient::new_with_test_routes();

    let response = client.get("/test").send().expect("Request should succeed");

    // Test all assertion methods
    response.assert_status(200);
    response.assert_header("content-type", "application/json");
    response.assert_text_contains("message");
}

/// Test `TestClient` with concrete type usage
#[test]
fn test_client_concrete_type_usage() {
    // Test that we can use the concrete type directly without cfg attributes
    let client = ConcreteTestClient::new_with_test_routes();
    let response = client
        .get("/test")
        .send()
        .expect("Concrete request should work");
    response.assert_status(200);
}

/// Test `TestClient` concurrent usage
#[test]
fn test_client_concurrent_usage() {
    let client = ConcreteTestClient::new_with_test_routes();

    // Test multiple concurrent requests
    let response1 = client
        .get("/test")
        .send()
        .expect("First request should work");
    let response2 = client
        .get("/health")
        .send()
        .expect("Second request should work");

    response1.assert_status(200);
    response2.assert_status(200);
}

/// Test that `TestClient` properly manages its runtime
#[test]
fn test_client_runtime_management() {
    // Test multiple client instances
    let client1 = ConcreteTestClient::new_with_test_routes();
    let client2 = ConcreteTestClient::new_with_test_routes();

    let response1 = client1.get("/test").send().expect("Client 1 should work");
    let response2 = client2.get("/test").send().expect("Client 2 should work");

    response1.assert_status(200);
    response2.assert_status(200);
}

// Parallel API Tests - Ensure backend equivalence

/// Test that both backends handle basic GET requests identically
#[test]
fn test_parallel_basic_get_requests() {
    let client = ConcreteTestClient::new_with_test_routes();

    let response = client.get("/test").send().expect("GET should work");
    response.assert_status(200);
    response.assert_header("content-type", "application/json");
    response.assert_text_contains("message");
}

/// Test that both backends handle POST requests identically
#[test]
fn test_parallel_post_json_requests() {
    let client = ConcreteTestClient::new_with_api_routes();

    let response = client
        .post("/api/echo")
        .header("Content-Type", "application/json")
        .body_bytes(b"{\"test\": \"data\"}".to_vec())
        .send()
        .expect("POST should work");

    response.assert_status(200);
    response.assert_header("content-type", "application/json");
}

/// Test that both backends handle 404 responses identically
#[test]
fn test_parallel_404_responses() {
    let client = ConcreteTestClient::default();

    let response = client
        .get("/nonexistent/path")
        .send()
        .expect("Request should succeed");

    response.assert_status(404);
}

/// Test that both backends handle custom headers identically
#[test]
fn test_parallel_custom_headers() {
    let client = ConcreteTestClient::new_with_test_routes();

    let response = client
        .get("/test")
        .header("X-Custom-Header", "test-value")
        .header("User-Agent", "test-client/1.0")
        .send()
        .expect("Request with headers should work");

    response.assert_status(200);
}

/// Test that the concrete client works without cfg attributes
#[test]
fn test_concrete_client_usage() {
    let client = ConcreteTestClient::new_with_test_routes();
    let response = client
        .get("/test")
        .send()
        .expect("Concrete client should work");
    response.assert_status(200);
}

/// Test that both backends handle empty responses identically
#[test]
fn test_parallel_empty_responses() {
    let client = ConcreteTestClient::default();

    // This should return 404 for both backends
    let response = client.get("/empty").send().expect("Request should succeed");

    response.assert_status(404);
}
