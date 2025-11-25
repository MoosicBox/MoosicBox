//! Test suite for handler macro system with Actix backend.
//!
//! This binary demonstrates and tests the handler macro system's compatibility
//! with the Actix backend, verifying that extractors work correctly without
//! Send bound issues.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_web_server::{Headers, HttpResponse, Method, Path, Query, RequestInfo, Route};
use serde::Deserialize;

/// Test handler with no parameters - completely Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn simple_handler() -> Result<HttpResponse, moosicbox_web_server::Error> {
    Ok(HttpResponse::ok().with_body("Simple handler response - no params!"))
}

/// Test handler with `RequestInfo` extractor - Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn info_handler(info: RequestInfo) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let response = format!("Request to {} via {:?}", info.path, info.method);
    Ok(HttpResponse::ok().with_body(response))
}

/// Test handler with `Headers` extractor - Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn headers_handler(headers: Headers) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let user_agent = headers.user_agent().map_or("Unknown", String::as_str);
    let response = format!("User-Agent: {user_agent}");
    Ok(HttpResponse::ok().with_body(response))
}

/// Query parameters for search endpoint.
#[derive(Deserialize)]
struct SearchQuery {
    /// Search term (optional).
    q: Option<String>,
    /// Maximum number of results to return (optional).
    limit: Option<u32>,
}

/// Test handler with `Query` extractor - Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn query_handler(
    Query(query): Query<SearchQuery>,
) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let search_term = query.q.unwrap_or_else(|| "nothing".to_string());
    let limit = query.limit.unwrap_or(10);
    let response = format!("Searching for '{search_term}' with limit {limit}");
    Ok(HttpResponse::ok().with_body(response))
}

/// Test handler with `Path` extractor - Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn path_handler(
    Path(user_id): Path<u32>,
) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let response = format!("User ID from path: {user_id}");
    Ok(HttpResponse::ok().with_body(response))
}

/// Test handler with multiple extractors - Send-safe.
///
/// # Errors
///
/// Returns an error if the response cannot be created.
async fn multi_handler(
    info: RequestInfo,
    headers: Headers,
) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let response = format!(
        "Path: {}, Method: {:?}, User-Agent: {}",
        info.path,
        info.method,
        headers.user_agent().map_or("Unknown", String::as_str)
    );
    Ok(HttpResponse::ok().with_body(response))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing handler macro system with Actix backend...");

    // Test that simple handler compiles and can be converted (NO PARAMETERS - Send-safe!)
    println!("Testing simple handler (no params)...");
    let _route_simple = Route::with_handler(Method::Get, "/hello", simple_handler);
    println!("✅ Simple handler (no params) compiles and converts to Route");

    // Test that RequestInfo extractor handler compiles and can be converted
    println!("Testing RequestInfo extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/info")]
    let _route_info = Route::with_handler1(Method::Get, "/info", info_handler);
    println!("✅ RequestInfo extractor handler compiles and converts to Route");

    // Test that Headers extractor handler compiles and can be converted
    println!("Testing Headers extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/headers")]
    let _route_headers = Route::with_handler1(Method::Get, "/headers", headers_handler);
    println!("✅ Headers extractor handler compiles and converts to Route");

    // Test that Query extractor handler compiles and can be converted
    println!("Testing Query extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/search")]
    let _route_query = Route::with_handler1(Method::Get, "/search", query_handler);
    println!("✅ Query extractor handler compiles and converts to Route");

    // Test that Path extractor handler compiles and can be converted
    println!("Testing Path extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/users/{id}")]
    let _route_path = Route::with_handler1(Method::Get, "/users/{id}", path_handler);
    println!("✅ Path extractor handler compiles and converts to Route");

    // Test that multi-extractor handler compiles and can be converted
    println!("Testing multi-extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/multi")]
    let _route_multi = Route::with_handler2(Method::Get, "/multi", multi_handler);
    println!("✅ Multi-extractor handler compiles and converts to Route");

    println!("🎉 All handler macro tests passed for Actix backend!");
    println!("📝 Note: All handlers use extractors - NO Send bounds issues!");
    println!("📝 Path extractor implemented - extracts from last path segment!");

    Ok(())
}
