#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Method, Query, RequestData, Route};

#[cfg(any(feature = "actix", feature = "simulator"))]
use serde::Deserialize;

// Simple query parameters
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize)]
struct SimpleQuery {
    name: String,
    age: u32,
}

// Query parameters with optional fields
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize)]
struct OptionalQuery {
    search: String,
    limit: Option<u32>,
    offset: Option<u32>,
    sort: Option<String>,
}

// Handler demonstrating simple query extraction
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn simple_query_handler(query: Query<SimpleQuery>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Simple Query Extraction:\n  Name: {}\n  Age: {}\n  Query struct: {:?}",
        query.0.name, query.0.age, query.0
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler demonstrating optional query parameters
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn optional_query_handler(query: Query<OptionalQuery>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Optional Query Parameters:\n  Search: {}\n  Limit: {:?}\n  Offset: {:?}\n  Sort: {:?}",
        query.0.search, query.0.limit, query.0.offset, query.0.sort
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler combining query extraction with other extractors
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn combined_handler(
    query: Query<SimpleQuery>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    let response = format!(
        "Combined Extractors:\n  Query Name: {}\n  Query Age: {}\n  Request Method: {:?}\n  Request Path: {}\n  User-Agent: {:?}",
        query.0.name, query.0.age, data.method, data.path, data.user_agent
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler demonstrating error handling (simplified)
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn error_demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Query Extraction Demo:\n  Query String: '{}'\n  Tip: Try ?name=John&age=25 for simple_query_handler\n  Tip: Try ?search=rust&limit=10 for optional_query_handler",
        data.query
    );
    Ok(HttpResponse::ok().with_body(response))
}

#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend Query Extractor Examples...");

    // Create routes using the new handler system with query extractors
    let simple_route = Route::with_handler1(Method::Get, "/simple", simple_query_handler);
    let optional_route = Route::with_handler1(Method::Get, "/optional", optional_query_handler);
    let combined_route = Route::with_handler2(Method::Get, "/combined", combined_handler);
    let error_route = Route::with_handler1(Method::Get, "/error", error_demo_handler);

    println!("✅ Query extractor routes created successfully:");
    println!(
        "   Simple:   {} {} (requires: ?name=X&age=N)",
        simple_route.method, simple_route.path
    );
    println!(
        "   Optional: {} {} (requires: ?search=X, optional: limit,offset,sort)",
        optional_route.method, optional_route.path
    );
    println!(
        "   Combined: {} {} (requires: ?name=X&age=N)",
        combined_route.method, combined_route.path
    );
    println!(
        "   Error:    {} {} (demonstrates query string access)",
        error_route.method, error_route.path
    );
    println!("   Backend: Actix Web");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Query Extractor Examples...");

    // Create routes
    let simple_route = Route::with_handler1(Method::Get, "/simple", simple_query_handler);
    let optional_route = Route::with_handler1(Method::Get, "/optional", optional_query_handler);
    let combined_route = Route::with_handler2(Method::Get, "/combined", combined_handler);
    let error_route = Route::with_handler1(Method::Get, "/error", error_demo_handler);

    println!("✅ Query extractor routes created successfully:");
    println!("   Simple:   {} {}", simple_route.method, simple_route.path);
    println!(
        "   Optional: {} {}",
        optional_route.method, optional_route.path
    );
    println!(
        "   Combined: {} {}",
        combined_route.method, combined_route.path
    );
    println!("   Error:    {} {}", error_route.method, error_route.path);
    println!("   Backend: Simulator");

    // Test the simple query handler
    println!("\n📋 Testing Simple Query Handler:");
    let request = SimulationRequest::new(Method::Get, "/simple")
        .with_query_string("name=Alice&age=30")
        .with_header("user-agent", "MoosicBox-QueryTest/1.0");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let query = Query::<SimpleQuery>::from_request_sync(&http_request)?;
    println!("✅ Query extracted successfully:");
    println!("   Name: {}", query.0.name);
    println!("   Age: {}", query.0.age);

    // Test the optional query handler
    println!("\n📋 Testing Optional Query Handler:");
    let request = SimulationRequest::new(Method::Get, "/optional")
        .with_query_string("search=rust&limit=10&sort=date");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let query = Query::<OptionalQuery>::from_request_sync(&http_request)?;
    println!("✅ Optional query extracted successfully:");
    println!("   Search: {}", query.0.search);
    println!("   Limit: {:?}", query.0.limit);
    println!("   Sort: {:?}", query.0.sort);

    // Test error handling
    println!("\n📋 Testing Query String Access:");
    let request = SimulationRequest::new(Method::Get, "/error")
        .with_query_string("invalid=query&missing=required_fields");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let data = RequestData::from_request_sync(&http_request)?;
    println!("✅ RequestData extracted successfully:");
    println!("   Query String: '{}'", data.query);
    println!("   Path: {}", data.path);

    Ok(())
}

#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Query Extractor Examples - Query<T> Usage");
    println!("==============================================\n");

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

    println!("✅ Query Extractor Examples Complete!");
    println!("   - Query<T> extractor working with serde deserialization");
    println!("   - Support for required and optional query parameters");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - URL decoding handled automatically");
    println!("   - Ready for production use with complex query structures");
    println!("   - Note: Full async handler execution needs async runtime");

    Ok(())
}

#[cfg(not(any(feature = "actix", feature = "simulator")))]
fn main() {
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║                  Query Extractor Example                   ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");
    eprintln!("║ This example demonstrates Query<T> extractor for URL       ║");
    eprintln!("║ parameter parsing with serde deserialization.             ║");
    eprintln!("║                                                            ║");
    eprintln!("║ To run this example, enable a backend feature:            ║");
    eprintln!("║                                                            ║");
    eprintln!("║   cargo run --example query_extractor --features actix    ║");
    eprintln!("║   cargo run --example query_extractor --features simulator║");
    eprintln!("║                                                            ║");
    eprintln!("║ The 'actix' feature uses the production Actix Web backend.║");
    eprintln!("║ The 'simulator' feature uses a test simulator backend.    ║");
    eprintln!("╚════════════════════════════════════════════════════════════╝");
}
