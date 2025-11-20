//! Basic HTTP handler example demonstrating minimal request handling.
//!
//! This example shows how to create simple HTTP request handlers using only
//! the `RequestData` extractor from `moosicbox_web_server`, without requiring
//! any JSON parsing or serde dependencies.
//!
//! # Features
//!
//! * Basic request information extraction (method, path, query, headers)
//! * Multiple extractor support (demonstrates using `RequestData` multiple times)
//! * Backend flexibility (works with both Actix and Simulator backends)
//! * Zero JSON/serde dependencies for minimal complexity
//!
//! # Usage
//!
//! Run with Actix backend:
//! ```sh
//! cargo run --features actix
//! ```
//!
//! Run with Simulator backend:
//! ```sh
//! cargo run --features simulator
//! ```
//!
//! # Example Handlers
//!
//! The example includes three handler functions:
//!
//! * `basic_info_handler` - Extracts and displays basic request information
//! * `double_data_handler` - Demonstrates using multiple `RequestData` extractors
//! * `error_demo_handler` - Shows basic error handling without complex parsing

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "actix", feature = "simulator"))]
use moosicbox_web_server::{Error, HttpResponse, RequestData};

/// Handles requests by extracting and displaying basic request information.
///
/// This handler demonstrates the simplest use of `RequestData` to extract
/// basic request metadata without any JSON or query parsing.
///
/// # Errors
///
/// Returns an error if the response cannot be constructed.
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

/// Handles requests using multiple `RequestData` extractors.
///
/// This handler demonstrates that the same extractor can be used multiple times
/// in a single handler function, which can be useful for certain handler patterns.
///
/// # Errors
///
/// Returns an error if the response cannot be constructed.
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

/// Demonstrates basic error handling without complex parsing.
///
/// This handler shows how to access query string data through `RequestData`
/// without needing separate query parsing or serde dependencies.
///
/// # Errors
///
/// Returns an error if the response cannot be constructed.
#[cfg(any(feature = "actix", feature = "simulator"))]
#[allow(clippy::unused_async)]
async fn error_demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    let response = format!(
        "Basic Handler Demo:\n  Query String: '{}'\n  Tip: This handler only uses RequestData\n  Tip: No JSON or query parsing dependencies needed",
        data.query
    );
    Ok(HttpResponse::ok().with_body(response))
}

/// Runs the example handlers with the Actix Web backend.
///
/// This function sets up and displays the route configuration for running
/// the basic handler examples using the production Actix Web server backend.
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

/// Runs the example handlers with the Simulator backend.
///
/// This function sets up the route configuration and runs test simulations
/// to demonstrate the basic handler functionality using the test simulator backend.
///
/// # Errors
///
/// * Failed to extract `RequestData` from the simulated request
/// * Failed to construct the simulated HTTP request
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

/// Entry point for the basic handler example.
///
/// Runs the appropriate backend examples based on the enabled feature flags.
/// Requires either the `actix` or `simulator` feature to be enabled.
///
/// # Errors
///
/// * Backend-specific errors from running the simulator examples
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "actix", feature = "simulator"))]
    mod handler_tests {
        use super::*;
        use moosicbox_web_server::FromRequest;

        #[test_log::test(switchy_async::test)]
        async fn test_basic_info_handler_returns_formatted_response() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/basic-info")
                .with_query_string("test=1&debug=true")
                .with_header("user-agent", "TestAgent/1.0")
                .with_header("content-type", "application/json");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = basic_info_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Basic Request Info:"));
            assert!(body_str.contains("Method: Get"));
            assert!(body_str.contains("Path: /basic-info"));
            assert!(body_str.contains("Query: test=1&debug=true"));
            assert!(body_str.contains("User Agent: Some(\"TestAgent/1.0\")"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_basic_info_handler_handles_empty_query() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/basic-info");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = basic_info_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Basic Request Info:"));
            assert!(body_str.contains("Query: "));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_double_data_handler_extracts_same_data() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Post, "/double")
                .with_query_string("param1=value1&param2=value2")
                .with_header("user-agent", "DoubleTest/1.0");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data1 = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract first RequestData");
            let data2 = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract second RequestData");

            let response = double_data_handler(data1, data2)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Double RequestData:"));
            assert!(body_str.contains("Data1 Method: Post"));
            assert!(body_str.contains("Data2 Method: Post"));
            assert!(body_str.contains("Path: /double"));
            assert!(body_str.contains("Same data: true"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_double_data_handler_with_different_methods() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Put, "/double");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data1 = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract first RequestData");
            let data2 = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract second RequestData");

            let response = double_data_handler(data1, data2)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Data1 Method: Put"));
            assert!(body_str.contains("Data2 Method: Put"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_error_demo_handler_displays_query_string() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/error")
                .with_query_string("error=test&code=500");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = error_demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Basic Handler Demo:"));
            assert!(body_str.contains("Query String: 'error=test&code=500'"));
            assert!(body_str.contains("No JSON or query parsing dependencies needed"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_error_demo_handler_handles_no_query_string() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/error");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = error_demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };
            assert!(body_str.contains("Query String: ''"));
        }
    }

    #[cfg(feature = "actix")]
    mod actix_tests {
        use super::*;

        #[test]
        fn test_actix_routes_configuration() {
            let routes = [
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

            assert_eq!(routes.len(), 3, "Should have 3 routes configured");
            assert_eq!(routes[0].path, "/basic-info");
            assert_eq!(routes[1].path, "/double");
            assert_eq!(routes[2].path, "/error");
        }
    }

    #[cfg(feature = "simulator")]
    mod simulator_tests {
        use super::*;

        #[test]
        fn test_simulator_routes_configuration() {
            let routes = [
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

            assert_eq!(routes.len(), 3, "Should have 3 routes configured");
            assert_eq!(routes[0].path, "/basic-info");
            assert_eq!(routes[1].path, "/double");
            assert_eq!(routes[2].path, "/error");
        }

        #[test]
        fn test_run_simulator_examples_creates_valid_request_data() {
            use moosicbox_web_server::FromRequest;
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/basic-info")
                .with_query_string("test=1&debug=true")
                .with_header("user-agent", "MoosicBox-BasicTest/1.0")
                .with_header("content-type", "application/json");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data = RequestData::from_request_sync(&http_request)
                .expect("Should successfully extract RequestData");

            assert_eq!(data.method, moosicbox_web_server::Method::Get);
            assert_eq!(data.path, "/basic-info");
            assert_eq!(data.query, "test=1&debug=true");
            assert_eq!(data.headers.len(), 2);
        }

        #[test]
        fn test_double_request_data_extraction() {
            use moosicbox_web_server::FromRequest;
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/double")
                .with_query_string("param1=value1&param2=value2")
                .with_header("user-agent", "MoosicBox-DoubleTest/1.0");

            let stub = SimulationStub::new(request);
            let http_request = moosicbox_web_server::HttpRequest::Stub(
                moosicbox_web_server::Stub::Simulator(stub),
            );

            let data1 = RequestData::from_request_sync(&http_request)
                .expect("Should successfully extract first RequestData");
            let data2 = RequestData::from_request_sync(&http_request)
                .expect("Should successfully extract second RequestData");

            assert_eq!(data1.method, data2.method);
            assert_eq!(data1.path, data2.path);
            assert_eq!(data1.query, data2.query);
        }
    }
}
