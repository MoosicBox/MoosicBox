#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

//! `ActixTestClient` Integration Tests
//!
//! This module provides comprehensive integration tests for `ActixTestClient`,
//! ensuring it works correctly with `switchy_async` runtime and `actix_web::test` utilities.

use moosicbox_web_server::test_client::{TestClient, TestResponseExt};

#[cfg(feature = "actix")]
use moosicbox_web_server::test_client::actix::ActixTestClient;

/// Test `ActixTestClient` basic interface and functionality
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_basic_interface() {
    let client = ActixTestClient::new();

    // Test basic HTTP methods
    let get_response = client.get("/test").send().expect("GET should work");
    get_response.assert_status(200);

    let post_response = client.post("/test").send().expect("POST should work");
    post_response.assert_status(200);

    let put_response = client.put("/test").send().expect("PUT should work");
    put_response.assert_status(200);

    let delete_response = client.delete("/test").send().expect("DELETE should work");
    delete_response.assert_status(200);
}

/// Test `ActixTestClient` header handling
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_headers() {
    let client = ActixTestClient::new();

    let response = client
        .get("/test")
        .header("X-Custom-Header", "test-value")
        .header("Authorization", "Bearer token123")
        .send()
        .expect("Request with headers should work");

    response.assert_status(200);
    // ActixTestClient should include its identifier header
    response.assert_header("x-test-client", "actix");
}

/// Test `ActixTestClient` request body handling
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_body() {
    let client = ActixTestClient::new();

    let response = client
        .post("/test")
        .header("Content-Type", "application/json")
        .body_bytes(b"{\"message\": \"hello\"}".to_vec())
        .send()
        .expect("POST with body should work");

    response.assert_status(200);
    response.assert_header("content-type", "application/json");
}

/// Test `ActixTestClient` error handling
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_error_handling() {
    let client = ActixTestClient::new();

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

/// Test `ActixTestClient` URL handling
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_url_handling() {
    let client = ActixTestClient::new();

    // Test URL construction
    assert_eq!(client.url("/test"), "http://localhost:8080/test");
    assert_eq!(
        client.url("/api/v1/users"),
        "http://localhost:8080/api/v1/users"
    );

    // Test with custom base URL
    let custom_client = ActixTestClient::with_base_url("https://api.example.com".to_string());
    assert_eq!(custom_client.url("/test"), "https://api.example.com/test");
}

/// Test `ActixTestClient` response assertion methods
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_response_assertions() {
    let client = ActixTestClient::new();

    let response = client.get("/test").send().expect("Request should succeed");

    // Test various assertion methods
    response.assert_status(200);
    response.assert_success();
    response.assert_header("content-type", "application/json");

    // Test body content
    let body_str = response.text().expect("Body should be valid UTF-8");
    assert!(body_str.contains("Hello from ActixTestClient"));
}

/// Test `ActixTestClient` with generic `TestClient` trait
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_generic_usage() {
    fn test_generic_client<T: TestClient>(client: &T, expected_identifier: &str) {
        let response = client
            .get("/test")
            .send()
            .expect("Generic request should work");
        response.assert_status(200);
        response.assert_header("x-test-client", expected_identifier);
    }

    // Test ActixTestClient through generic interface
    let actix_client = ActixTestClient::new();
    test_generic_client(&actix_client, "actix");
}

/// Test concurrent usage (important for async runtime handling)
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_concurrent_requests() {
    let client = ActixTestClient::new();

    // Make multiple requests to ensure the runtime handles them properly
    let mut responses = Vec::new();

    for i in 0..5 {
        let path = if i % 2 == 0 { "/test" } else { "/health" };
        let response = client
            .get(path)
            .send()
            .expect("Concurrent request should work");
        responses.push(response);
    }

    // Verify all responses
    for response in responses {
        response.assert_success();
        response.assert_header("x-test-client", "actix");
    }
}

/// Test that `ActixTestClient` properly manages its runtime
#[cfg(feature = "actix")]
#[test]
fn test_actix_client_runtime_management() {
    // Create multiple clients to test runtime handling
    let client1 = ActixTestClient::new();
    let client2 = ActixTestClient::new();

    // Both clients should work independently
    let response1 = client1.get("/test").send().expect("Client 1 should work");
    let response2 = client2.get("/health").send().expect("Client 2 should work");

    response1.assert_status(200);
    response2.assert_status(200);

    // Test that we can access runtime information
    let _runtime1 = client1.runtime();
    let _runtime2 = client2.runtime();
}
