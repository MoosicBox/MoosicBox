//! JSON extractor example demonstrating `Json<T>` usage with serde deserialization.
//!
//! This example shows how to use the `Json<T>` extractor from `moosicbox_web_server`
//! to automatically deserialize JSON request bodies into strongly-typed Rust structs
//! using serde. It demonstrates common patterns for building JSON APIs including
//! simple payloads, optional fields, combined extractors, and JSON responses.
//!
//! # Features
//!
//! * `actix` - Run with Actix Web backend
//! * `simulator` - Run with Simulator backend (default)
//! * `serde` - Enable JSON serialization/deserialization support (default)
//!
//! # Demonstrated Patterns
//!
//! This example demonstrates:
//!
//! * Basic JSON extraction with `Json<T>` for simple structured payloads
//! * Handling optional fields with `Option<T>` for partial updates
//! * Combining `Json<T>` with `RequestData` extractors for metadata access
//! * Generating JSON responses with `serde_json::to_string`
//! * Content-Type validation and error handling best practices
//! * Backend-agnostic JSON API creation (works with Actix and Simulator)
//!
//! # Usage
//!
//! Run with the simulator backend:
//! ```text
//! cargo run --package json_extractor_standalone_example
//! ```
//!
//! Run with the Actix backend:
//! ```text
//! cargo run --package json_extractor_standalone_example --features actix
//! ```
//!
//! # Handler Examples
//!
//! The example includes several handler functions demonstrating different patterns:
//!
//! * `simple_json_handler` - Basic JSON extraction with required fields
//! * `optional_json_handler` - Handling partial updates with optional fields
//! * `combined_json_handler` - Combining JSON with other extractors
//! * `json_response_handler` - Generating JSON responses
//! * `error_demo_handler` - Content-Type validation and error handling

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Json, RequestData};
#[cfg(any(feature = "actix", feature = "simulator"))]
use serde::{Deserialize, Serialize};

/// Simple JSON payload representing a user with required fields.
///
/// This struct is used to demonstrate basic JSON extraction with the `Json<T>` extractor.
/// All fields are required and deserialization will fail if any field is missing or has
/// an invalid type.
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize, Serialize)]
struct User {
    /// The user's full name.
    name: String,
    /// The user's email address.
    email: String,
    /// The user's age in years.
    age: u32,
}

/// JSON payload for partial user updates with optional fields.
///
/// This struct demonstrates handling partial updates where any combination of fields
/// can be present or absent in the JSON request. All fields are wrapped in `Option<T>`
/// to allow for flexible updates without requiring all fields to be present.
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize, Serialize)]
struct UpdateUser {
    /// Optional updated user name.
    name: Option<String>,
    /// Optional updated email address.
    email: Option<String>,
    /// Optional updated age in years.
    age: Option<u32>,
    /// Optional user biography or description.
    bio: Option<String>,
}

/// Handler demonstrating simple JSON extraction with required fields.
///
/// This handler accepts a JSON payload that must contain all required fields
/// (name, email, age) and demonstrates basic usage of the `Json<T>` extractor.
///
/// # Errors
///
/// * `Error::BadRequest` - If the request body is not valid JSON
/// * `Error::BadRequest` - If required fields are missing from the JSON
/// * `Error::BadRequest` - If field types don't match (e.g., age is not a number)
/// * `Error::BadRequest` - If Content-Type is not application/json
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn simple_json_handler(json: Json<User>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Simple JSON Extraction:\n  Name: {}\n  Email: {}\n  Age: {}\n  User struct: {:?}",
        json.0.name, json.0.email, json.0.age, json.0
    );
    Ok(HttpResponse::ok().with_body(response))
}

/// Handler demonstrating optional field handling for partial updates.
///
/// This handler accepts JSON payloads with any combination of optional fields,
/// making it suitable for PATCH operations where only modified fields are sent.
/// Any fields not present in the JSON will be `None` in the deserialized struct.
///
/// # Errors
///
/// * `Error::BadRequest` - If the request body is not valid JSON
/// * `Error::BadRequest` - If field types don't match expected types
/// * `Error::BadRequest` - If Content-Type is not application/json
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn optional_json_handler(json: Json<UpdateUser>) -> Result<HttpResponse, Error> {
    let mut updates = Vec::new();

    if let Some(name) = &json.0.name {
        updates.push(format!("name -> {name}"));
    }
    if let Some(email) = &json.0.email {
        updates.push(format!("email -> {email}"));
    }
    if let Some(age) = json.0.age {
        updates.push(format!("age -> {age}"));
    }
    if let Some(bio) = &json.0.bio {
        updates.push(format!("bio -> {} chars", bio.len()));
    }

    let response = format!(
        "Optional JSON Fields:\n  Updates: [{}]\n  Full struct: {:?}",
        updates.join(", "),
        json.0
    );
    Ok(HttpResponse::ok().with_body(response))
}

