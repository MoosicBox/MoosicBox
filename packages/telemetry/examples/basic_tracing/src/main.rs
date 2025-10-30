#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic OpenTelemetry tracing example
//!
//! This example demonstrates how to initialize and use OpenTelemetry tracing
//! with the `switchy_telemetry` package. It shows:
//! - Initializing a tracer with a service name
//! - Setting up a tracing subscriber
//! - Creating instrumented functions
//! - Manual span creation
//! - Logging events within spans

use std::time::Duration;

use switchy_telemetry::init_tracer;
use tracing::{Level, info, instrument, span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Main entry point demonstrating OpenTelemetry tracing setup
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic OpenTelemetry Tracing Example ===\n");

    // Step 1: Initialize the OpenTelemetry tracer
    // This creates a tracer layer that will export spans to an OTLP endpoint
    println!("1. Initializing OpenTelemetry tracer...");
    let tracer_layer = init_tracer("basic-tracing-example")?;
    println!("   ✓ Tracer initialized successfully\n");

    // Step 2: Set up the tracing subscriber with the OpenTelemetry layer
    // The subscriber collects and processes all spans and events
    println!("2. Setting up tracing subscriber...");
    tracing_subscriber::registry()
        .with(tracer_layer)
        .with(tracing_subscriber::fmt::layer()) // Also log to stdout for visibility
        .init();
    println!("   ✓ Subscriber configured\n");

    // Step 3: Use instrumented functions (automatic span creation)
    println!("3. Calling instrumented functions...");
    instrumented_function("Alice", 42);
    println!();

    // Step 4: Demonstrate nested function calls with automatic span hierarchy
    println!("4. Demonstrating nested spans...");
    process_request(123)?;
    println!();

    // Step 5: Manual span creation for fine-grained control
    println!("5. Creating manual spans...");
    manual_span_example();
    println!();

    // Step 6: Simulate some work with spans
    println!("6. Simulating concurrent operations...");
    simulate_concurrent_work()?;
    println!();

    println!("=== Example completed successfully ===");
    println!("\nNote: Spans are exported to the OTLP endpoint configured via OTEL_ENDPOINT");
    println!("      Default endpoint: http://127.0.0.1:4317");
    println!("      Use a tool like Jaeger or Grafana Tempo to visualize traces");

    Ok(())
}

/// Function with automatic span creation via #[instrument]
///
/// The #[instrument] attribute automatically creates a span with:
/// - Span name matching the function name
/// - Function arguments as span attributes
#[instrument]
fn instrumented_function(name: &str, value: i32) {
    info!("Processing data for user: {}, value: {}", name, value);

    // Simulate some work
    std::thread::sleep(Duration::from_millis(50));

    info!("Completed processing for {}", name);
}

/// Demonstrates nested function calls with automatic span hierarchy
#[instrument]
fn process_request(request_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting request processing");

    // Call another instrumented function - this creates a child span
    validate_request(request_id)?;

    // Perform the main operation
    execute_operation(request_id)?;

    info!("Request processed successfully");
    Ok(())
}

/// Validates a request (creates a child span under `process_request`)
#[instrument]
fn validate_request(request_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Validating request");
    std::thread::sleep(Duration::from_millis(30));
    info!("Request {} is valid", request_id);
    Ok(())
}

/// Executes the main operation (creates a child span under `process_request`)
#[instrument]
fn execute_operation(request_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Executing operation");
    std::thread::sleep(Duration::from_millis(100));
    info!("Operation completed for request {}", request_id);
    Ok(())
}

/// Demonstrates manual span creation for fine-grained control
fn manual_span_example() {
    // Create a parent span manually
    let parent_span = span!(Level::INFO, "manual_operation", operation_type = "demo");
    let _enter = parent_span.enter();

    info!("Inside manual parent span");

    // Create a child span
    {
        let child_span = span!(Level::INFO, "manual_child_task", task_id = 1);
        let _child_enter = child_span.enter();

        info!("Inside manual child span");
        std::thread::sleep(Duration::from_millis(40));
    }

    info!("Back in parent span");
}

/// Simulates concurrent operations with spans
#[instrument]
fn simulate_concurrent_work() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting concurrent work simulation");

    // Process multiple items
    for i in 1..=3 {
        process_item(i)?;
    }

    info!("Concurrent work completed");
    Ok(())
}

/// Processes a single item
#[instrument]
fn process_item(item_id: u32) -> Result<(), Box<dyn std::error::Error>> {
    info!("Processing item {}", item_id);
    std::thread::sleep(Duration::from_millis(25));
    info!("Item {} processed", item_id);
    Ok(())
}
