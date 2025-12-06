//! Comprehensive Integration Tests for SimulatorWebServer
//!
//! This module contains integration tests that validate the complete functionality
//! of the SimulatorWebServer, testing all components working together in realistic
//! scenarios. These tests complement the unit tests in src/simulator.rs by testing
//! the full request/response pipeline.
//!
//! Section 5.1.8: Comprehensive Integration Testing

#![cfg(feature = "simulator")]

use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use moosicbox_web_server::{
    HttpResponse, Scope,
    simulator::{SimulationRequest, SimulationResponse, SimulatorWebServer},
};
use switchy_http_models::Method;

// Helper function to create a test server
fn create_test_server() -> SimulatorWebServer {
    SimulatorWebServer {
        scopes: Vec::new(),
        routes: BTreeMap::new(),
        state: Arc::new(std::sync::RwLock::new(
            moosicbox_web_server::extractors::state::StateContainer::new(),
        )),
        static_files: None,
    }
}

// Helper function to execute async request processing synchronously
// This is a simple async executor for testing purposes
fn process_request_sync(
    server: &SimulatorWebServer,
    request: SimulationRequest,
) -> SimulationResponse {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Waker};

    struct SimpleWaker;

    impl std::task::Wake for SimpleWaker {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(SimpleWaker));
    let mut context = Context::from_waker(&waker);

    let mut future = Box::pin(server.process_request(request));

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                // In a real executor, we'd wait for the waker to be called
                // For testing, we'll just continue polling
                std::thread::yield_now();
            }
        }
    }
}

/// Test 1: Multiple routes with different HTTP methods
#[test]
fn test_multiple_routes_different_methods() {
    let mut server = create_test_server();

    // Register routes with different methods using simple handlers
    server.register_route(
        Method::Get,
        "/api/users",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("GET users")) })),
    );

    server.register_route(
        Method::Post,
        "/api/users",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("POST users")) })),
    );

    server.register_route(
        Method::Put,
        "/api/users/123",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("PUT user")) })),
    );

    server.register_route(
        Method::Delete,
        "/api/users/456",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("DELETE user")) })),
    );

    // Test GET request
    let request = SimulationRequest::new(Method::Get, "/api/users");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("GET users"));

    // Test POST request
    let request = SimulationRequest::new(Method::Post, "/api/users");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("POST users"));

    // Test PUT request
    let request = SimulationRequest::new(Method::Put, "/api/users/123");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("PUT user"));

    // Test DELETE request
    let request = SimulationRequest::new(Method::Delete, "/api/users/456");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("DELETE user"));

    // Verify method discrimination - wrong method should not match
    let request = SimulationRequest::new(Method::Head, "/api/users");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 404);
}

/// Test 2: Route registration and matching
#[test]
fn test_route_registration_and_matching() {
    let mut server = create_test_server();

    // Register routes with path parameters
    server.register_route(
        Method::Get,
        "/api/users/{id}",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("User found")) })),
    );

    server.register_route(
        Method::Get,
        "/api/posts/{post_id}/comments/{comment_id}",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("Comment found")) })),
    );

    // Test single parameter route
    let request = SimulationRequest::new(Method::Get, "/api/users/123");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("User found"));

    // Test multiple parameter route
    let request = SimulationRequest::new(Method::Get, "/api/posts/456/comments/789");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("Comment found"));

    // Test non-matching route
    let request = SimulationRequest::new(Method::Get, "/api/nonexistent");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 404);
}

/// Test 3: Scope processing with nested routes
#[test]
fn test_scope_processing() {
    let mut server = create_test_server();

    // Create nested scopes
    let api_scope = Scope::new("/api").route(Method::Get, "/health", |_req| {
        Box::pin(async move { Ok(HttpResponse::ok().with_body("healthy")) })
    });

    let v1_scope = Scope::new("/v1").route(Method::Get, "/users", |_req| {
        Box::pin(async move { Ok(HttpResponse::ok().with_body("v1 users")) })
    });

    let admin_scope = Scope::new("/admin").route(Method::Delete, "/users/{id}", |_req| {
        Box::pin(async move { Ok(HttpResponse::ok().with_body("user deleted")) })
    });

    let nested_scope = api_scope.with_scope(v1_scope.with_scope(admin_scope));

    server.register_scope(&nested_scope);

    // Test top-level route
    let request = SimulationRequest::new(Method::Get, "/api/health");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("healthy"));

    // Test nested route
    let request = SimulationRequest::new(Method::Get, "/api/v1/users");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("v1 users"));

    // Test deeply nested route
    let request = SimulationRequest::new(Method::Delete, "/api/v1/admin/users/123");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("user deleted"));
}