/// Handler demonstrating combining JSON extraction with other extractors.
///
/// This handler shows how to use multiple extractors in a single handler function.
/// It extracts both JSON payload data and request metadata, demonstrating that
/// extractors can be composed to access different aspects of the HTTP request.
///
/// # Errors
///
/// * `Error::BadRequest` - If the request body is not valid JSON
/// * `Error::BadRequest` - If required JSON fields are missing
/// * `Error::BadRequest` - If field types don't match expected types
/// * `Error::BadRequest` - If Content-Type is not application/json
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn combined_json_handler(json: Json<User>, data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Combined JSON + RequestData:\n  JSON Name: {}\n  JSON Email: {}\n  Request Method: {:?}\n  Request Path: {}\n  Content-Type: {:?}",
        json.0.name, json.0.email, data.method, data.path, data.content_type
    );
    Ok(HttpResponse::ok().with_body(response))
}

/// Handler demonstrating JSON response generation.
///
/// This handler shows how to generate JSON responses using `serde_json::to_string`.
/// It accepts a JSON payload, modifies it, and returns the modified data as JSON.
/// This pattern is common in REST APIs for echo, transform, or enrichment endpoints.
///
/// # Errors
///
/// * `Error::BadRequest` - If the request body is not valid JSON
/// * `Error::BadRequest` - If required JSON fields are missing
/// * `Error::BadRequest` - If field types don't match expected types
/// * `Error::BadRequest` - If Content-Type is not application/json
/// * `Error::BadRequest` - If serializing the response to JSON fails
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn json_response_handler(json: Json<User>) -> Result<HttpResponse, Error> {
    // Echo back the user with a modification
    let mut user = json.0;
    user.name = format!("Hello, {}!", user.name);

    let json_response = serde_json::to_string(&user).map_err(Error::bad_request)?;

    // Note: HttpResponse doesn't support headers yet, but the content is JSON
    Ok(HttpResponse::ok().with_body(json_response))
}

/// Handler demonstrating Content-Type validation and error handling.
///
/// This simplified handler only uses `RequestData` to demonstrate Content-Type
/// checking without actually parsing JSON. It shows how to validate request
/// headers and provide helpful error messages to API clients.
///
/// # Errors
///
/// This handler currently doesn't return errors, but a production version might return:
/// * `Error::BadRequest` - If Content-Type header is missing or invalid
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn error_demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let body_info = if data.content_type.as_deref() == Some("application/json") {
        "Content-Type: application/json (good!)"
    } else {
        "Content-Type: not application/json (may cause issues)"
    };

    let response = format!(
        "JSON Extraction Demo:\n  {}\n  Path: {}\n  Tip: Send valid JSON with name, email, and age fields\n  Tip: Set Content-Type: application/json header",
        body_info, data.path
    );
    Ok(HttpResponse::ok().with_body(response))
}

/// Runs examples using the Actix Web backend.
///
/// Creates route definitions demonstrating various JSON extraction patterns
/// and prints information about each route to the console. This function
/// is only compiled when the `actix` feature is enabled.
#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend JSON Extractor Examples...");

    let routes = [
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/simple",
            simple_json_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Patch,
            "/optional",
            optional_json_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Post,
            "/combined",
            combined_json_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/echo",
            json_response_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ JSON extractor routes created successfully:");
    for (i, route) in routes.iter().enumerate() {
        let description = match i {
            0 => "(expects User JSON)",
            1 => "(expects UpdateUser JSON)",
            2 => "(expects User JSON + RequestData)",
            3 => "(returns JSON)",
            4 => "(demonstrates error handling)",
            _ => "",
        };
        println!(
            "   {}: {} {} {}",
            route.method, route.path, route.method, description
        );
    }
    println!("   Backend: Actix Web");
    println!("   Note: Actix requires body to be pre-extracted for JSON parsing");
}

