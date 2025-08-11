#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Method, RequestData, Route};

#[cfg(all(feature = "serde", any(feature = "actix", feature = "simulator")))]
use moosicbox_web_server::{Json, Query};

#[cfg(any(feature = "actix", feature = "simulator"))]
use serde::{Deserialize, Serialize};

// Query parameters for search
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
    offset: Option<u32>,
}

// JSON payload for user updates
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Deserialize, Serialize)]
struct UserUpdate {
    name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
}

// Response structure
#[cfg(any(feature = "actix", feature = "simulator"))]
#[derive(Debug, Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

// Handler combining Query + RequestData
#[cfg(all(feature = "serde", any(feature = "actix", feature = "simulator")))]
async fn search_handler(
    query: Query<SearchQuery>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    let response = ApiResponse {
        success: true,
        message: format!(
            "Search executed: '{}' (limit: {:?}, offset: {:?})",
            query.0.q, query.0.limit, query.0.offset
        ),
        data: Some(serde_json::json!({
            "search_term": query.0.q,
            "limit": query.0.limit.unwrap_or(10),
            "offset": query.0.offset.unwrap_or(0),
            "request_method": format!("{:?}", data.method),
            "request_path": data.path,
            "user_agent": data.user_agent
        })),
    };

    let json_response = serde_json::to_string(&response).map_err(Error::bad_request)?;
    Ok(HttpResponse::ok().with_body(json_response))
}

// Handler combining Json + RequestData
#[cfg(all(feature = "serde", any(feature = "actix", feature = "simulator")))]
async fn update_handler(json: Json<UserUpdate>, data: RequestData) -> Result<HttpResponse, Error> {
    let response = ApiResponse {
        success: true,
        message: "User updated successfully".to_string(),
        data: Some(serde_json::json!({
            "updates": {
                "name": json.0.name,
                "email": json.0.email,
                "bio": json.0.bio
            },
            "request_info": {
                "method": format!("{:?}", data.method),
                "path": data.path,
                "content_type": data.content_type,
                "remote_addr": data.remote_addr
            }
        })),
    };

    let json_response = serde_json::to_string(&response).map_err(Error::bad_request)?;
    Ok(HttpResponse::ok().with_body(json_response))
}

// Simple handler with just RequestData
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn info_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let response = ApiResponse {
        success: true,
        message: "Request information extracted".to_string(),
        data: Some(serde_json::json!({
            "method": format!("{:?}", data.method),
            "path": data.path,
            "query": data.query,
            "headers_count": data.headers.len(),
            "user_agent": data.user_agent,
            "content_type": data.content_type,
            "remote_addr": data.remote_addr
        })),
    };

    let json_response = serde_json::to_string(&response).map_err(Error::bad_request)?;
    Ok(HttpResponse::ok().with_body(json_response))
}

// Handler with two RequestData extractors (for demonstration)
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn double_data_handler(
    data1: RequestData,
    data2: RequestData,
) -> Result<HttpResponse, Error> {
    let response = ApiResponse {
        success: true,
        message: "Double RequestData extraction".to_string(),
        data: Some(serde_json::json!({
            "data1_method": format!("{:?}", data1.method),
            "data2_method": format!("{:?}", data2.method),
            "path": data1.path,
            "query": data1.query,
            "same_data": data1.method == data2.method
        })),
    };

    let json_response = serde_json::to_string(&response).map_err(Error::bad_request)?;
    Ok(HttpResponse::ok().with_body(json_response))
}

