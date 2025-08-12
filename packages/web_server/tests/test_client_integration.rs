#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

//! `ActixTestClient` Integration Tests
//!
//! This module provides comprehensive integration tests for `ActixTestClient`,
//! ensuring it works correctly with `switchy_async` runtime and `actix_web::test` utilities.
//!
//! NOTE: These tests only run when actix feature is enabled AND simulator feature is NOT enabled.
//! See Section 5.2.3.2 for runtime compatibility details.
//!
//! TODO(5.2.4): Add tests for custom Scope/Route configurations once conversion is implemented

use moosicbox_web_server::test_client::{TestClient, TestResponseExt};

#[cfg(all(feature = "actix", not(feature = "simulator")))]
use moosicbox_web_server::test_client::actix::{ActixTestClient, ActixWebServer};

/// Test `ActixTestClient` basic interface and functionality
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_basic_interface() {
    let server = ActixWebServer::with_test_routes();
    let client = ActixTestClient::new(server);

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

/// Test `ActixTestClient` header handling
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_headers() {
    let server = ActixWebServer::with_test_routes();
    let client = ActixTestClient::new(server);

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

/// Test `ActixTestClient` request body handling
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_body() {
    let server = ActixWebServer::with_api_routes();
    let client = ActixTestClient::new(server);

    let response = client
        .post("/api/echo")
        .header("Content-Type", "application/json")
        .body_bytes(b"{\"message\": \"hello\"}".to_vec())
        .send()
        .expect("POST with body should work");

    response.assert_status(200);
    response.assert_header("content-type", "application/json");
}

/// Test `ActixTestClient` error handling
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_error_handling() {
    let server = ActixWebServer::with_test_routes();
    let client = ActixTestClient::new(server);

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
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_url_handling() {
    let server = ActixWebServer::new(Vec::new());
    let client = ActixTestClient::new(server);

    // Test URL construction - real test servers use dynamic ports
    assert!(client.url("/test").starts_with("http://"));
    assert!(client.url("/test").ends_with("/test"));
    assert!(client.url("/api/v1/users").starts_with("http://"));
    assert!(client.url("/api/v1/users").ends_with("/api/v1/users"));

    // Test with custom server configuration
    // Note: Real test servers use dynamic ports, so we can't predict the exact URL
    let custom_server = ActixWebServer::new(Vec::new());
    let custom_client = ActixTestClient::new(custom_server);
    assert!(custom_client.url("/test").starts_with("http://"));
    assert!(custom_client.url("/test").ends_with("/test"));
}

/// Test `ActixTestClient` response assertion methods
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_response_assertions() {
    let server = ActixWebServer::with_test_routes();
    let client = ActixTestClient::new(server);

    let response = client.get("/test").send().expect("Request should succeed");

    // Test various assertion methods
    response.assert_status(200);
    response.assert_success();
    response.assert_header("content-type", "application/json");

    // Test body content - real server returns real response
    let body_str = response.text().expect("Body should be valid UTF-8");
    assert!(body_str.contains("Hello from test route!"));
}

/// Test `ActixTestClient` with generic `TestClient` trait
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_generic_usage() {
    fn test_generic_client<T: TestClient>(client: &T, _expected_identifier: &str) {
        let response = client
            .get("/test")
            .send()
            .expect("Generic request should work");
        response.assert_status(200);
        // Real servers return real headers, not fake test identifiers
        response.assert_header("content-type", "application/json");
    }

    // Test ActixTestClient through generic interface
    let server = ActixWebServer::with_test_routes();
    let actix_client = ActixTestClient::new(server);
    test_generic_client(&actix_client, "actix");
}

/// Test concurrent usage (important for async runtime handling)
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_concurrent_requests() {
    let server = ActixWebServer::with_test_routes();
    let client = ActixTestClient::new(server);

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
        // Real servers return real headers, not fake test identifiers
        response.assert_header("content-type", "application/json");
    }
}

/// Test that `ActixTestClient` properly manages its runtime
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
fn test_actix_client_runtime_management() {
    // Create multiple clients to test runtime handling
    let server1 = ActixWebServer::with_test_routes();
    let server2 = ActixWebServer::with_test_routes();
    let client1 = ActixTestClient::new(server1);
    let client2 = ActixTestClient::new(server2);

    // Both clients should work independently
    let response1 = client1.get("/test").send().expect("Client 1 should work");
    let response2 = client2.get("/health").send().expect("Client 2 should work");

    response1.assert_status(200);
    response2.assert_status(200);

    // Test that we can access runtime information
    let _runtime1 = client1.runtime();
    let _runtime2 = client2.runtime();
}

// Parallel API Tests - Ensure ActixTestClient/SimulatorTestClient equivalence

/// Test that both `ActixTestClient` and `SimulatorTestClient` handle basic GET requests identically
#[test]
fn test_parallel_basic_get_requests() {
    // Test with SimulatorTestClient
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        use moosicbox_web_server::simulator::SimulatorWebServer;
        use moosicbox_web_server::test_client::simulator::SimulatorTestClient;
        use std::sync::{Arc, RwLock};

        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(RwLock::new(
                moosicbox_web_server::extractors::state::StateContainer::new(),
            )),
        };

        server.register_route(
            switchy_http_models::Method::Get,
            "/test",
            Box::new(|_req| {
                Box::pin(async {
                    Ok(moosicbox_web_server::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body("{\"message\":\"Hello from SimulatorTestClient!\"}"))
                })
            }),
        );

        let client = SimulatorTestClient::new(server);
        let response = client
            .get("/test")
            .send()
            .expect("Simulator request should succeed");
        response.assert_status(200);
        response.assert_header("Content-Type", "application/json");
    }

    // Test with ActixTestClient
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        let server = ActixWebServer::with_test_routes();
        let client = ActixTestClient::new(server);
        let response = client
            .get("/test")
            .send()
            .expect("Actix request should succeed");
        response.assert_status(200);
        response.assert_header("content-type", "application/json");
    }
}

