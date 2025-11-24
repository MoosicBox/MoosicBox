#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `switchy_env`
//!
//! This example demonstrates basic environment variable access using `switchy_env`.
//! It shows how to:
//! - Get environment variables as strings
//! - Get variables with default values
//! - Parse variables to specific types
//! - Check if variables exist

use switchy_env::{var, var_exists, var_or, var_parse, var_parse_opt, var_parse_or};

fn main() {
    println!("=== Switchy Env Basic Usage Example ===\n");

    // Example 1: Get environment variable as string
    println!("1. Getting environment variables as strings:");
    match var("PATH") {
        Ok(path) => println!(
            "   PATH is set (truncated): {}...",
            &path[..path.len().min(50)]
        ),
        Err(e) => println!("   Error: {e}"),
    }

    // Example 2: Get variable with a default value
    println!("\n2. Getting variables with defaults:");
    let port = var_or("PORT", "8080");
    println!("   PORT (defaults to 8080): {port}");

    let debug = var_or("DEBUG", "false");
    println!("   DEBUG (defaults to false): {debug}");

    // Example 3: Parse environment variables to specific types
    println!("\n3. Parsing environment variables to specific types:");

    // Set some example variables for demonstration
    // SAFETY: This is an example program demonstrating environment variable usage
    unsafe {
        std::env::set_var("TIMEOUT", "30");
        std::env::set_var("MAX_CONNECTIONS", "100");
        std::env::set_var("ENABLE_CACHE", "true");
    }

    // Parse to u64
    match var_parse::<u64>("TIMEOUT") {
        Ok(timeout) => println!("   TIMEOUT as u64: {timeout}"),
        Err(e) => println!("   Error parsing TIMEOUT: {e}"),
    }

    // Parse to usize
    match var_parse::<usize>("MAX_CONNECTIONS") {
        Ok(max_conn) => println!("   MAX_CONNECTIONS as usize: {max_conn}"),
        Err(e) => println!("   Error parsing MAX_CONNECTIONS: {e}"),
    }

    // Parse to bool
    match var_parse::<bool>("ENABLE_CACHE") {
        Ok(enable) => println!("   ENABLE_CACHE as bool: {enable}"),
        Err(e) => println!("   Error parsing ENABLE_CACHE: {e}"),
    }

    // Example 4: Parse with default values
    println!("\n4. Parsing with defaults:");
    let workers: usize = var_parse_or("WORKERS", 4);
    println!("   WORKERS (defaults to 4): {workers}");

    let verbose: bool = var_parse_or("VERBOSE", false);
    println!("   VERBOSE (defaults to false): {verbose}");

    // Example 5: Optional parsing (useful when a variable may or may not be set)
    println!("\n5. Optional parsing:");

    // SAFETY: This is an example program demonstrating environment variable usage
    unsafe {
        std::env::set_var("LOG_LEVEL", "3");
    }

    match var_parse_opt::<u32>("LOG_LEVEL") {
        Ok(Some(level)) => println!("   LOG_LEVEL is set to: {level}"),
        Ok(None) => println!("   LOG_LEVEL is not set"),
        Err(e) => println!("   Error parsing LOG_LEVEL: {e}"),
    }

    match var_parse_opt::<u32>("UNSET_VAR") {
        Ok(Some(val)) => println!("   UNSET_VAR is set to: {val}"),
        Ok(None) => println!("   UNSET_VAR is not set (this is expected)"),
        Err(e) => println!("   Error parsing UNSET_VAR: {e}"),
    }

    // Example 6: Check if a variable exists
    println!("\n6. Checking variable existence:");
    println!("   PATH exists: {}", var_exists("PATH"));
    println!(
        "   NONEXISTENT_VAR exists: {}",
        var_exists("NONEXISTENT_VAR")
    );

    // Example 7: Handling parse errors
    println!("\n7. Handling parse errors:");
    // SAFETY: This is an example program demonstrating environment variable usage
    unsafe {
        std::env::set_var("INVALID_NUMBER", "not_a_number");
    }

    match var_parse::<u32>("INVALID_NUMBER") {
        Ok(val) => println!("   Parsed value: {val}"),
        Err(e) => println!("   Expected parse error: {e}"),
    }

    println!("\n=== Example completed successfully! ===");
}