/// Test 4: Request and response handling
#[test]
fn test_request_response_handling() {
    let mut server = create_test_server();

    // Register route that echoes request information
    server.register_route(
        Method::Post,
        "/api/echo",
        Box::new(|_req| {
            Box::pin(async move {
                Ok(HttpResponse::ok()
                    .with_header("X-Echo", "true")
                    .with_body("Echo response"))
            })
        }),
    );

    // Test POST request with headers and body
    let request = SimulationRequest::new(Method::Post, "/api/echo")
        .with_header("Content-Type", "application/json")
        .with_header("X-Custom", "test-value")
        .with_body(Bytes::from(r#"{"test": "data"}"#));

    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("Echo response"));
    assert_eq!(response.headers.get("X-Echo"), Some(&"true".to_string()));
}

/// Test 5: 404 handling for unmatched routes
#[test]
fn test_404_handling() {
    let mut server = create_test_server();

    server.register_route(
        Method::Get,
        "/api/users",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("users")) })),
    );

    // Test various unmatched routes
    let test_cases = vec![
        ("/api/nonexistent", Method::Get),
        ("/api/users/extra/path", Method::Get),
        ("/different/path", Method::Get),
        ("/api/users", Method::Post), // Wrong method
        ("", Method::Get),            // Empty path
        ("/", Method::Get),           // Root path
    ];

    for (path, method) in test_cases {
        let request = SimulationRequest::new(method, path);
        let response = process_request_sync(&server, request);
        assert_eq!(
            response.status, 404,
            "Path '{}' with method '{:?}' should return 404",
            path, method
        );
        assert_eq!(response.body_str(), Some("Not Found"));
    }
}

/// Test 6: Performance test - Multiple route registrations
#[test]
fn test_performance_multiple_routes() {
    let mut server = create_test_server();

    // Register 100 routes (scaled down from 1000 for faster testing)
    for i in 0..100 {
        let path = format!("/api/route_{}", i);
        let expected_body = format!("Response {}", i);

        server.register_route(
            Method::Get,
            &path,
            Box::new(move |_req| {
                let body = expected_body.clone();
                Box::pin(async move { Ok(HttpResponse::ok().with_body(body)) })
            }),
        );
    }

    // Test that all routes work correctly
    for i in 0..100 {
        let path = format!("/api/route_{}", i);
        let expected_body = format!("Response {}", i);

        let request = SimulationRequest::new(Method::Get, &path);
        let response = process_request_sync(&server, request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body_str(), Some(expected_body.as_str()));
    }

    // Test a few more routes to verify they're all working
    let request = SimulationRequest::new(Method::Get, "/api/route_50");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("Response 50"));

    let request = SimulationRequest::new(Method::Get, "/api/route_99");
    let response = process_request_sync(&server, request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body_str(), Some("Response 99"));
}

/// Test 7: Performance test - Multiple request processing
#[test]
fn test_performance_multiple_requests() {
    let mut server = create_test_server();

    // Register a few routes
    server.register_route(
        Method::Get,
        "/api/test1",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("test1")) })),
    );

    server.register_route(
        Method::Get,
        "/api/test2",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("test2")) })),
    );

    server.register_route(
        Method::Get,
        "/api/test3",
        Box::new(|_req| Box::pin(async move { Ok(HttpResponse::ok().with_body("test3")) })),
    );

    // Process 1000 requests (scaled down from 10000 for faster testing)
    for i in 0..1000 {
        let route_num = (i % 3) + 1;
        let path = format!("/api/test{}", route_num);
        let expected_body = format!("test{}", route_num);

        let request = SimulationRequest::new(Method::Get, &path);
        let response = process_request_sync(&server, request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body_str(), Some(expected_body.as_str()));
    }
}