/// Runs examples using the Simulator backend.
///
/// Creates test requests and demonstrates JSON extraction using the simulator
/// backend. This function actually executes the extraction logic by creating
/// simulated HTTP requests and processing them through the `FromRequest` trait.
/// This function is only compiled when the `simulator` feature is enabled and
/// `actix` is not.
///
/// # Errors
///
/// * Returns error if JSON extraction fails during any test case
/// * Returns error if JSON deserialization fails for test payloads
#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend JSON Extractor Examples...");

    let routes = [
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/simple",
            simple_json_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Patch,
            "/optional",
            optional_json_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Post,
            "/combined",
            combined_json_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/echo",
            json_response_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Post,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ JSON extractor routes created successfully:");
    for route in &routes {
        println!("   {}: {} {}", route.method, route.path, route.method);
    }
    println!("   Backend: Simulator");

    // Test the simple JSON handler
    println!("\n📋 Testing Simple JSON Handler:");
    let user_json = r#"{"name": "Alice", "email": "alice@example.com", "age": 30}"#;
    let request = SimulationRequest::new(moosicbox_web_server::Method::Post, "/simple")
        .with_header("content-type", "application/json")
        .with_body(user_json.as_bytes().to_vec());

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let json = Json::<User>::from_request_sync(&http_request)?;
    println!("✅ JSON extracted successfully:");
    println!("   Name: {}", json.0.name);
    println!("   Email: {}", json.0.email);
    println!("   Age: {}", json.0.age);

    // Test the optional JSON handler
    println!("\n📋 Testing Optional JSON Handler:");
    let update_json = r#"{"name": "Bob Updated", "bio": "New bio text"}"#;
    let request = SimulationRequest::new(moosicbox_web_server::Method::Patch, "/optional")
        .with_header("content-type", "application/json")
        .with_body(update_json.as_bytes().to_vec());

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let json = Json::<UpdateUser>::from_request_sync(&http_request)?;
    println!("✅ Optional JSON extracted successfully:");
    println!("   Name: {:?}", json.0.name);
    println!("   Email: {:?}", json.0.email);
    println!("   Age: {:?}", json.0.age);
    println!("   Bio: {:?}", json.0.bio);

    // Test JSON response handler
    println!("\n📋 Testing JSON Response Handler:");
    let user_json = r#"{"name": "Charlie", "email": "charlie@example.com", "age": 35}"#;
    let request = SimulationRequest::new(moosicbox_web_server::Method::Post, "/echo")
        .with_header("content-type", "application/json")
        .with_body(user_json.as_bytes().to_vec());

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let json = Json::<User>::from_request_sync(&http_request)?;
    println!("✅ JSON for response extracted successfully:");
    println!("   Original Name: {}", json.0.name);
    println!(
        "   (Response would modify name to 'Hello, {}!')",
        json.0.name
    );
    println!("   Note: HttpResponse doesn't support headers yet");

    // Test error demo handler (RequestData only)
    println!("\n📋 Testing Error Demo Handler (RequestData only):");
    let request = SimulationRequest::new(moosicbox_web_server::Method::Post, "/error")
        .with_header("content-type", "text/plain")
        .with_body(b"not json".to_vec());

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let data = RequestData::from_request_sync(&http_request)?;
    println!("✅ RequestData extracted successfully:");
    println!("   Content-Type: {:?}", data.content_type);
    println!("   Path: {}", data.path);

    Ok(())
}

/// Entry point for the JSON extractor examples.
///
/// Runs the appropriate backend examples based on which features are enabled.
/// Prints usage information if no backend features are enabled.
///
/// # Errors
///
/// * Returns error if simulator backend tests fail (when `simulator` feature is enabled)
#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 JSON Extractor Examples - Json<T> Usage");
    println!("===========================================\n");

    #[cfg(feature = "actix")]
    {
        run_actix_examples();
        println!();
    }

    #[cfg(feature = "simulator")]
    #[cfg(not(feature = "actix"))]
    {
        run_simulator_examples()?;
        println!();
    }

    #[cfg(not(any(feature = "actix", feature = "simulator")))]
    {
        eprintln!("╔════════════════════════════════════════════════════════════╗");
        eprintln!("║                JSON Extractor Example                      ║");
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ This example demonstrates JSON extraction with serde       ║");
        eprintln!("║ deserialization and JSON response generation.             ║");
        eprintln!("║                                                            ║");
        eprintln!("║ To run this example, enable a backend feature:            ║");
        eprintln!("║                                                            ║");
        eprintln!("║   cargo run --features actix                              ║");
        eprintln!("║   cargo run --features simulator                          ║");
        eprintln!("║                                                            ║");
        eprintln!("║ The 'actix' feature uses the production Actix Web backend.║");
        eprintln!("║ The 'simulator' feature uses a test simulator backend.    ║");
        eprintln!("╚════════════════════════════════════════════════════════════╝");
    }

    println!("✅ JSON Extractor Examples Complete!");
    println!("   - Json<T> extractor working with serde deserialization");
    println!("   - Support for simple and complex nested JSON structures");
    println!("   - Optional field handling with partial updates");
    println!("   - JSON response generation with serde_json");
    println!("   - Combined JSON + RequestData extraction");
    println!("   - Error handling and content-type validation");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - Real-world JSON API patterns");

    Ok(())
}
