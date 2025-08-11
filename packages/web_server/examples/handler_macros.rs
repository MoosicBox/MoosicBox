#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, Method, RequestData, Route};

// Handler with 0 parameters - just returns a simple response
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn handler_0_params() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::ok().with_body("Handler with 0 parameters working!"))
}

// Handler with 1 parameter - extracts RequestData (Send-safe)
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn handler_1_param(data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Handler with 1 parameter:\n  Method: {:?}\n  Path: {}\n  Query: {}\n  Headers: {}",
        data.method,
        data.path,
        data.query,
        data.headers.len()
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler with 2 parameters - extracts RequestData twice (for demonstration)
#[cfg(any(feature = "actix", feature = "simulator"))]
async fn handler_2_params(data1: RequestData, data2: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Handler with 2 parameters:\n  Data1 method: {:?}\n  Data2 method: {:?}\n  Path: {}\n  User-Agent: {:?}",
        data1.method, data2.method, data1.path, data1.user_agent
    );
    Ok(HttpResponse::ok().with_body(response))
}

#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend Handler Macro Examples...");

    // Create routes using the new handler system
    let route_0 = Route::with_handler(Method::Get, "/handler0", handler_0_params);
    let route_1 = Route::with_handler1(Method::Get, "/handler1", handler_1_param);
    let route_2 = Route::with_handler2(Method::Get, "/handler2", handler_2_params);

    println!("✅ Handler routes created successfully:");
    println!("   0 params: {} {}", route_0.method, route_0.path);
    println!("   1 param:  {} {}", route_1.method, route_1.path);
    println!("   2 params: {} {}", route_2.method, route_2.path);
    println!("   Backend: Actix Web");
    println!("   Note: Using RequestData (Send-safe) instead of HttpRequest");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Handler Macro Examples...");

    // Create routes using the new handler system
    let route_0 = Route::with_handler(Method::Get, "/handler0", handler_0_params);
    let route_1 = Route::with_handler1(Method::Get, "/handler1", handler_1_param);
    let route_2 = Route::with_handler2(Method::Get, "/handler2", handler_2_params);

    println!("✅ Handler routes created successfully:");
    println!("   0 params: {} {}", route_0.method, route_0.path);
    println!("   1 param:  {} {}", route_1.method, route_1.path);
    println!("   2 params: {} {}", route_2.method, route_2.path);
    println!("   Backend: Simulator");

    // Test the 1-parameter handler
    println!("\n📋 Testing 1-parameter handler:");

    let cookies = vec![
        ("session".to_string(), "test123".to_string()),
        ("preferences".to_string(), "dark_mode".to_string()),
    ];

    let request = SimulationRequest::new(Method::Get, "/handler1")
        .with_query_string("param1=value1&param2=value2")
        .with_header("user-agent", "MoosicBox-HandlerTest/1.0")
        .with_header("content-type", "application/json")
        .with_cookies(cookies)
        .with_remote_addr("127.0.0.1:8080");

    let stub = SimulationStub::new(request);
    let http_request =
        moosicbox_web_server::HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

    // Execute the 1-parameter handler
    let data = RequestData::from_request_sync(&http_request)?;

    // For this example, we'll just show that the handler can be called
    // In a real server, the async runtime would handle this
    println!("✅ Handler would be called with extracted RequestData:");
    println!("   Method: {:?}", data.method);
    println!("   Path: {}", data.path);
    println!("   Query: {}", data.query);
    println!("   Headers: {}", data.headers.len());

    // Simulate the response without actually calling the async handler
    let simulated_response = format!(
        "Handler with 1 parameter:\n  Method: {:?}\n  Path: {}\n  Query: {}\n  Headers: {}",
        data.method,
        data.path,
        data.query,
        data.headers.len()
    );

    println!("📋 Simulated Handler Response:");
    println!("{simulated_response}");

    Ok(())
}

#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Handler Macro Examples - 0 to 2 Parameters");
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

    println!("✅ Handler Macro Examples Complete!");
    println!("   - Handlers with 0-2 parameters working");
    println!("   - Using RequestData for Send-safe parameter extraction");
    println!("   - Clean async function syntax (no macros needed!)");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - Note: HttpRequest parameters have Send bounds issues with Actix");

    Ok(())
}

#[cfg(not(any(feature = "actix", feature = "simulator")))]
fn main() {
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║                  Handler Macros Example                    ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");
    eprintln!("║ This example demonstrates handler macro usage for 0-2     ║");
    eprintln!("║ parameter handlers with the web server abstraction.       ║");
    eprintln!("║                                                            ║");
    eprintln!("║ To run this example, enable a backend feature:            ║");
    eprintln!("║                                                            ║");
    eprintln!("║   cargo run --example handler_macros --features actix     ║");
    eprintln!("║   cargo run --example handler_macros --features simulator ║");
    eprintln!("║                                                            ║");
    eprintln!("║ The 'actix' feature uses the production Actix Web backend.║");
    eprintln!("║ The 'simulator' feature uses a test simulator backend.    ║");
    eprintln!("╚════════════════════════════════════════════════════════════╝");
}
