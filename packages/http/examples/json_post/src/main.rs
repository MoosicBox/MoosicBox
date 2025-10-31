#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! JSON POST request example using the `switchy_http` crate.
//!
//! This example demonstrates how to make HTTP POST requests with JSON payloads
//! and deserialize JSON responses.

use serde::{Deserialize, Serialize};

/// Errors that can occur when running the JSON POST example.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request error.
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Request payload structure for the POST request.
#[derive(Debug, Serialize)]
struct PostData {
    /// Example string field.
    title: String,
    /// Example integer field.
    user_id: i32,
    /// Example boolean field.
    completed: bool,
}

/// Response structure from the API.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    /// The ID assigned by the API.
    id: i32,
    /// The title from our request.
    title: String,
    /// The user ID from our request.
    #[serde(rename = "userId")]
    user_id: i32,
    /// The completed status from our request.
    completed: bool,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    // Create the request payload
    let post_data = PostData {
        title: "Learn switchy_http".to_string(),
        user_id: 1,
        completed: false,
    };

    log::info!("Sending POST request with data: {post_data:?}");

    // Create an HTTP client
    let client = switchy_http::Client::new();

    // Make a POST request with JSON body
    // Using httpbin.org's POST endpoint which echoes back the data
    let response = client
        .post("https://httpbin.org/post")
        .json(&post_data)
        .send()
        .await?;

    log::info!("Response status: {:?}", response.status());

    // Read the response as JSON
    let response_text = response.text().await?;
    println!("Response body:\n{response_text}");

    // Demonstrate using JSONPlaceholder API (a common test API)
    log::info!("\nTrying JSONPlaceholder API...");

    let todo_data = PostData {
        title: "Test TODO item".to_string(),
        user_id: 1,
        completed: false,
    };

    let response = client
        .post("https://jsonplaceholder.typicode.com/todos")
        .json(&todo_data)
        .send()
        .await?;

    log::info!("Response status: {:?}", response.status());

    // Deserialize the JSON response into our struct
    let api_response: ApiResponse = response.json().await?;

    println!("\nDeserialized response:");
    println!("  ID: {}", api_response.id);
    println!("  Title: {}", api_response.title);
    println!("  User ID: {}", api_response.user_id);
    println!("  Completed: {}", api_response.completed);

    Ok(())
}
