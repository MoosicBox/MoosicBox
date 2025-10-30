//! Basic usage example for `switchy_env`
//!
//! This example demonstrates standard environment variable access patterns including:
//! - Reading variables as strings
//! - Parsing variables to specific types
//! - Using default values
//! - Handling optional variables
//! - Checking variable existence

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_env::standard::{var, var_exists, var_or, var_parse, var_parse_opt, var_parse_or};

fn main() {
    println!("=== switchy_env Basic Usage Example ===\n");

    // Set some test environment variables for demonstration
    // In real usage, these would come from your shell or .env file
    unsafe {
        std::env::set_var("EXAMPLE_PORT", "8080");
        std::env::set_var("EXAMPLE_DEBUG", "true");
        std::env::set_var("EXAMPLE_MAX_CONNECTIONS", "100");
        std::env::set_var("EXAMPLE_TIMEOUT_SECS", "30");
    }

    // 1. Read a variable as a string
    println!("1. Reading variables as strings:");
    match var("EXAMPLE_PORT") {
        Ok(port) => println!("   PORT = {port}"),
        Err(e) => println!("   Error: {e}"),
    }

    // 2. Read a variable with a default value
    println!("\n2. Using default values:");
    let host = var_or("EXAMPLE_HOST", "localhost");
    println!("   HOST = {host} (using default)");

    let port = var_or("EXAMPLE_PORT", "3000");
    println!("   PORT = {port} (from environment)");

    // 3. Parse variables to specific types
    println!("\n3. Parsing to specific types:");
    match var_parse::<u16>("EXAMPLE_PORT") {
        Ok(port_num) => println!("   PORT as u16 = {port_num}"),
        Err(e) => println!("   Error: {e}"),
    }

    match var_parse::<bool>("EXAMPLE_DEBUG") {
        Ok(debug) => println!("   DEBUG as bool = {debug}"),
        Err(e) => println!("   Error: {e}"),
    }

    // 4. Parse with default values
    println!("\n4. Parsing with defaults:");
    let max_conn: usize = var_parse_or("EXAMPLE_MAX_CONNECTIONS", 50);
    println!("   MAX_CONNECTIONS = {max_conn} (from environment)");

    let buffer_size: usize = var_parse_or("EXAMPLE_BUFFER_SIZE", 4096);
    println!("   BUFFER_SIZE = {buffer_size} (using default)");

    // 5. Handle optional variables
    println!("\n5. Optional variables:");
    match var_parse_opt::<u64>("EXAMPLE_TIMEOUT_SECS") {
        Ok(Some(timeout)) => println!("   TIMEOUT_SECS = {timeout} seconds"),
        Ok(None) => println!("   TIMEOUT_SECS not set"),
        Err(e) => println!("   Error parsing TIMEOUT_SECS: {e}"),
    }

    match var_parse_opt::<u64>("EXAMPLE_RETRY_COUNT") {
        Ok(Some(retries)) => println!("   RETRY_COUNT = {retries}"),
        Ok(None) => println!("   RETRY_COUNT not set (will use default)"),
        Err(e) => println!("   Error parsing RETRY_COUNT: {e}"),
    }

    // 6. Check if variables exist
    println!("\n6. Checking variable existence:");
    println!("   PORT exists: {}", var_exists("EXAMPLE_PORT"));
    println!(
        "   NONEXISTENT exists: {}",
        var_exists("EXAMPLE_NONEXISTENT")
    );

    // 7. Demonstrate error handling
    println!("\n7. Error handling:");
    match var("EXAMPLE_MISSING_VAR") {
        Ok(value) => println!("   Found value: {value}"),
        Err(e) => println!("   Expected error: {e}"),
    }

    // Attempt to parse an invalid value
    unsafe {
        std::env::set_var("EXAMPLE_INVALID_NUMBER", "not-a-number");
    }
    match var_parse::<u32>("EXAMPLE_INVALID_NUMBER") {
        Ok(value) => println!("   Parsed value: {value}"),
        Err(e) => println!("   Expected parse error: {e}"),
    }

    println!("\n=== Example Complete ===");
}