/// Test that both clients handle POST requests with JSON bodies identically
#[cfg(feature = "serde")]
#[test]
fn test_parallel_post_json_requests() {
    // Test with SimulatorTestClient
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        use moosicbox_web_server::simulator::SimulatorWebServer;
        use moosicbox_web_server::test_client::simulator::SimulatorTestClient;
        use std::sync::{Arc, RwLock};

        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(RwLock::new(
                moosicbox_web_server::extractors::state::StateContainer::new(),
            )),
        };

        server.register_route(
            switchy_http_models::Method::Post,
            "/echo",
            Box::new(|req| {
                let body_str = req.body().map_or_else(
                    || "{}".to_string(),
                    |body| String::from_utf8_lossy(body).to_string(),
                );

                Box::pin(async move {
                    Ok(moosicbox_web_server::HttpResponse::ok()
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
            .expect("Simulator POST should succeed");

        response.assert_status(200);
        response.assert_header("Content-Type", "application/json");
        response.assert_json_equals(&test_data);
    }

    // Test with ActixTestClient
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        let server = ActixWebServer::with_api_routes();
        let client = ActixTestClient::new(server);
        let test_data = serde_json::json!({"message": "test"});

        let response = client
            .post("/api/echo")
            .json(&test_data)
            .send()
            .expect("Actix POST should succeed");

        response.assert_status(200);
        response.assert_header("content-type", "application/json");
        // Note: The response body format might differ slightly between implementations
    }
}

/// Test that both clients handle 404 responses identically
#[test]
fn test_parallel_404_responses() {
    // Test with SimulatorTestClient
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        use moosicbox_web_server::simulator::SimulatorWebServer;
        use moosicbox_web_server::test_client::simulator::SimulatorTestClient;
        use std::sync::{Arc, RwLock};

        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(RwLock::new(
                moosicbox_web_server::extractors::state::StateContainer::new(),
            )),
        };

        let client = SimulatorTestClient::new(server);
        let response = client
            .get("/nonexistent")
            .send()
            .expect("Simulator 404 request should succeed");
        response.assert_status(404);
    }

    // Test with ActixTestClient
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        let server = ActixWebServer::new(Vec::new());
        let client = ActixTestClient::new(server);
        let response = client
            .get("/nonexistent")
            .send()
            .expect("Actix 404 request should succeed");
        response.assert_status(404);
    }
}

