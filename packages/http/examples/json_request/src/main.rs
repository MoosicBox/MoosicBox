//! JSON API request example using the `switchy_http` crate.
//!
//! This example demonstrates how to work with JSON APIs by sending POST requests with
//! JSON payloads and deserializing JSON responses using the `json` feature.
//!
//! # Usage
//!
//! ```bash
//! cargo run --package switchy_http_json_request_example
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use serde::{Deserialize, Serialize};

/// Errors that can occur when running the JSON request example.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request error.
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
}

/// Example request payload for JSON POST request.
#[derive(Debug, Serialize)]
struct CreateUserRequest {
    name: String,
    job: String,
}

/// Example response payload from JSON API.
#[derive(Debug, Deserialize)]
struct CreateUserResponse {
    name: String,
    job: String,
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Example nested JSON structure for GET request.
#[derive(Debug, Deserialize)]
struct UserData {
    id: u32,
    email: String,
    first_name: String,
    last_name: String,
    avatar: String,
}

/// Example response containing nested data.
#[derive(Debug, Deserialize)]
struct GetUserResponse {
    data: UserData,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    // Create an HTTP client
    let client = switchy_http::Client::new();

    // Example 1: POST request with JSON body
    log::info!("Sending POST request with JSON body...");

    let request_body = CreateUserRequest {
        name: "John Doe".to_string(),
        job: "Software Engineer".to_string(),
    };

    let response = client
        .post("https://reqres.in/api/users")
        .json(&request_body)
        .send()
        .await?;

    // Deserialize JSON response
    let user_response: CreateUserResponse = response.json().await?;

    println!("POST Response:");
    println!("  Name: {}", user_response.name);
    println!("  Job: {}", user_response.job);
    println!("  ID: {}", user_response.id);
    println!("  Created At: {}", user_response.created_at);
    println!();

    // Example 2: GET request with JSON response
    log::info!("Sending GET request for JSON data...");

    let response = client.get("https://reqres.in/api/users/2").send().await?;

    // Deserialize nested JSON response
    let get_response: GetUserResponse = response.json().await?;

    println!("GET Response:");
    println!("  User ID: {}", get_response.data.id);
    println!("  Email: {}", get_response.data.email);
    println!(
        "  Name: {} {}",
        get_response.data.first_name, get_response.data.last_name
    );
    println!("  Avatar: {}", get_response.data.avatar);
    println!();

    // Example 3: Query parameters with JSON response
    log::info!("Sending GET request with query parameters...");

    let response = client
        .get("https://reqres.in/api/users")
        .query_param("page", "2")
        .send()
        .await?;

    // Check status code
    println!("Response status: {:?}", response.status());

    Ok(())
}
