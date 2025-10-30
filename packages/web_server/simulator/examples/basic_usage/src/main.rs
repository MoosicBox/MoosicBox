//! Basic usage example for `web_server_simulator`
//!
//! This example demonstrates how to use the web server simulator for testing
//! HTTP interactions without starting a real server.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

use serde::{Deserialize, Serialize};
use switchy_http_models::Method as HttpMethod;
use web_server_simulator::{SimulatedRequest, SimulatedResponse, SimulationWebServer, handlers};

/// Example user data structure
#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for demonstration purposes
    env_logger::init();

    println!("=== Web Server Simulator - Basic Usage Example ===\n");

    // Step 1: Create a new simulation web server
    println!("1. Creating simulation web server...");
    let server = SimulationWebServer::new();
    println!("   Server created (not yet started)\n");

    // Step 2: Add route handlers using helper functions
    println!("2. Adding route handlers...");

    // Add a simple text response handler
    let hello_handler = handlers::text_response(HttpMethod::Get, "/hello", "Hello, World!");
    server.add_route(hello_handler).await;
    println!("   Added: GET /hello (text response)");

    // Add an HTML response handler
    let home_handler = handlers::html_response(
        HttpMethod::Get,
        "/",
        "<html><body><h1>Welcome to Web Server Simulator</h1></body></html>",
    );
    server.add_route(home_handler).await;
    println!("   Added: GET / (HTML response)");

    // Add a JSON response handler with structured data
    let user = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };
    let user_handler = handlers::json_response(HttpMethod::Get, "/api/user", user);
    server.add_route(user_handler).await;
    println!("   Added: GET /api/user (JSON response)");

    // Add a health check endpoint
    let health_handler = handlers::health_check("/health");
    server.add_route(health_handler).await;
    println!("   Added: GET /health (health check)\n");

    // Step 3: Add mock responses for specific request patterns
    println!("3. Adding mock responses...");
    server
        .add_mock_response(
            "GET /status",
            SimulatedResponse::ok().with_text_body("Service is running"),
        )
        .await;
    println!("   Added mock: GET /status\n");

    // Step 4: Start the simulation server
    println!("4. Starting simulation server...");
    server.start().await?;
    println!("   Server is now running: {}\n", server.is_running().await);

    // Step 5: Make simulated requests and verify responses
    println!("5. Making simulated requests...\n");

    // Request 1: Simple text endpoint
    println!("   Request: GET /hello");
    let request = SimulatedRequest::new(HttpMethod::Get, "/hello");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    if let Some(body) = response.body {
        println!("   Body: {}", String::from_utf8_lossy(&body));
    }
    println!();

    // Request 2: HTML endpoint
    println!("   Request: GET /");
    let request = SimulatedRequest::new(HttpMethod::Get, "/");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    let content_type = response
        .headers
        .get("content-type")
        .map_or("(none)", String::as_str);
    println!("   Content-Type: {content_type}");
    println!();

    // Request 3: JSON API endpoint
    println!("   Request: GET /api/user");
    let request = SimulatedRequest::new(HttpMethod::Get, "/api/user");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    if let Some(body) = &response.body {
        let user: User = serde_json::from_slice(body)?;
        println!("   User: {user:?}");
    }
    println!();

    // Request 4: Health check endpoint
    println!("   Request: GET /health");
    let request = SimulatedRequest::new(HttpMethod::Get, "/health");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    if let Some(body) = &response.body {
        println!("   Health: {}", String::from_utf8_lossy(body));
    }
    println!();

    // Request 5: Mock response endpoint
    println!("   Request: GET /status");
    let request = SimulatedRequest::new(HttpMethod::Get, "/status");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    if let Some(body) = response.body {
        println!("   Body: {}", String::from_utf8_lossy(&body));
    }
    println!();

    // Step 6: Demonstrate request with headers and query string
    println!("6. Making request with headers and query string...\n");
    let request = SimulatedRequest::new(HttpMethod::Get, "/hello")
        .with_query_string("lang=en")
        .with_header("User-Agent", "ExampleClient/1.0")
        .with_header("Accept", "text/plain");
    let response = server.handle_request(request).await?;
    println!("   Status: {}", response.status_code);
    println!();

    // Step 7: Demonstrate request logging
    println!("7. Checking request log...");
    let log = server.get_request_log();
    println!("   Total requests logged: {}", log.len());
    for (i, req) in log.iter().enumerate() {
        println!("   #{}: {} {}", i + 1, req.method, req.path);
    }
    println!();

    // Step 8: Test error handling - route not found
    println!("8. Testing error handling...\n");
    let request = SimulatedRequest::new(HttpMethod::Get, "/nonexistent");
    match server.handle_request(request).await {
        Ok(_) => println!("   Unexpected success"),
        Err(e) => println!("   Expected error: {e}"),
    }
    println!();

    // Step 9: Clear request log
    println!("9. Clearing request log...");
    server.clear_request_log();
    println!("   Requests in log: {}\n", server.get_request_log().len());

    // Step 10: Stop the server
    println!("10. Stopping simulation server...");
    server.stop().await;
    println!("    Server is now running: {}\n", server.is_running().await);

    println!("=== Example completed successfully! ===");

    Ok(())
}
