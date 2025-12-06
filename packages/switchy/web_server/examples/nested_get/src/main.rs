//! Example demonstrating nested GET routes with the `MoosicBox` web server.
//!
//! This example shows how to create a web server with nested scopes and routes,
//! including CORS configuration and query parameter handling.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_web_server::{HttpResponse, Scope};

#[switchy_async::main]
async fn main() {
    env_logger::init();

    let cors = switchy_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server =
        switchy_web_server::WebServerBuilder::new()
            .with_cors(cors)
            .with_scope(Scope::new("/nested").get("/example", |req| {
                let path = req.path().to_string();
                let query = req.query_string().to_string();
                Box::pin(async move {
                    Ok(HttpResponse::ok()
                        .with_body(format!("hello, world! path={path} query={query}")))
                })
            }))
            .build();

    server.start().await;
}
