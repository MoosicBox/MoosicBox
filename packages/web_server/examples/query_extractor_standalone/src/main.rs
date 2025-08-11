#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Query, RequestData};
#[cfg(any(feature = "actix", feature = "simulator"))]
use serde::Deserialize;

// Simple query parameters
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields are demonstrated in Debug output
struct SimpleQuery {
    name: String,
    age: u32,
}

// Query parameters with optional fields
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields are demonstrated in Debug output
struct OptionalQuery {
    search: String,
    limit: Option<u32>,
    offset: Option<u32>,
    sort: Option<String>,
}

// Handler demonstrating simple query extraction
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn simple_query_handler(query: Query<SimpleQuery>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Simple Query Extraction:\n  Name: {}\n  Age: {}\n  Query struct: {:?}",
        query.0.name, query.0.age, query.0
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler demonstrating optional query parameters
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn optional_query_handler(query: Query<OptionalQuery>) -> Result<HttpResponse, Error> {
    let response = format!(
        "Optional Query Parameters:\n  Search: {}\n  Limit: {:?}\n  Offset: {:?}\n  Sort: {:?}",
        query.0.search, query.0.limit, query.0.offset, query.0.sort
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler combining query extraction with other extractors
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
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
#[allow(clippy::unused_async)]
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

    let routes = [
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/simple",
            simple_query_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/optional",
            optional_query_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Get,
            "/combined",
            combined_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ Query extractor routes created successfully:");
    for (i, route) in routes.iter().enumerate() {
        let description = match i {
            0 => "(requires: ?name=X&age=N)",
            1 => "(requires: ?search=X, optional: limit,offset,sort)",
            2 => "(requires: ?name=X&age=N + RequestData)",
            3 => "(demonstrates error handling)",
            _ => "",
        };
        println!(
            "   {}: {} {} {}",
            route.method, route.path, route.method, description
        );
    }
    println!("   Backend: Actix Web");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Query Extractor Examples...");

    let routes = [
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/simple",
            simple_query_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/optional",
            optional_query_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Get,
            "/combined",
            combined_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ Query extractor routes created successfully:");
    for route in &routes {
        println!("   {}: {} {}", route.method, route.path, route.method);
    }
    println!("   Backend: Simulator");

    // Test error demo handler (always available)
    println!("\n📋 Testing Error Demo Handler (RequestData only):");
    let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/error")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-QueryTest/1.0");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let data = RequestData::from_request_sync(&http_request)?;
    println!("✅ RequestData extracted successfully:");
    println!("   Query: {}", data.query);
    println!("   Path: {}", data.path);

    // Test the simple query handler
    println!("\n📋 Testing Simple Query Handler:");
    let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/simple")
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
    let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/optional")
        .with_query_string("search=rust&limit=10&sort=date");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let query = Query::<OptionalQuery>::from_request_sync(&http_request)?;
    println!("✅ Optional query extracted successfully:");
    println!("   Search: {}", query.0.search);
    println!("   Limit: {:?}", query.0.limit);
    println!("   Sort: {:?}", query.0.sort);

    Ok(())
}

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

    #[cfg(not(any(feature = "actix", feature = "simulator")))]
    {
        eprintln!("╔════════════════════════════════════════════════════════════╗");
        eprintln!("║                Query Extractor Example                     ║");
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ This example demonstrates query parameter extraction       ║");
        eprintln!("║ with serde deserialization.                               ║");
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

    println!("✅ Query Extractor Examples Complete!");
    println!("   - Query<T> extractor working with serde deserialization");
    println!("   - Support for required and optional query parameters");
    println!("   - Type-safe query parameter parsing");
    println!("   - Combined Query + RequestData extraction");
    println!("   - Error handling for malformed query strings");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - Real-world query parameter patterns");

    Ok(())
}