#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend Combined Extractor Examples...");

    // Create routes with multiple extractors
    #[cfg(feature = "serde")]
    let search_route = Route::with_handler2(Method::Get, "/search", search_handler);
    #[cfg(feature = "serde")]
    let update_route = Route::with_handler2(Method::Put, "/update", update_handler);
    let info_route = Route::with_handler1(Method::Get, "/info", info_handler);
    let double_route = Route::with_handler2(Method::Get, "/double", double_data_handler);

    println!("✅ Combined extractor routes created successfully:");
    #[cfg(feature = "serde")]
    println!(
        "   Search: {} {} (Query + RequestData)",
        search_route.method, search_route.path
    );
    #[cfg(feature = "serde")]
    println!(
        "   Update: {} {} (Json + RequestData)",
        update_route.method, update_route.path
    );
    println!(
        "   Info:   {} {} (RequestData only)",
        info_route.method, info_route.path
    );
    println!(
        "   Double: {} {} (RequestData + RequestData)",
        double_route.method, double_route.path
    );
    println!("   Backend: Actix Web");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Combined Extractor Examples...");

    // Create routes
    #[cfg(feature = "serde")]
    let search_route = Route::with_handler2(Method::Get, "/search", search_handler);
    #[cfg(feature = "serde")]
    let update_route = Route::with_handler2(Method::Put, "/update", update_handler);
    let info_route = Route::with_handler1(Method::Get, "/info", info_handler);
    let double_route = Route::with_handler2(Method::Get, "/double", double_data_handler);

    println!("✅ Combined extractor routes created successfully:");
    #[cfg(feature = "serde")]
    println!("   Search: {} {}", search_route.method, search_route.path);
    #[cfg(feature = "serde")]
    println!("   Update: {} {}", update_route.method, update_route.path);
    println!("   Info:   {} {}", info_route.method, info_route.path);
    println!("   Double: {} {}", double_route.method, double_route.path);
    println!("   Backend: Simulator");

    // Test the info handler (RequestData only)
    println!("\n📋 Testing Info Handler (RequestData only):");
    let request = SimulationRequest::new(Method::Get, "/info")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-CombinedTest/1.0")
        .with_header("content-type", "application/json");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let data = RequestData::from_request_sync(&http_request)?;
    println!("✅ RequestData extracted successfully:");
    println!("   Method: {:?}", data.method);
    println!("   Path: {}", data.path);
    println!("   Query: {}", data.query);
    println!("   Headers: {}", data.headers.len());

    // Test the double data handler (RequestData + RequestData)
    println!("\n📋 Testing Double Data Handler (RequestData + RequestData):");
    let request = SimulationRequest::new(Method::Get, "/double")
        .with_query_string("param1=value1&param2=value2")
        .with_header("user-agent", "MoosicBox-DoubleTest/1.0");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    let data1 = RequestData::from_request_sync(&http_request)?;
    let data2 = RequestData::from_request_sync(&http_request)?;
    println!("✅ Double RequestData extracted successfully:");
    println!("   Data1 Method: {:?}", data1.method);
    println!("   Data2 Method: {:?}", data2.method);
    println!("   Same data: {}", data1.method == data2.method);

    // Test the search handler (Query + RequestData) if serde is available
    #[cfg(feature = "serde")]
    {
        println!("\n📋 Testing Search Handler (Query + RequestData):");
        let request = SimulationRequest::new(Method::Get, "/search")
            .with_query_string("q=rust+web+server&limit=20&offset=10")
            .with_header("user-agent", "MoosicBox-CombinedTest/1.0");

        let stub = SimulationStub::new(request);
        let http_request =
            moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));
        let query = Query::<SearchQuery>::from_request_sync(&http_request)?;
        let data = RequestData::from_request_sync(&http_request)?;
        println!("✅ Query + RequestData extracted successfully:");
        println!("   Search term: {}", query.0.q);
        println!("   Limit: {:?}", query.0.limit);
        println!("   Request method: {:?}", data.method);
        println!("   User agent: {:?}", data.user_agent);
    }

    Ok(())
}

#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Combined Extractors Examples - Multiple Extractors Together");
    println!("==============================================================\n");

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

    println!("✅ Combined Extractors Examples Complete!");
    println!("   - Multiple extractors working together (up to 2 parameters currently)");
    println!("   - Query + RequestData combinations");
    println!("   - Json + RequestData combinations");
    println!("   - RequestData + RequestData combinations");
    println!("   - RequestData extraction working standalone");
    println!("   - JSON API response patterns");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - Note: More complex combinations need 3+ parameter handler support");

    Ok(())
}

#[cfg(not(any(feature = "actix", feature = "simulator")))]
fn main() {
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║                Combined Extractors Example                 ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");
    eprintln!("║ This example demonstrates multiple extractors working      ║");
    eprintln!("║ together in handler functions.                            ║");
    eprintln!("║                                                            ║");
    eprintln!("║ To run this example, enable a backend feature:            ║");
    eprintln!("║                                                            ║");
    eprintln!("║   cargo run --example combined_extractors --features actix║");
    eprintln!("║   cargo run --example combined_extractors --features simulator║");
    eprintln!("║                                                            ║");
    eprintln!("║ The 'actix' feature uses the production Actix Web backend.║");
    eprintln!("║ The 'simulator' feature uses a test simulator backend.    ║");
    eprintln!("╚════════════════════════════════════════════════════════════╝");
}
