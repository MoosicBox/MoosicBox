//! Test binary for synchronous `FromRequest` trait extraction.
//!
//! This binary validates the synchronous extraction logic of the `FromRequest` trait
//! implementation for various types including `RequestData`, `String`, `u32`, and `bool`.
//! It uses the simulator stub to create test HTTP requests and verifies that extraction
//! works correctly for both valid and invalid inputs.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};
use moosicbox_web_server::{FromRequest, HttpRequest, Method, RequestData};

/// Creates a test `HttpRequest` with predefined headers and query parameters.
///
/// This helper constructs an `HttpRequest` using the simulator with:
/// * Method: GET
/// * Path: `/test/path`
/// * Query: `name=john&age=30&active=true`
/// * Headers: user-agent, content-type, authorization
/// * Remote address: `127.0.0.1:8080`
#[must_use]
fn create_test_request() -> HttpRequest {
    let sim_req = SimulationRequest::new(Method::Get, "/test/path")
        .with_query_string("name=john&age=30&active=true")
        .with_header("user-agent", "test-agent")
        .with_header("content-type", "application/json")
        .with_header("authorization", "Bearer token123")
        .with_remote_addr("127.0.0.1:8080");

    HttpRequest::new(SimulationStub::new(sim_req))
}

/// Creates a test `HttpRequest` with a custom query string.
///
/// # Arguments
///
/// * `query` - The query string to include in the request
#[must_use]
fn create_test_request_with_query(query: &str) -> HttpRequest {
    let sim_req = SimulationRequest::new(Method::Get, "/test").with_query_string(query);

    HttpRequest::new(SimulationStub::new(sim_req))
}

/// Tests synchronous extraction of `RequestData` from an `HttpRequest`.
///
/// Validates that all fields (method, path, query, headers, etc.) are correctly
/// extracted from a simulated HTTP request.
///
/// # Errors
///
/// Returns an error if:
/// * `RequestData` extraction fails
/// * Any extracted field doesn't match expected values
fn test_request_data_sync_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing RequestData sync extraction...");

    let req = create_test_request();

    // Actually call from_request_sync
    let result = RequestData::from_request_sync(&req);

    match result {
        Ok(data) => {
            println!("✅ RequestData extracted successfully");
            println!("  Method: {:?}", data.method);
            println!("  Path: {}", data.path);
            println!("  Query: {}", data.query);
            println!("  Headers count: {}", data.headers.len());

            // Verify the extraction worked correctly
            assert_eq!(data.method, Method::Get);
            assert_eq!(data.path, "/test/path");
            assert_eq!(data.query, "name=john&age=30&active=true");
            assert!(data.user_agent.is_some());
            assert_eq!(data.user_agent.as_ref().unwrap(), "test-agent");
            assert!(data.content_type.is_some());
            assert_eq!(data.content_type.as_ref().unwrap(), "application/json");
            assert!(data.remote_addr.is_some());
            println!("✅ All RequestData fields extracted correctly");
        }
        Err(e) => {
            println!("❌ RequestData extraction failed: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

/// Tests synchronous extraction of `String` from query string.
///
/// Validates that string values can be correctly extracted from the request query.
///
/// # Errors
///
/// Returns an error if:
/// * String extraction fails
/// * Extracted value doesn't match expected content
fn test_string_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing String extraction...");

    let req = create_test_request_with_query("hello world");

    let result = String::from_request_sync(&req);

    match result {
        Ok(value) => {
            println!("✅ String extracted: '{value}'");
            assert_eq!(value, "hello world");
        }
        Err(e) => {
            println!("❌ String extraction failed: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

/// Tests synchronous extraction of `u32` from query string.
///
/// Validates both successful parsing of valid integers and proper error handling
/// for invalid input.
///
/// # Errors
///
/// Returns an error if:
/// * Valid `u32` values fail to parse
/// * Invalid input doesn't produce expected error
fn test_u32_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing u32 extraction...");

    // Test valid number
    let req = create_test_request_with_query("42");
    let result = u32::from_request_sync(&req);

    match result {
        Ok(value) => {
            println!("✅ u32 extracted: {value}");
            assert_eq!(value, 42);
        }
        Err(e) => {
            println!("❌ u32 extraction failed: {e}");
            return Err(e.into());
        }
    }

    // Test invalid number (should fail)
    let req = create_test_request_with_query("not_a_number");
    let result = u32::from_request_sync(&req);

    match result {
        Ok(_) => {
            println!("❌ u32 extraction should have failed for invalid input");
            return Err("Expected error for invalid u32".into());
        }
        Err(e) => {
            println!("✅ u32 extraction properly failed for invalid input: {e}");
            assert!(e.to_string().contains("Failed to parse"));
        }
    }

    Ok(())
}

/// Tests synchronous extraction of `bool` from query string.
///
/// Validates that various boolean representations (true/false, 1/0, yes/no, on/off)
/// are correctly parsed into boolean values.
///
/// # Errors
///
/// Returns an error if:
/// * Boolean extraction fails
/// * Any test case produces unexpected value
fn test_bool_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing bool extraction...");

    let test_cases = [
        ("true", true),
        ("1", true),
        ("yes", true),
        ("on", true),
        ("false", false),
        ("0", false),
        ("no", false),
        ("off", false),
        ("anything_else", false),
    ];

    for (input, expected) in test_cases {
        let req = create_test_request_with_query(input);
        let result = bool::from_request_sync(&req);

        match result {
            Ok(value) => {
                println!("✅ bool('{input}') = {value}");
                assert_eq!(value, expected, "Failed for input '{input}'");
            }
            Err(e) => {
                println!("❌ bool extraction failed for '{input}': {e}");
                return Err(e.into());
            }
        }
    }

    Ok(())
}

/// Main entry point for synchronous `FromRequest` extraction tests.
///
/// Runs a comprehensive test suite validating synchronous extraction of various types
/// from HTTP requests.
///
/// # Errors
///
/// Returns an error if any test fails.
#[switchy_async::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing synchronous extraction with FromRequest trait...");
    println!();

    // Run all the actual tests
    test_request_data_sync_extraction()?;
    println!();

    test_string_extraction()?;
    println!();

    test_u32_extraction()?;
    println!();

    test_bool_extraction()?;
    println!();

    println!("🎉 All synchronous FromRequest tests passed!");
    println!("📝 These tests actually validate extraction logic, not just imports");

    Ok(())
}
