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
