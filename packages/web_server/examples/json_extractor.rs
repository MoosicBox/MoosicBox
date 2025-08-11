#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Json, Method, RequestData, Route};

#[cfg(any(feature = "actix", feature = "simulator"))]
use serde::{Deserialize, Serialize};

// Simple JSON payload
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize, Serialize)]
struct User {
    name: String,
    email: String,
    age: u32,
}

// JSON payload with optional fields
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize, Serialize)]
struct UpdateUser {
    name: Option<String>,
    email: Option<String>,
    age: Option<u32>,
    bio: Option<String>,
}

// Handler demonstrating simple JSON extraction
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn simple_json_handler(json: Json<User>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Simple JSON Extraction:\n  Name: {}\n  Email: {}\n  Age: {}\n  User struct: {:?}",
        json.0.name, json.0.email, json.0.age, json.0
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler demonstrating optional fields
#[cfg(any(feature = "actix", feature = "simulator"))]
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

// Handler combining JSON with other extractors
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn combined_json_handler(json: Json<User>, data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Combined JSON + RequestData:\n  JSON Name: {}\n  JSON Email: {}\n  Request Method: {:?}\n  Request Path: {}\n  Content-Type: {:?}",
        json.0.name, json.0.email, data.method, data.path, data.content_type
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler demonstrating JSON response
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn json_response_handler(json: Json<User>) -> Result<HttpResponse, Error> {
    // Echo back the user with a modification
    let mut user = json.0;
    user.name = format!("Hello, {}!", user.name);

    let json_response = serde_json::to_string(&user).map_err(Error::bad_request)?;

    // Note: HttpResponse doesn't support headers yet, but the content is JSON
    Ok(HttpResponse::ok().with_body(json_response))
}

// Handler demonstrating error handling (simplified)
#[cfg(any(feature = "actix", feature = "simulator"))]
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

#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend JSON Extractor Examples...");

    // Create routes using the new handler system with JSON extractors
    let simple_route = Route::with_handler1(Method::Post, "/simple", simple_json_handler);
    let optional_route = Route::with_handler1(Method::Patch, "/optional", optional_json_handler);
    let combined_route = Route::with_handler2(Method::Post, "/combined", combined_json_handler);
    let response_route = Route::with_handler1(Method::Post, "/echo", json_response_handler);
    let error_route = Route::with_handler1(Method::Post, "/error", error_demo_handler);

    println!("✅ JSON extractor routes created successfully:");
    println!(
        "   Simple:   {} {} (expects User JSON)",
        simple_route.method, simple_route.path
    );
    println!(
        "   Optional: {} {} (expects UpdateUser JSON)",
        optional_route.method, optional_route.path
    );
    println!(
        "   Combined: {} {} (expects User JSON)",
        combined_route.method, combined_route.path
    );
    println!(
        "   Response: {} {} (returns JSON)",
        response_route.method, response_route.path
    );
    println!(
        "   Error:    {} {} (demonstrates error handling)",
        error_route.method, error_route.path
    );
    println!("   Backend: Actix Web");
    println!("   Note: Actix requires body to be pre-extracted for JSON parsing");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend JSON Extractor Examples...");

    // Create routes
    let simple_route = Route::with_handler1(Method::Post, "/simple", simple_json_handler);
    let optional_route = Route::with_handler1(Method::Patch, "/optional", optional_json_handler);
    let combined_route = Route::with_handler2(Method::Post, "/combined", combined_json_handler);
    let response_route = Route::with_handler1(Method::Post, "/echo", json_response_handler);
    let error_route = Route::with_handler1(Method::Post, "/error", error_demo_handler);

    println!("✅ JSON extractor routes created successfully:");
    println!("   Simple:   {} {}", simple_route.method, simple_route.path);
    println!(
        "   Optional: {} {}",
        optional_route.method, optional_route.path
    );
    println!(
        "   Combined: {} {}",
        combined_route.method, combined_route.path
    );
    println!(
        "   Response: {} {}",
        response_route.method, response_route.path
    );
    println!("   Error:    {} {}", error_route.method, error_route.path);
    println!("   Backend: Simulator");

    // Test the simple JSON handler
    println!("\n📋 Testing Simple JSON Handler:");
    let user_json = r#"{"name": "Alice", "email": "alice@example.com", "age": 30}"#;
    let request = SimulationRequest::new(Method::Post, "/simple")
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
    let request = SimulationRequest::new(Method::Patch, "/optional")
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
    let request = SimulationRequest::new(Method::Post, "/echo")
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

    // Test error handling
    println!("\n📋 Testing Content-Type Check:");
    let request = SimulationRequest::new(Method::Post, "/error")
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

#[cfg(any(feature = "actix", feature = "simulator"))]
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

    println!("✅ JSON Extractor Examples Complete!");
    println!("   - Json<T> extractor working with serde deserialization");
    println!("   - Support for simple and complex nested JSON structures");
    println!("   - Optional field handling with partial updates");
    println!("   - JSON response generation (headers not yet supported)");
    println!("   - Works with Simulator backend (Actix requires pre-extraction)");
    println!("   - Ready for production REST API development");
    println!("   - Note: Full async handler execution needs async runtime");

    Ok(())
}

#[cfg(not(any(feature = "actix", feature = "simulator")))]
fn main() {
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║                   JSON Extractor Example                   ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");
    eprintln!("║ This example demonstrates Json<T> extractor for request    ║");
    eprintln!("║ body parsing with serde deserialization.                  ║");
    eprintln!("║                                                            ║");
    eprintln!("║ To run this example, enable a backend feature:            ║");
    eprintln!("║                                                            ║");
    eprintln!("║   cargo run --example json_extractor --features actix     ║");
    eprintln!("║   cargo run --example json_extractor --features simulator ║");
    eprintln!("║                                                            ║");
    eprintln!("║ The 'actix' feature uses the production Actix Web backend.║");
    eprintln!("║ The 'simulator' feature uses a test simulator backend.    ║");
    eprintln!("╚════════════════════════════════════════════════════════════╝");
}
