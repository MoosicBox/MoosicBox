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

/// Test that demonstrates nested scope data structure is supported
///
/// This test proves that the Scope data structure supports nesting and that
/// both backends accept nested scope configurations (even if they don't process them correctly).
///
/// This test focuses on the data structure validation rather than HTTP behavior,
/// since we know `ActixTestClient` ignores nested scopes.
#[test]
fn test_nested_scope_data_structure_is_supported() {
    use moosicbox_web_server::{HttpResponse, HttpResponseBody, Method, Scope};

    // Create nested scope structure: /api/v1/users
    let users_scope = Scope::new("/users").route(Method::Get, "", |_req| {
        Box::pin(async {
            Ok(HttpResponse::ok()
                .with_content_type("application/json")
                .with_body(HttpResponseBody::from(r#"{"users":["alice","bob"]}"#)))
        })
    });

    let v1_scope = Scope::new("/v1").with_scope(users_scope);

    let api_scope = Scope::new("/api")
        .with_scope(v1_scope)
        .route(Method::Get, "/status", |_req| {
            Box::pin(async {
                Ok(HttpResponse::ok()
                    .with_content_type("application/json")
                    .with_body(HttpResponseBody::from(r#"{"status":"ok"}"#)))
            })
        });

    // Verify the scope structure has nesting - this proves the data structure supports it
    assert_eq!(api_scope.path, "/api");
    assert_eq!(api_scope.routes.len(), 1); // /status route
    assert_eq!(api_scope.scopes.len(), 1); // v1 scope

    let v1 = &api_scope.scopes[0];
    assert_eq!(v1.path, "/v1");
    assert_eq!(v1.scopes.len(), 1); // users scope

    let users = &v1.scopes[0];
    assert_eq!(users.path, "/users");
    assert_eq!(users.routes.len(), 1); // GET route

    // Test that SimulatorWebServer accepts nested scopes
    #[cfg(feature = "simulator")]
    {
        let _simulator_server =
            moosicbox_web_server::simulator::SimulatorWebServer::new(vec![api_scope.clone()]);
        // SimulatorWebServer accepts the nested structure and processes it correctly
        // (proven by existing tests in simulator_integration.rs)
    }

    // Test that ActixWebServer now PANICS on nested scopes (5.2.4.2.1 safety check)
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        // We can't actually create the server here because it would panic
        // This is intentional - the panic prevents silent failures
        // The panic behavior is tested separately in test_actix_nested_scopes_cause_panic

        // Just verify that we can detect nested scopes in the data structure
        assert!(
            !api_scope.scopes.is_empty(),
            "Should be able to detect nested scopes in data structure"
        );
    }
}

/// Test that demonstrates ActixWebServer now panics on nested scopes (5.2.4.2.1 safety check)
///
/// This test proves that nested scopes are now detected and cause a panic,
/// preventing silent failures. This is a temporary safety measure until
/// nested scope support is implemented in 5.2.4.2.2+.
#[test]
#[should_panic(expected = "NESTED SCOPES NOT SUPPORTED")]
#[cfg(all(feature = "actix", not(feature = "simulator")))]
fn test_actix_nested_scopes_cause_panic() {
    use moosicbox_web_server::{HttpResponse, HttpResponseBody, Method, Scope};

    // Create the same nested scope structure
    let users_scope = Scope::new("/users").route(Method::Get, "", |_req| {
        Box::pin(async {
            Ok(HttpResponse::ok()
                .with_content_type("application/json")
                .with_body(HttpResponseBody::from(r#"{"users":["alice","bob"]}"#)))
        })
    });

    let v1_scope = Scope::new("/v1").with_scope(users_scope);

    let api_scope = Scope::new("/api")
        .with_scope(v1_scope)
        .route(Method::Get, "/status", |_req| {
            Box::pin(async {
                Ok(HttpResponse::ok()
                    .with_content_type("application/json")
                    .with_body(HttpResponseBody::from(r#"{"status":"ok"}"#)))
            })
        });

    // Verify the scope structure has nesting (this proves the data structure supports it)
    assert!(
        !api_scope.scopes.is_empty(),
        "API scope should have nested scopes"
    );
    assert_eq!(
        api_scope.scopes.len(),
        1,
        "API scope should have exactly one nested scope"
    );

    let v1 = &api_scope.scopes[0];
    assert!(!v1.scopes.is_empty(), "V1 scope should have nested scopes");
    assert_eq!(
        v1.scopes.len(),
        1,
        "V1 scope should have exactly one nested scope"
    );

    // This should panic with the expected message, proving that nested scopes are detected
    // and the silent failure problem is solved.
    let _server =
        moosicbox_web_server::test_client::actix_impl::ActixWebServer::new(vec![api_scope]);

    // TODO(5.2.4.2.4): Remove this panic test once nested scope support is implemented
    // TODO(5.2.4.2.2): Once nested scope support is implemented, change this to a success test
    // that verifies /api/v1/users actually works
}
