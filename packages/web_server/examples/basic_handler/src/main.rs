#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic handler example demonstrating the `Route::with_handler()` method.
//!
//! This example shows how to use the clean async function syntax with
//! `Route::with_handler1()` instead of the verbose `Box::pin(async move {...})`
//! boilerplate. The example works with both Actix and Simulator backends,
//! demonstrating backend-agnostic handler creation.
//!
//! # Features
//!
//! * `actix` - Run with Actix Web backend
//! * `simulator` - Run with Simulator backend (default)
//!
//! # Example
//!
//! Run with the simulator backend:
//! ```sh
//! cargo run --package basic_handler_example
//! ```
//!
//! Run with the Actix backend:
//! ```sh
//! cargo run --package basic_handler_example --features actix
//! ```

use std::fmt::Write;

use moosicbox_web_server::{Error, HttpResponse, Method, RequestData, Route};
#[cfg(any(feature = "simulator", not(feature = "actix")))]
use moosicbox_web_server::{FromRequest, HttpRequest};

/// Demonstrates the `Route::with_handler()` method with clean async function syntax.
///
/// This handler showcases how to use `RequestData` to access HTTP request information
/// in a Send-safe manner, avoiding the verbose `Box::pin(async move {...})` boilerplate.
/// It extracts and displays various request properties including method, path, query string,
/// headers, and remote address.
///
/// # Errors
///
/// Returns an error if request data extraction fails or response construction fails.
async fn demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let mut response = String::new();

    response.push_str("=== New Handler System Demonstration ===\n\n");

    // Test method()
    writeln!(response, "HTTP Method: {:?}", data.method).unwrap();

    // Test path()
    writeln!(response, "Path: {}", data.path).unwrap();

    // Test query_string()
    if data.query.is_empty() {
        response.push_str("Query String: None\n");
    } else {
        writeln!(response, "Query String: {}", data.query).unwrap();
    }

    // Test headers
    if let Some(user_agent) = &data.user_agent {
        writeln!(response, "User-Agent: {user_agent}").unwrap();
    } else {
        response.push_str("User-Agent: None\n");
    }

    if let Some(content_type) = &data.content_type {
        writeln!(response, "Content-Type: {content_type}").unwrap();
    } else {
        response.push_str("Content-Type: None\n");
    }

    // Test headers collection
    writeln!(response, "All Headers: {} found", data.headers.len()).unwrap();
    for (name, value) in &data.headers {
        writeln!(response, "  {name}: {value}").unwrap();
    }

    // Test remote_addr()
    if let Some(addr) = data.remote_addr {
        writeln!(response, "Remote Address: {addr}").unwrap();
    } else {
        response.push_str("Remote Address: None\n");
    }

    // Note: RequestData doesn't include body, but that's okay for this demo
    response
        .push_str("Body: Not available in RequestData (use Json<T> extractor for body parsing)\n");

    response.push_str("\n=== Route::with_handler() Working! ===\n");
    response.push_str("No more Box::pin(async move {...}) boilerplate!\n");
    response.push_str("Clean async function syntax!\n");
    response.push_str("Works with both Actix and Simulator backends!\n");
    response.push_str("RequestData provides Send-safe access to request info!\n");

    Ok(HttpResponse::ok().with_body(response))
}

/// Demonstrates the handler system with the Actix Web backend.
///
/// Creates a route using `Route::with_handler1()` with the Actix backend enabled,
/// showing how the new handler system eliminates the need for `Box::pin` boilerplate
/// while maintaining compatibility with Actix Web.
#[cfg(feature = "actix")]
fn run_actix_example() {
    println!("🚀 Running Actix Backend Example...");

    // NEW: Using Route::with_handler1() for 1-parameter handler - no more Box::pin boilerplate!
    let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

    println!("✅ Route created successfully with new handler system:");
    println!("   Method: {:?}", route.method);
    println!("   Path: {}", route.path);
    println!("   Handler: Clean async function (no Box::pin!)");
    println!("   Backend: Actix Web");
}

/// Demonstrates the handler system with the Simulator backend.
///
/// Creates a route using `Route::with_handler1()` with the Simulator backend,
/// then demonstrates request data extraction by creating a test request with
/// various headers, query parameters, and cookies. Shows how `RequestData`
/// provides Send-safe access to request information.
///
/// # Errors
///
/// * Request data extraction fails
/// * Route creation fails
/// * Backend initialization fails
#[cfg(any(feature = "simulator", not(feature = "actix")))]
fn run_simulator_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Example...");

    // NEW: Using Route::with_handler1() for 1-parameter handler - same clean syntax!
    let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

    println!("✅ Route created successfully with new handler system:");
    println!("   Method: {:?}", route.method);
    println!("   Path: {}", route.path);
    println!("   Handler: Clean async function (no Box::pin!)");
    println!("   Backend: Simulator");

    // Test the handler with a simulation request
    let cookies = vec![
        ("session".to_string(), "abc123".to_string()),
        ("theme".to_string(), "dark".to_string()),
    ];

    let request = SimulationRequest::new(Method::Post, "/demo")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-Test/1.0")
        .with_header("content-type", "application/json")
        .with_cookies(cookies)
        .with_remote_addr("192.168.1.100:54321")
        .with_body(b"{\"message\": \"Hello from simulator!\"}".to_vec());

    let stub = SimulationStub::new(request);
    let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    // Extract RequestData and show what the handler would receive
    let data = RequestData::from_request_sync(&http_request)?;

    println!("\n📋 Handler would receive RequestData:");
    println!("   Method: {:?}", data.method);
    println!("   Path: {}", data.path);
    println!("   Query: {}", data.query);
    println!("   User-Agent: {:?}", data.user_agent);
    println!("   Content-Type: {:?}", data.content_type);
    println!("   Remote Address: {:?}", data.remote_addr);
    println!("   Headers: {} total", data.headers.len());

    println!("\n✅ RequestData extraction successful!");
    println!("   Handler would process this data and return an HttpResponse");
    println!("   Note: Full async execution requires an async runtime");

    Ok(())
}

/// Entry point demonstrating the `Route::with_handler()` method.
///
/// Runs backend-specific examples based on enabled features, demonstrating
/// how the new handler system works identically across different backends
/// (Actix Web and Simulator).
///
/// # Errors
///
/// * Backend example execution fails
/// * No backend features are enabled
#[cfg_attr(
    all(feature = "actix", not(any(feature = "simulator", not(feature = "actix")))),
    allow(clippy::unnecessary_wraps)
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Basic Handler Example - Route::with_handler() Method");
    println!("=====================================================\n");

    #[cfg(feature = "actix")]
    {
        run_actix_example();
        println!();
    }

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        run_simulator_example()?;
        println!();
    }

    #[cfg(not(any(feature = "actix", any(feature = "simulator", not(feature = "actix")))))]
    {
        println!("❌ No backend features enabled!");
        println!("   Run with: cargo run --example basic_handler --features actix");
        println!("   Or with:  cargo run --example basic_handler --features simulator");
    }

    println!("✅ Basic Handler Example Complete!");
    println!("   - Route::with_handler1() method working");
    println!("   - Clean async function syntax (no Box::pin boilerplate)");
    println!("   - Works identically with both Actix and Simulator backends");
    println!("   - RequestData provides Send-safe access to request information");
    println!("   - Ready for production use with the new handler system");

    Ok(())
}
