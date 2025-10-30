#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic logging example demonstrating initialization and usage of `MoosicBox` logging.
//!
//! This example shows:
//! - Initializing the logging system with file output
//! - Using standard log macros (info, debug, warn, error, trace)
//! - Using the conditional `debug_or_trace` macro
//! - Environment-based log filtering

use moosicbox_logging::{InitError, debug_or_trace, init, log};

fn main() -> Result<(), InitError> {
    // Initialize logging with a log file named "basic_usage.log"
    // The file will be created in {config_dir}/logs/basic_usage.log
    // Log level filtering is controlled by MOOSICBOX_LOG or RUST_LOG environment variables
    // Default: trace level in debug builds, info level in release builds
    let _layer = init(Some("basic_usage.log"), None)?;

    // Log at different levels to demonstrate the logging system
    log::info!("Application started - this is an info message");
    log::debug!("This is a debug message with details: counter = {}", 42);
    log::warn!("This is a warning message");
    log::error!("This is an error message");
    log::trace!("This is a trace message with very detailed information");

    // Demonstrate the debug_or_trace macro
    // This macro logs at trace level if trace logging is enabled,
    // otherwise it falls back to debug level
    debug_or_trace!(
        ("Short debug message: operation completed"),
        (
            "Detailed trace message: operation completed with result = {:?}",
            "success"
        )
    );

    // Simulate some application work
    perform_calculation(10, 20);

    // Log completion
    log::info!("Application finished successfully");

    println!("\nExample completed!");
    println!("Check the log file at: {{config_dir}}/logs/basic_usage.log");
    println!("\nTip: Run with RUST_LOG=trace to see all messages:");
    println!(
        "  RUST_LOG=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml"
    );

    Ok(())
}

/// Example function that uses logging
fn perform_calculation(a: i32, b: i32) -> i32 {
    log::debug!("perform_calculation called with a={a}, b={b}");

    let result = a + b;

    log::trace!("Calculation details: {a} + {b} = {result}");
    log::info!("Calculation result: {result}");

    result
}
