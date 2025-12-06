//! Debug tests for `IntoHandler` trait implementation with Actix backend.
//!
//! This binary verifies that handler functions correctly implement the `IntoHandler`
//! trait for various parameter combinations.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_web_server::Error;
use switchy_web_server::handler::IntoHandler;
use switchy_web_server::{HttpResponse, RequestInfo};

/// Simple test function with no parameters.
///
/// This should implement `IntoHandler<()>`.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn simple_handler() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::ok().with_body("Simple handler"))
}

/// Test function with one extractor.
///
/// This should implement `IntoHandler<(RequestInfo,)>`.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn info_handler(info: RequestInfo) -> Result<HttpResponse, Error> {
    let path = &info.path;
    let response = format!("Path: {path}");
    Ok(HttpResponse::ok().with_body(response))
}

/// Runs the `IntoHandler` trait debug tests.
fn main() {
    println!("Testing IntoHandler trait implementation...");

    // Test simple handler (no parameters)
    let _simple_handler = simple_handler.into_handler();
    println!("✅ Simple handler (no params) implements IntoHandler!");

    // Test handler with one parameter
    let _info_handler = info_handler.into_handler();
    println!("✅ Info handler (1 param) implements IntoHandler!");

    println!("🎉 All IntoHandler trait tests passed!");
}
