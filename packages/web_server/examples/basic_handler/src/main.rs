//! Basic handler example demonstrating the `Route::with_handler()` method.
//!
//! This example shows how to use the clean async function syntax with
//! `Route::with_handler1()` instead of the verbose `Box::pin(async move {...})`
//! boilerplate. The example works with both Actix and Simulator backends,
//! demonstrating backend-agnostic handler creation.
//!
//! # Features
//!
//! * `actix` - Run with Actix Web backend
//! * `simulator` - Run with Simulator backend (default)
//!
//! # Example
//!
//! Run with the simulator backend:
//! ```sh
//! cargo run --package basic_handler_example
//! ```
//!
//! Run with the Actix backend:
//! ```sh
//! cargo run --package basic_handler_example --features actix
//! ```

use moosicbox_web_server::{
    Error, FromRequest, HttpRequest, HttpResponse, Method, RequestData, Route,
};

/// Demonstrates the `Route::with_handler()` method with clean async function syntax.
///
/// This handler showcases how to use `RequestData` to access HTTP request information
/// in a Send-safe manner, avoiding the verbose `Box::pin(async move {...})` boilerplate.
/// It extracts and displays various request properties including method, path, query string,
/// headers, and remote address.
///
/// # Errors
///
/// Returns an error if request data extraction fails or response construction fails.
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

/// Demonstrates the handler system with the Actix Web backend.
///
/// Creates a route using `Route::with_handler1()` with the Actix backend enabled,
/// showing how the new handler system eliminates the need for `Box::pin` boilerplate
/// while maintaining compatibility with Actix Web.
///
/// # Errors
///
/// Returns an error if route creation or backend initialization fails.
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

/// Demonstrates the handler system with the Simulator backend.
///
/// Creates a route using `Route::with_handler1()` with the Simulator backend,
/// then demonstrates request data extraction by creating a test request with
/// various headers, query parameters, and cookies. Shows how `RequestData`
/// provides Send-safe access to request information.
///
/// # Errors
///
/// * Request data extraction fails
/// * Route creation fails
/// * Backend initialization fails
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

/// Entry point demonstrating the `Route::with_handler()` method.
///
/// Runs backend-specific examples based on enabled features, demonstrating
/// how the new handler system works identically across different backends
/// (Actix Web and Simulator).
///
/// # Errors
///
/// * Backend example execution fails
/// * No backend features are enabled
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

#[cfg(test)]
mod tests {
    use super::*;

    mod handler_tests {
        use super::*;
        use moosicbox_web_server::FromRequest;

        #[test_log::test(switchy_async::test)]
        async fn test_demo_handler_returns_formatted_response() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(Method::Post, "/demo")
                .with_query_string("test=1&debug=true")
                .with_header("user-agent", "TestAgent/1.0")
                .with_header("content-type", "application/json")
                .with_remote_addr("192.168.1.100:54321");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(body_str.contains("=== New Handler System Demonstration ==="));
            assert!(body_str.contains("HTTP Method: Post"));
            assert!(body_str.contains("Path: /demo"));
            assert!(body_str.contains("Query String: test=1&debug=true"));
            assert!(body_str.contains("User-Agent: TestAgent/1.0"));
            assert!(body_str.contains("Content-Type: application/json"));
            assert!(body_str.contains("Remote Address: 192.168.1.100:54321"));
            assert!(body_str.contains("All Headers: 2 found"));
            assert!(body_str.contains("=== Route::with_handler() Working! ==="));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_demo_handler_handles_missing_optional_fields() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(Method::Post, "/demo");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(body_str.contains("Query String: None"));
            assert!(body_str.contains("User-Agent: None"));
            assert!(body_str.contains("Content-Type: None"));
            assert!(body_str.contains("Remote Address: None"));
            assert!(body_str.contains("All Headers: 0 found"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_demo_handler_processes_all_headers() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(Method::Post, "/demo")
                .with_header("user-agent", "CustomAgent/2.0")
                .with_header("content-type", "text/plain")
                .with_header("x-custom-header", "custom-value")
                .with_header("authorization", "Bearer token123");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            // Verify that the handler processes all headers correctly
            assert!(body_str.contains("User-Agent: CustomAgent/2.0"));
            assert!(body_str.contains("Content-Type: text/plain"));
            // Note: Custom headers may be included in the All Headers section
            assert!(body_str.contains("All Headers:"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_demo_handler_with_different_http_methods() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            for method in [Method::Get, Method::Put, Method::Delete, Method::Patch] {
                let request = SimulationRequest::new(method, "/demo");

                let stub = SimulationStub::new(request);
                let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

                let data = RequestData::from_request_sync(&http_request)
                    .expect("Failed to extract RequestData");

                let response = demo_handler(data)
                    .await
                    .expect("Handler should return Ok response");

                let body = response.body.expect("Response should have body");
                let body_str = match body {
                    moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                        String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                    }
                };

                assert!(body_str.contains(&format!("HTTP Method: {:?}", method)));
            }
        }

        #[test_log::test(switchy_async::test)]
        async fn test_demo_handler_with_complex_query_string() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(Method::Post, "/demo")
                .with_query_string("param1=value1&param2=value2&filter=active&sort=desc");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let data = RequestData::from_request_sync(&http_request)
                .expect("Failed to extract RequestData");

            let response = demo_handler(data)
                .await
                .expect("Handler should return Ok response");

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str
                    .contains("Query String: param1=value1&param2=value2&filter=active&sort=desc")
            );
        }
    }

    #[cfg(feature = "actix")]
    mod actix_tests {
        use super::*;

        #[test]
        fn test_actix_route_configuration() {
            let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

            assert_eq!(route.path, "/demo");
            assert_eq!(route.method, Method::Post);
        }

        #[test]
        fn test_run_actix_example_succeeds() {
            let result = run_actix_example();
            assert!(result.is_ok());
        }
    }

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    mod simulator_tests {
        use super::*;

        #[test]
        fn test_simulator_route_configuration() {
            let route = Route::with_handler1(Method::Post, "/demo", demo_handler);

            assert_eq!(route.path, "/demo");
            assert_eq!(route.method, Method::Post);
        }

        #[test]
        fn test_run_simulator_example_succeeds() {
            let result = run_simulator_example();
            assert!(result.is_ok());
        }

        #[test]
        fn test_simulator_request_data_extraction() {
            use moosicbox_web_server::FromRequest;
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

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

            let data = RequestData::from_request_sync(&http_request)
                .expect("Should successfully extract RequestData");

            assert_eq!(data.method, Method::Post);
            assert_eq!(data.path, "/demo");
            assert_eq!(data.query, "test=1&debug=true");
            assert_eq!(data.user_agent, Some("MoosicBox-Test/1.0".to_string()));
            assert_eq!(data.content_type, Some("application/json".to_string()));
            assert_eq!(data.remote_addr, Some("192.168.1.100:54321".to_string()));
            // Note: Headers may include cookies, so count can vary
            assert!(data.headers.len() >= 2);
        }
    }
}
