//! Example demonstrating nested GET routes with the `MoosicBox` web server.
//!
//! This example shows how to create a web server with nested scopes and routes,
//! including CORS configuration and query parameter handling.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(test)]
use moosicbox_web_server::HttpRequest;
use moosicbox_web_server::{HttpResponse, Scope};

/// Handler for the nested GET route that returns a formatted greeting with request details.
///
/// This handler demonstrates how to access and use request path and query string information
/// in a nested route handler. It's designed to show that nested scopes work correctly
/// and that request information is properly passed through the routing system.
///
/// # Arguments
///
/// * `req` - The HTTP request containing path and query string information
///
/// # Returns
///
/// Returns an HTTP response with a body containing the greeting message along with
/// the request path and query string for demonstration purposes.
#[cfg(test)]
fn nested_example_handler(req: &HttpRequest) -> HttpResponse {
    let path = req.path().to_string();
    let query = req.query_string().to_string();
    HttpResponse::ok().with_body(format!("hello, world! path={path} query={query}"))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server = moosicbox_web_server::WebServerBuilder::new()
        .with_cors(cors)
        .with_scope(Scope::new("/nested").get("/example", |req| {
            let path = req.path().to_string();
            let query = req.query_string().to_string();
            Box::pin(async move {
                Ok(HttpResponse::ok().with_body(format!("hello, world! path={path} query={query}")))
            })
        }))
        .build();

    server.start().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    mod nested_handler_tests {
        use super::*;

        #[test]
        fn test_nested_example_handler_with_query_parameters() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example")
                .with_query_string("foo=bar&baz=qux");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str.contains("hello, world!"),
                "Response should contain greeting"
            );
            assert!(
                body_str.contains("path=/nested/example"),
                "Response should contain the request path"
            );
            assert!(
                body_str.contains("query=foo=bar&baz=qux"),
                "Response should contain the query string"
            );
        }

        #[test]
        fn test_nested_example_handler_without_query_parameters() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str.contains("hello, world!"),
                "Response should contain greeting"
            );
            assert!(
                body_str.contains("path=/nested/example"),
                "Response should contain the request path"
            );
            assert!(
                body_str.contains("query="),
                "Response should show empty query string"
            );
            // Verify empty query string (not just absent)
            assert!(
                body_str.ends_with("query=") || body_str.contains("query= "),
                "Query string should be empty"
            );
        }

        #[test]
        fn test_nested_example_handler_with_special_characters_in_query() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example")
                .with_query_string("name=John%20Doe&email=test%40example.com");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str.contains("hello, world!"),
                "Response should contain greeting"
            );
            assert!(
                body_str.contains("query=name=John%20Doe&email=test%40example.com"),
                "Response should contain URL-encoded query string"
            );
        }

        #[test]
        fn test_nested_example_handler_with_deeply_nested_path() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example/deep/path/segments");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str.contains("path=/nested/example/deep/path/segments"),
                "Response should contain the full nested path"
            );
        }

        #[test]
        fn test_nested_example_handler_with_multiple_query_parameters() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example")
                .with_query_string("a=1&b=2&c=3&debug=true&filter=active");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            let body = response.body.expect("Response should have body");
            let body_str = match body {
                moosicbox_web_server::HttpResponseBody::Bytes(bytes) => {
                    String::from_utf8(bytes.to_vec()).expect("Body should be valid UTF-8")
                }
            };

            assert!(
                body_str.contains("query=a=1&b=2&c=3&debug=true&filter=active"),
                "Response should contain all query parameters"
            );
        }

        #[test]
        fn test_nested_example_handler_response_status() {
            use moosicbox_web_server::{
                simulator::{SimulationRequest, SimulationStub},
                Method, Stub,
            };

            let request = SimulationRequest::new(Method::Get, "/nested/example")
                .with_query_string("test=value");

            let stub = SimulationStub::new(request);
            let http_request = HttpRequest::Stub(Stub::Simulator(stub));

            let response = nested_example_handler(&http_request);

            assert_eq!(
                response.status_code,
                switchy_http_models::StatusCode::Ok,
                "Response status should be 200 OK"
            );
            assert!(response.body.is_some(), "Response should have a body");
        }
    }
}
