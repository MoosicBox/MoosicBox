#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating custom headers and query parameters with `switchy_http`.
//!
//! This example shows how to add custom HTTP headers and query parameters to requests,
//! which is essential for API authentication, pagination, filtering, and other common
//! HTTP use cases.

use serde::Deserialize;

/// Errors that can occur when running the headers and params example.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request error.
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Response structure from httpbin.org/get endpoint.
#[derive(Debug, Deserialize)]
struct HttpBinResponse {
    /// The headers that were sent in the request.
    headers: serde_json::Value,
    /// The query parameters that were sent.
    args: serde_json::Value,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = switchy_http::Client::new();

    // Example 1: Adding custom headers
    log::info!("Example 1: Custom headers");
    println!("=== Example 1: Custom Headers ===\n");

    let response = client
        .get("https://httpbin.org/headers")
        .header("User-Agent", "switchy_http/1.0")
        .header("X-Custom-Header", "my-custom-value")
        .header("Authorization", "Bearer fake-token-for-demo")
        .send()
        .await?;

    let response_text = response.text().await?;
    println!("Response with custom headers:");
    println!("{response_text}\n");

    // Example 2: Adding query parameters
    log::info!("Example 2: Query parameters");
    println!("=== Example 2: Query Parameters ===\n");

    let response = client
        .get("https://httpbin.org/get")
        .query_param("page", "1")
        .query_param("limit", "10")
        .query_param("sort", "name")
        .send()
        .await?;

    let httpbin_response: HttpBinResponse = response.json().await?;
    println!("Query parameters received by server:");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&httpbin_response.args)?
    );

    // Example 3: Optional query parameters
    log::info!("Example 3: Optional query parameters");
    println!("=== Example 3: Optional Query Parameters ===\n");

    let user_filter: Option<&str> = Some("john");
    let category_filter: Option<&str> = None;

    let response = client
        .get("https://httpbin.org/get")
        .query_param("status", "active")
        .query_param_opt("user", user_filter)
        .query_param_opt("category", category_filter)
        .send()
        .await?;

    let httpbin_response: HttpBinResponse = response.json().await?;
    println!("Query parameters (with optionals):");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&httpbin_response.args)?
    );

    // Example 4: Bulk query parameters
    log::info!("Example 4: Bulk query parameters");
    println!("=== Example 4: Bulk Query Parameters ===\n");

    let params = [
        ("filter[status]", "active"),
        ("filter[type]", "premium"),
        ("sort", "-created_at"),
        ("page", "2"),
    ];

    let response = client
        .get("https://httpbin.org/get")
        .query_params(&params)
        .send()
        .await?;

    let httpbin_response: HttpBinResponse = response.json().await?;
    println!("Bulk query parameters:");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&httpbin_response.args)?
    );

    // Example 5: Combining headers and query parameters
    log::info!("Example 5: Combined headers and query parameters");
    println!("=== Example 5: Combined Headers and Query Parameters ===\n");

    let response = client
        .get("https://httpbin.org/get")
        .header("Accept", "application/json")
        .header("X-API-Key", "demo-key-123")
        .query_param("api_version", "v2")
        .query_param("format", "json")
        .send()
        .await?;

    let httpbin_response: HttpBinResponse = response.json().await?;
    println!("Request with both headers and query params:");
    println!(
        "Headers: {}",
        serde_json::to_string_pretty(&httpbin_response.headers)?
    );
    println!(
        "Query params: {}\n",
        serde_json::to_string_pretty(&httpbin_response.args)?
    );

    // Example 6: Using the Header enum for common headers
    log::info!("Example 6: Using Header enum");
    println!("=== Example 6: Using Header Enum for Common Headers ===\n");

    let response = client
        .get("https://httpbin.org/headers")
        .header(switchy_http::Header::UserAgent.as_ref(), "CustomBot/1.0")
        .header(switchy_http::Header::Authorization.as_ref(), "Bearer token")
        .send()
        .await?;

    let response_text = response.text().await?;
    println!("Response with Header enum:");
    println!("{response_text}\n");

    println!("All examples completed successfully!");

    Ok(())
}
