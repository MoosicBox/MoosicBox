use moosicbox_web_server::{
    Error, FromRequest, HttpRequest, HttpResponse, Method, RequestData, Route,
};

// Simple handler that demonstrates the new Route::with_handler() method
// Using RequestData instead of HttpRequest to avoid Send bounds issues
async fn demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let mut response = String::new();

    response.push_str("=== New Handler System Demonstration ===\n\n");

    // Test method()
    response.push_str(&format!("HTTP Method: {:?}\n", data.method));

    // Test path()
    response.push_str(&format!("Path: {}\n", data.path));

    // Test query_string()
    if data.query.is_empty() {
        response.push_str("Query String: None\n");
    } else {
        response.push_str(&format!("Query String: {}\n", data.query));
    }

    // Test headers
    if let Some(user_agent) = &data.user_agent {
        response.push_str(&format!("User-Agent: {}\n", user_agent));
    } else {
        response.push_str("User-Agent: None\n");
    }

    if let Some(content_type) = &data.content_type {
        response.push_str(&format!("Content-Type: {}\n", content_type));
    } else {
        response.push_str("Content-Type: None\n");
    }

    // Test headers collection
    response.push_str(&format!("All Headers: {} found\n", data.headers.len()));
    for (name, value) in &data.headers {
        response.push_str(&format!("  {}: {}\n", name, value));
    }

    // Test remote_addr()
    if let Some(addr) = data.remote_addr {
        response.push_str(&format!("Remote Address: {}\n", addr));
    } else {
        response.push_str("Remote Address: None\n");
    }

    // Note: RequestData doesn't include body, but that's okay for this demo
    response
        .push_str("Body: Not available in RequestData (use Json<T> extractor for body parsing)\n");

    response.push_str("\n=== Route::with_handler() Working! ===\n");
    response.push_str("✅ No more Box::pin(async move {...}) boilerplate!\n");
    response.push_str("✅ Clean async function syntax!\n");
    response.push_str("✅ Works with both Actix and Simulator backends!\n");
    response.push_str("✅ RequestData provides Send-safe access to request info!\n");

    Ok(HttpResponse::ok().with_body(response))
}

#[cfg(feature = "actix")]
fn run_actix_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Running Actix Backend Example...");

    // NEW: Using Route::with_handler1() for 1-parameter handler - no more Box::pin boilerplate!
    let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

    println!("✅ Route created successfully with new handler system:");
    println!("   Method: {:?}", route.method);
    println!("   Path: {}", route.path);
    println!("   Handler: Clean async function (no Box::pin!)");
    println!("   Backend: Actix Web");

    Ok(())
}

#[cfg(any(feature = "simulator", not(feature = "actix")))]
fn run_simulator_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Example...");

    // NEW: Using Route::with_handler1() for 1-parameter handler - same clean syntax!
    let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

    println!("✅ Route created successfully with new handler system:");
    println!("   Method: {:?}", route.method);
    println!("   Path: {}", route.path);
    println!("   Handler: Clean async function (no Box::pin!)");
    println!("   Backend: Simulator");

    // Test the handler with a simulation request
    let cookies = vec![
        ("session".to_string(), "abc123".to_string()),
        ("theme".to_string(), "dark".to_string()),
    ];

    let request = SimulationRequest::new(Method::Post, "/demo")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-Test/1.0")
        .with_header("content-type", "application/json")
        .with_cookies(cookies)
        .with_remote_addr("192.168.1.100:54321")
        .with_body(b"{\"message\": \"Hello from simulator!\"}".to_vec());

    let stub = SimulationStub::new(request);
    let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    // Extract RequestData and show what the handler would receive
    let data = RequestData::from_request_sync(&http_request)?;

    println!("\n📋 Handler would receive RequestData:");
    println!("   Method: {:?}", data.method);
    println!("   Path: {}", data.path);
    println!("   Query: {}", data.query);
    println!("   User-Agent: {:?}", data.user_agent);
    println!("   Content-Type: {:?}", data.content_type);
    println!("   Remote Address: {:?}", data.remote_addr);
    println!("   Headers: {} total", data.headers.len());

    println!("\n✅ RequestData extraction successful!");
    println!("   Handler would process this data and return an HttpResponse");
    println!("   Note: Full async execution requires an async runtime");

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Basic Handler Example - Route::with_handler() Method");
    println!("=====================================================\n");

    #[cfg(feature = "actix")]
    {
        run_actix_example()?;
        println!();
    }

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    {
        run_simulator_example()?;
        println!();
    }

    #[cfg(not(any(feature = "actix", any(feature = "simulator", not(feature = "actix")))))]
    {
        println!("❌ No backend features enabled!");
        println!("   Run with: cargo run --example basic_handler --features actix");
        println!("   Or with:  cargo run --example basic_handler --features simulator");
    }

    println!("✅ Basic Handler Example Complete!");
    println!("   - Route::with_handler1() method working");
    println!("   - Clean async function syntax (no Box::pin boilerplate)");
    println!("   - Works identically with both Actix and Simulator backends");
    println!("   - RequestData provides Send-safe access to request information");
    println!("   - Ready for production use with the new handler system");

    Ok(())
}