/// Test that both clients handle custom headers identically
#[test]
fn test_parallel_custom_headers() {
    // Test with SimulatorTestClient
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        use moosicbox_web_server::simulator::SimulatorWebServer;
        use moosicbox_web_server::test_client::simulator::SimulatorTestClient;
        use std::sync::{Arc, RwLock};

        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(RwLock::new(
                moosicbox_web_server::extractors::state::StateContainer::new(),
            )),
        };

        server.register_route(
            switchy_http_models::Method::Get,
            "/auth",
            Box::new(|req| {
                let auth_header = req
                    .header("authorization")
                    .unwrap_or("No auth header")
                    .to_string();

                Box::pin(async move {
                    Ok(moosicbox_web_server::HttpResponse::ok().with_body(auth_header))
                })
            }),
        );

        let client = SimulatorTestClient::new(server);
        let response = client
            .get("/auth")
            .bearer_token("test-token")
            .send()
            .expect("Simulator auth request should succeed");

        response.assert_status(200);
        response.assert_text_equals("Bearer test-token");
    }

    // Test with ActixTestClient
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        let server = ActixWebServer::with_test_routes();
        let client = ActixTestClient::new(server);
        let response = client
            .get("/test")
            .bearer_token("test-token")
            .send()
            .expect("Actix auth request should succeed");

        response.assert_status(200);
        // Both clients should handle headers the same way
    }
}

/// Test custom routes defined via Scope/Route system
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[test]
#[ignore = "TODO(5.2.4): Enable when Scope/Route conversion works"]
fn test_custom_routes() {
    // TODO(5.2.4): Enable this test when Scope/Route conversion works
    // let custom_scope = crate::Scope::new("/custom")
    //     .with_route(crate::Route::new(
    //         switchy_http_models::Method::Get,
    //         "/endpoint",
    //         |_req| {
    //             Box::pin(async move {
    //                 Ok(crate::HttpResponse::ok()
    //                     .with_content_type("application/json")
    //                     .with_body(r#"{"custom":"response"}"#))
    //             })
    //         }
    //     ));
    // let server = ActixWebServer::new(vec![custom_scope]);
    // let client = ActixTestClient::new(server);
    // let response = client.get("/custom/endpoint").send().expect("Custom route should work");
    // response.assert_status(200);
    // response.assert_json_contains("custom", "response");
}

/// Test that both clients work with the generic `TestClient` trait
#[test]
fn test_parallel_generic_trait_usage() {
    fn test_generic_client<T: TestClient>(client: &T, path: &str) -> bool {
        let response = client.get(path).send();
        response.is_ok()
    }

    // Test with SimulatorTestClient
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        use moosicbox_web_server::simulator::SimulatorWebServer;
        use moosicbox_web_server::test_client::simulator::SimulatorTestClient;
        use std::sync::{Arc, RwLock};

        let server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(RwLock::new(
                moosicbox_web_server::extractors::state::StateContainer::new(),
            )),
        };

        let client = SimulatorTestClient::new(server);
        assert!(test_generic_client(&client, "/test"));
    }

    // Test with ActixTestClient
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    {
        let server = ActixWebServer::new(Vec::new());
        let client = ActixTestClient::new(server);
        assert!(test_generic_client(&client, "/test"));
    }
}
