#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_logging`
//!
//! This example demonstrates:
//! - Initializing the logging system with file output
//! - Using standard log macros (info, debug, trace, etc.)
//! - Using the `debug_or_trace!` macro for conditional logging

use moosicbox_logging::{InitError, debug_or_trace, init, log};

fn main() -> Result<(), InitError> {
    // Initialize logging with a log file
    // The log file will be created in the config directory's logs subdirectory
    println!("Initializing logging system...");
    let _layer = init(Some("basic_usage_example.log"), None)?;
    println!("Logging initialized successfully!");
    println!();

    // Use standard log macros from the re-exported `log` crate
    println!("Demonstrating standard log macros:");
    log::error!("This is an error message");
    log::warn!("This is a warning message");
    log::info!("This is an info message");
    log::debug!("This is a debug message");
    log::trace!("This is a trace message");
    println!();

    // Use the debug_or_trace! macro
    // This macro logs at trace level if trace is enabled, otherwise at debug level
    println!("Demonstrating debug_or_trace! macro:");
    debug_or_trace!(
        ("Short debug message: Processing started"),
        ("Detailed trace message: Processing started with full context and details")
    );

    // Demonstrate logging with formatted strings
    let count = 42;
    let operation = "data processing";
    log::info!("Starting {operation} with {count} items");

    // Simulate some work
    for i in 1..=3 {
        log::debug!("Processing item {i}");
        debug_or_trace!(
            ("Item {i} processed"),
            ("Item {i} processed successfully with all metadata")
        );
    }

    log::info!("Completed {} of {count} items", 3);

    println!();
    println!("All log messages have been written!");
    println!("Check the log file at: {{config_dir}}/logs/basic_usage_example.log");

    Ok(())
}
