#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::future_not_send)] // Actix handlers don't need to be Send

//! Actix Web with OpenTelemetry Metrics Example
//!
//! This example demonstrates how to integrate OpenTelemetry tracing and metrics
//! into an Actix web application using `switchy_telemetry`. It shows:
//! - Setting up OpenTelemetry tracing for a web service
//! - Adding request tracing middleware
//! - Adding request metrics middleware
//! - Serving a /metrics endpoint
//! - Instrumenting request handlers

use std::sync::Arc;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, get, middleware, web};
use switchy_telemetry::{RequestTracing, get_http_metrics_handler, metrics};
use tracing::{info, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Main entry point for the Actix web server with OpenTelemetry integration
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("=== Actix Web with OpenTelemetry Example ===\n");

    // Step 1: Initialize OpenTelemetry tracing
    println!("1. Initializing OpenTelemetry tracing...");
    let tracer_layer = switchy_telemetry::init_tracer("actix-metrics-example")
        .map_err(std::io::Error::other)?;
    println!("   ✓ Tracer initialized successfully\n");

    // Step 2: Set up tracing subscriber with both OpenTelemetry and console output
    println!("2. Configuring tracing subscriber...");
    tracing_subscriber::registry()
        .with(tracer_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();
    println!("   ✓ Subscriber configured\n");

    // Step 3: Create the HTTP metrics handler
    // This handler provides the /metrics endpoint and request metrics middleware
    println!("3. Creating HTTP metrics handler...");
    let metrics_handler = Arc::new(get_http_metrics_handler());
    println!("   ✓ Metrics handler created\n");

    // Step 4: Start the HTTP server
    let bind_address = "127.0.0.1:8080";
    println!("4. Starting HTTP server on {bind_address}...");
    println!("   ✓ Server starting\n");

    println!("=== Server Ready ===");
    println!("Try these endpoints:");
    println!("  • http://127.0.0.1:8080/          - Simple greeting");
    println!("  • http://127.0.0.1:8080/hello/Bob - Personalized greeting");
    println!("  • http://127.0.0.1:8080/data      - JSON response example");
    println!("  • http://127.0.0.1:8080/metrics   - Telemetry metrics endpoint");
    println!("\nPress Ctrl+C to stop the server\n");

    HttpServer::new(move || {
        App::new()
            // Add logger middleware for request/response logging
            .wrap(middleware::Logger::default())
            // Add OpenTelemetry request tracing middleware
            // This creates a span for each incoming request
            .wrap(RequestTracing::new())
            // Add request metrics middleware
            // This collects metrics about requests (count, duration, etc.)
            .wrap(metrics_handler.request_middleware())
            // Add metrics handler to app data so the /metrics endpoint can access it
            .app_data(web::Data::new(metrics_handler.clone()))
            // Register service endpoints
            .service(index)
            .service(hello)
            .service(get_data)
            .service(metrics) // The /metrics endpoint from switchy_telemetry
    })
    .bind(bind_address)?
    .run()
    .await
}

/// Root endpoint - simple greeting
///
/// The #[instrument] attribute creates a span for this handler,
/// allowing you to trace the request through your application.
#[get("/")]
#[instrument(skip(req))]
async fn index(req: HttpRequest) -> HttpResponse {
    // Log information about the request
    info!(
        method = %req.method(),
        path = %req.path(),
        "Handling index request"
    );

    HttpResponse::Ok()
        .content_type("text/plain")
        .body("Hello! This is an Actix web server with OpenTelemetry integration.\n\nTry:\n  /hello/{name}\n  /data\n  /metrics\n")
}

/// Personalized greeting endpoint
///
/// Demonstrates path parameters with instrumented handlers
#[get("/hello/{name}")]
#[instrument(skip(req))]
async fn hello(req: HttpRequest, name: web::Path<String>) -> HttpResponse {
    let name = name.into_inner();

    info!(
        method = %req.method(),
        path = %req.path(),
        name = %name,
        "Handling personalized greeting"
    );

    // Simulate some processing
    simulate_work(&name).await;

    let message = format!("Hello, {name}! Your request has been traced with OpenTelemetry.\n");

    HttpResponse::Ok().content_type("text/plain").body(message)
}

/// JSON data endpoint
///
/// Demonstrates returning JSON with instrumentation
#[get("/data")]
#[instrument(skip(req))]
async fn get_data(req: HttpRequest) -> HttpResponse {
    info!(
        method = %req.method(),
        path = %req.path(),
        "Fetching data"
    );

    // Simulate fetching data
    let data = fetch_data().await;

    HttpResponse::Ok().json(data)
}

/// Simulates some asynchronous work
///
/// This creates a child span under the request handler span
#[instrument]
async fn simulate_work(name: &str) {
    info!("Simulating work for {}", name);

    // Simulate some processing time
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    info!("Work completed for {}", name);
}

/// Simulates fetching data from a database or service
///
/// Creates a child span to represent the data fetch operation
#[instrument]
async fn fetch_data() -> serde_json::Value {
    info!("Fetching data from source");

    // Simulate data retrieval time
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    info!("Data fetched successfully");

    serde_json::json!({
        "status": "success",
        "data": {
            "items": ["item1", "item2", "item3"],
            "count": 3
        },
        "traced": true,
        "message": "This response was generated with OpenTelemetry tracing"
    })
}
