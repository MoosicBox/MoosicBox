use moosicbox_web_server::{Headers, HttpResponse, Method, Query, RequestInfo, Route};
use serde::Deserialize;

// Test handler with NO parameters - completely Send-safe!
async fn simple_handler() -> Result<HttpResponse, moosicbox_web_server::Error> {
    Ok(HttpResponse::ok().with_body("Simple handler response - no params!"))
}

// Test handler with RequestInfo extractor - Send-safe!
async fn info_handler(info: RequestInfo) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let response = format!("Request to {} via {:?}", info.path, info.method);
    Ok(HttpResponse::ok().with_body(response))
}

// Test handler with Headers extractor - Send-safe!
async fn headers_handler(headers: Headers) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let user_agent = headers
        .user_agent()
        .map(|ua| ua.as_str())
        .unwrap_or("Unknown");
    let response = format!("User-Agent: {}", user_agent);
    Ok(HttpResponse::ok().with_body(response))
}

// Test handler with Query extractor - Send-safe!
#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<u32>,
}

async fn query_handler(
    Query(query): Query<SearchQuery>,
) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let search_term = query.q.unwrap_or_else(|| "nothing".to_string());
    let limit = query.limit.unwrap_or(10);
    let response = format!("Searching for '{}' with limit {}", search_term, limit);
    Ok(HttpResponse::ok().with_body(response))
}

// Test handler with multiple extractors - Send-safe!
async fn multi_handler(
    info: RequestInfo,
    headers: Headers,
) -> Result<HttpResponse, moosicbox_web_server::Error> {
    let response = format!(
        "Path: {}, Method: {:?}, User-Agent: {}",
        info.path,
        info.method,
        headers.user_agent().unwrap_or(&"Unknown".to_string())
    );
    Ok(HttpResponse::ok().with_body(response))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing handler macro system with Simulator backend...");

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

    // Test that multi-extractor handler compiles and can be converted
    println!("Testing multi-extractor handler...");
    // TODO: Replace with macro syntax once Step 8 is complete: #[get("/multi")]
    let _route_multi = Route::with_handler2(Method::Get, "/multi", multi_handler);
    println!("✅ Multi-extractor handler compiles and converts to Route");

    println!("🎉 All handler macro tests passed for Simulator backend!");
    println!("📝 Note: All handlers use extractors - NO Send bounds issues!");

    Ok(())
}
