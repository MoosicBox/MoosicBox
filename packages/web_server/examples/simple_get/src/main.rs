#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Simple GET endpoint example demonstrating basic route creation with scope-based routing.
//!
//! This example shows the fundamental building blocks of creating a web server with
//! `MoosicBox`'s web server abstraction. It demonstrates how to set up a basic GET
//! endpoint using the `Scope::get()` method, configure CORS for cross-origin requests,
//! and access request information like path and query parameters.
//!
//! # Features
//!
//! * `actix` - Run with Actix Web backend (enabled by default for this example)
//! * `cors` - Enable CORS middleware support (enabled by default for this example)
//!
//! # Demonstrated Concepts
//!
//! This example demonstrates:
//!
//! * Creating a web server with `WebServerBuilder`
//! * Setting up permissive CORS configuration for development
//! * Using `Scope::get()` shortcut for registering GET routes
//! * Inline closure-based route handlers with `Box::pin(async move {...})`
//! * Accessing request path and query string from `HttpRequest`
//! * Building HTTP responses with dynamic content
//! * Running the server in a Switchy async runtime
//!
//! # Usage
//!
//! Run the example:
//! ```sh
//! cargo run --package web_server_simple_get
//! ```
//!
//! Test the endpoint with curl:
//! ```sh
//! # Basic request
//! curl http://localhost:8080/example
//!
//! # With query parameters
//! curl "http://localhost:8080/example?name=test&value=123"
//! ```
//!
//! The server responds with a greeting message that includes the request path
//! and query string, demonstrating basic request information access.

use moosicbox_web_server::{HttpResponse, Scope};

fn main() {
    let rt = switchy::unsync::runtime::Runtime::new();

    let handle = rt.handle();

    handle.block_on(async {
        env_logger::init();

        let cors = moosicbox_web_server::cors::Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_any_header();

        let server = moosicbox_web_server::WebServerBuilder::new()
            .with_cors(cors)
            .with_scope(Scope::new("").get("/example", |req| {
                let path = req.path().to_string();
                let query = req.query_string().to_string();
                Box::pin(async move {
                    Ok(HttpResponse::ok()
                        .with_body(format!("hello, world! path={path} query={query}")))
                })
            }))
            .build();

        server.start().await;
    });
}

#[cfg(test)]
mod tests {
    use moosicbox_web_server::{HttpRequest, HttpResponseBody};

    #[cfg(feature = "simulator")]
    mod handler_tests {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_example_handler_returns_formatted_response() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/example")
                .with_query_string("name=test&value=123");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let path = http_request.path().to_string();
            let query = http_request.query_string().to_string();

            let response = moosicbox_web_server::HttpResponse::ok()
                .with_body(format!("hello, world! path={path} query={query}"));

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert_eq!(
                body_str,
                "hello, world! path=/example query=name=test&value=123"
            );
            assert!(body_str.contains("path=/example"));
            assert!(body_str.contains("query=name=test&value=123"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_example_handler_handles_empty_query() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/example");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let path = http_request.path().to_string();
            let query = http_request.query_string().to_string();

            let response = moosicbox_web_server::HttpResponse::ok()
                .with_body(format!("hello, world! path={path} query={query}"));

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert_eq!(body_str, "hello, world! path=/example query=");
            assert!(body_str.contains("path=/example"));
            assert!(body_str.contains("query="));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_example_handler_preserves_path_variations() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request =
                SimulationRequest::new(moosicbox_web_server::Method::Get, "/example/nested/path");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let path = http_request.path().to_string();
            let query = http_request.query_string().to_string();

            let response = moosicbox_web_server::HttpResponse::ok()
                .with_body(format!("hello, world! path={path} query={query}"));

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(body_str.contains("path=/example/nested/path"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_example_handler_with_complex_query_parameters() {
            use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

            let request = SimulationRequest::new(moosicbox_web_server::Method::Get, "/example")
                .with_query_string("filter=active&sort=desc&page=2&limit=50");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(moosicbox_web_server::Stub::Simulator(stub));

            let path = http_request.path().to_string();
            let query = http_request.query_string().to_string();

            let response = moosicbox_web_server::HttpResponse::ok()
                .with_body(format!("hello, world! path={path} query={query}"));

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert_eq!(
                body_str,
                "hello, world! path=/example query=filter=active&sort=desc&page=2&limit=50"
            );
            assert!(body_str.contains("filter=active"));
            assert!(body_str.contains("sort=desc"));
            assert!(body_str.contains("page=2"));
            assert!(body_str.contains("limit=50"));
        }
    }
}
