#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, RequestData};

// Simple handler with just RequestData
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn basic_info_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Basic Request Info:\n  Method: {:?}\n  Path: {}\n  Query: {}\n  Headers: {}\n  User Agent: {:?}",
        data.method,
        data.path,
        data.query,
        data.headers.len(),
        data.user_agent
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Handler with two RequestData extractors (for demonstration)
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn double_data_handler(
    data1: RequestData,
    data2: RequestData,
) -> Result<HttpResponse, Error> {
    let response = format!(
        "Double RequestData:\n  Data1 Method: {:?}\n  Data2 Method: {:?}\n  Path: {}\n  Same data: {}",
        data1.method,
        data2.method,
        data1.path,
        data1.method == data2.method
    );
    Ok(HttpResponse::ok().with_body(response))
}

// Error demo handler
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn error_demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Basic Handler Demo:\n  Query String: '{}'\n  Tip: This handler only uses RequestData\n  Tip: No JSON or query parsing dependencies needed",
        data.query
    );
    Ok(HttpResponse::ok().with_body(response))
}

#[cfg(feature = "actix")]
fn run_actix_examples() {
    println!("🚀 Running Actix Backend Basic Handler Examples...");

    let routes = vec![
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/basic-info",
            basic_info_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Get,
            "/double",
            double_data_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ Basic routes created:");
    for route in &routes {
        println!("   {}: {} {}", route.method, route.path, route.method);
    }
    println!("   Backend: Actix Web");
}

#[cfg(feature = "simulator")]
#[cfg(not(feature = "actix"))]
fn run_simulator_examples() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_web_server::FromRequest;
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    println!("🧪 Running Simulator Backend Basic Handler Examples...");

    let routes = vec![
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/basic-info",
            basic_info_handler,
        ),
        moosicbox_web_server::Route::with_handler2(
            moosicbox_web_server::Method::Get,
            "/double",
            double_data_handler,
        ),
        moosicbox_web_server::Route::with_handler1(
            moosicbox_web_server::Method::Get,
            "/error",
            error_demo_handler,
        ),
    ];

    println!("✅ Basic routes created:");
    for route in &routes {
        println!("   {}: {} {}", route.method, route.path, route.method);
    }
    println!("   Backend: Simulator");

    // Test basic info handler
    println!("\n📋 Testing Basic Info Handler (RequestData only):");
    let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/basic-info")
        .with_query_string("test=1&debug=true")
        .with_header("user-agent", "MoosicBox-BasicTest/1.0")
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

    // Test the double data handler
    println!("\n📋 Testing Double Data Handler (RequestData + RequestData):");
    let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/double")
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

    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Basic Handler Examples - RequestData Only");
    println!("============================================\n");

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
        eprintln!("║                Basic Handler Example                       ║");
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ This example demonstrates basic request handling without   ║");
        eprintln!("║ any JSON or query parsing dependencies.                   ║");
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

    println!("✅ Basic Handler Examples Complete!");
    println!("   - RequestData extraction working standalone");
    println!("   - Multiple RequestData extractors in one handler");
    println!("   - No serde or JSON dependencies required");
    println!("   - Works with both Actix and Simulator backends");
    println!("   - Clean, minimal web server functionality");

    Ok(())
}
