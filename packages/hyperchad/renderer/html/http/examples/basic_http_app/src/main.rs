#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::too_many_lines)]

//! Basic HTTP application example demonstrating `HyperChad` HTTP renderer.
//!
//! This example shows how to:
//! - Create an HTTP application with routing
//! - Handle multiple routes with dynamic content
//! - Process requests and generate HTTP responses
//! - Return different content types (HTML and JSON)

use hyperchad_renderer::{Content, View};
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_router::{RequestInfo, RouteRequest, Router};
use hyperchad_template::container;
use std::collections::BTreeMap;
use std::str::FromStr;

/// Creates the home page with welcome content and navigation.
fn create_home_page() -> Content {
    Content::View(Box::new(View {
        primary: Some(
            container! {
                div class="page" {
                    header class="header" {
                        h1 { "HyperChad HTTP Application" }
                        div class="nav" {
                            anchor href="/" class="nav-link" { "Home" }
                            span { " | " }
                            anchor href="/about" class="nav-link" { "About" }
                            span { " | " }
                            anchor href="/contact" class="nav-link" { "Contact" }
                        }
                    }
                    main class="main" {
                        section class="hero" {
                            h2 { "Welcome to HyperChad HTTP" }
                            span {
                                "This is a framework-agnostic HTTP adapter for HyperChad's HTML rendering capabilities. "
                                "It demonstrates how to build web applications with type-safe HTML generation."
                            }
                        }
                        section class="features" {
                            h3 { "Key Features" }
                            ul {
                                li { "Framework-agnostic HTTP request processing" }
                                li { "Type-safe HTML generation with the container! macro" }
                                li { "Built-in routing and navigation" }
                                li { "Action handling for interactive features" }
                                li { "Static asset serving" }
                            }
                        }
                    }
                    footer class="footer" {
                        span { "HyperChad HTTP Example - Built with Rust" }
                    }
                }
            }
            .into(),
        ),
        fragments: vec![],
        delete_selectors: vec![],
    }))
}

/// Creates the about page with framework information.
fn create_about_page() -> Content {
    Content::View(Box::new(View {
        primary: Some(
            container! {
                div class="page" {
                    header class="header" {
                        h1 { "About HyperChad HTTP" }
                        div class="nav" {
                            anchor href="/" class="nav-link" { "Home" }
                            span { " | " }
                            anchor href="/about" class="nav-link" { "About" }
                            span { " | " }
                            anchor href="/contact" class="nav-link" { "Contact" }
                        }
                    }
                    main class="main" {
                        section {
                            h2 { "What is HyperChad HTTP?" }
                            span {
                                "HyperChad HTTP is a generic HTTP adapter that combines HyperChad's HTML rendering "
                                "capabilities with routing, action handling, and static asset serving."
                            }
                            span {
                                "Unlike framework-specific integrations, this adapter works with any HTTP server "
                                "implementation, making it ideal for custom servers, microservices, and edge computing."
                            }
                        }
                        section {
                            h3 { "Architecture" }
                            span {
                                "The HTTP adapter processes RouteRequest objects and returns standard HTTP responses. "
                                "This design allows you to integrate it with any HTTP server framework."
                            }
                        }
                    }
                    footer class="footer" {
                        span { "HyperChad HTTP Example - Built with Rust" }
                    }
                }
            }
            .into(),
        ),
        fragments: vec![],
        delete_selectors: vec![],
    }))
}

/// Creates the contact page with a simple form.
fn create_contact_page() -> Content {
    Content::View(Box::new(View {
        primary: Some(
            container! {
                div class="page" {
                    header class="header" {
                        h1 { "Contact Us" }
                        div class="nav" {
                            anchor href="/" class="nav-link" { "Home" }
                            span { " | " }
                            anchor href="/about" class="nav-link" { "About" }
                            span { " | " }
                            anchor href="/contact" class="nav-link" { "Contact" }
                        }
                    }
                    main class="main" {
                        section {
                            h2 { "Get in Touch" }
                            span { "We'd love to hear from you!" }
                            form class="contact-form" {
                                div class="form-group" {
                                    span { "Name:" }
                                    input type="text" class="input-name";
                                }
                                div class="form-group" {
                                    span { "Email:" }
                                    input type="text" class="input-email";
                                }
                                div class="form-group" {
                                    span { "Message:" }
                                    textarea class="input-message" {}
                                }
                                button { "Send Message" }
                            }
                        }
                    }
                    footer class="footer" {
                        span { "HyperChad HTTP Example - Built with Rust" }
                    }
                }
            }
            .into(),
        ),
        fragments: vec![],
        delete_selectors: vec![],
    }))
}

/// Helper function to create a `RouteRequest` for testing
fn create_route_request(path: &str, method: &str) -> RouteRequest {
    RouteRequest {
        path: path.to_string(),
        method: switchy::http::models::Method::from_str(method).unwrap(),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        cookies: BTreeMap::new(),
        info: RequestInfo::default(),
        body: None,
    }
}

/// Creates and configures the HTTP application with router and renderer.
fn create_app() -> HttpApp<DefaultHtmlTagRenderer> {
    // Create a router and add routes
    let router = Router::new();

    // Add page routes
    router.add_route_result("/", |_req| async move {
        Ok::<_, Box<dyn std::error::Error>>(create_home_page())
    });

    router.add_route_result("/about", |_req| async move {
        Ok::<_, Box<dyn std::error::Error>>(create_about_page())
    });

    router.add_route_result("/contact", |_req| async move {
        Ok::<_, Box<dyn std::error::Error>>(create_contact_page())
    });

    // Add a JSON API route
    router.add_route_result("/api/status", |_req| async move {
        let status = serde_json::json!({
            "status": "ok",
            "message": "Server is running!",
        });

        Ok::<_, Box<dyn std::error::Error>>(Content::Raw {
            data: serde_json::to_vec(&status)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
                .into(),
            content_type: "application/json".to_string(),
        })
    });

    // Create the HTTP application with configuration
    let tag_renderer = DefaultHtmlTagRenderer::default();

    HttpApp::new(tag_renderer, router)
        .with_title("HyperChad HTTP Example")
        .with_description("A framework-agnostic HTTP application built with HyperChad")
        .with_viewport("width=device-width, initial-scale=1")
        .with_inline_css(
            r"
            * {
                margin: 0;
                padding: 0;
                box-sizing: border-box;
            }
            body {
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                line-height: 1.6;
                color: #333;
            }
            .page {
                min-height: 100vh;
                display: flex;
                flex-direction: column;
            }
            .header {
                background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                color: white;
                padding: 2rem;
                text-align: center;
            }
            .header h1 {
                margin-bottom: 1rem;
            }
            .nav {
                margin-top: 1rem;
            }
            .nav-link {
                color: white;
                text-decoration: none;
                padding: 0.5rem 1rem;
                border-radius: 4px;
                transition: background 0.3s;
            }
            .nav-link:hover {
                background: rgba(255, 255, 255, 0.2);
            }
            .main {
                flex: 1;
                padding: 2rem;
                max-width: 800px;
                margin: 0 auto;
                width: 100%;
            }
            .hero {
                margin-bottom: 2rem;
            }
            .hero h2 {
                color: #667eea;
                margin-bottom: 1rem;
            }
            .features {
                background: #f7fafc;
                padding: 1.5rem;
                border-radius: 8px;
                margin-bottom: 2rem;
            }
            .features h3 {
                margin-bottom: 1rem;
            }
            .features ul {
                list-style-position: inside;
            }
            .features li {
                margin-bottom: 0.5rem;
            }
            section {
                margin-bottom: 2rem;
            }
            section h2, section h3 {
                color: #667eea;
                margin-bottom: 1rem;
            }
            section p {
                margin-bottom: 1rem;
            }
            .contact-form {
                max-width: 500px;
            }
            .form-group {
                margin-bottom: 1rem;
            }
            .form-group label {
                display: block;
                margin-bottom: 0.5rem;
                font-weight: 600;
            }
            .form-group input,
            .form-group textarea {
                width: 100%;
                padding: 0.5rem;
                border: 1px solid #ddd;
                border-radius: 4px;
                font-family: inherit;
            }
            button {
                background: #667eea;
                color: white;
                padding: 0.75rem 1.5rem;
                border: none;
                border-radius: 4px;
                cursor: pointer;
                font-size: 1rem;
                transition: background 0.3s;
            }
            button:hover {
                background: #5568d3;
            }
            .footer {
                background: #2d3748;
                color: white;
                text-align: center;
                padding: 1.5rem;
            }
            ",
        )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Creating HyperChad HTTP application...");

    // Create the HTTP application
    let app = create_app();

    log::info!("Application created successfully!");
    log::info!("Testing different routes:\n");

    // Test the home page route
    log::info!("Testing GET /");
    let home_request = create_route_request("/", "GET");
    let home_response = app.process(&home_request).await?;
    log::info!("  Status: {}", home_response.status());
    log::info!(
        "  Content-Type: {}",
        home_response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
    );
    log::info!("  Body size: {} bytes", home_response.body().len());
    log::info!(
        "  Preview: {}...\n",
        String::from_utf8_lossy(home_response.body())
            .chars()
            .take(100)
            .collect::<String>()
    );

    // Test the about page route
    log::info!("Testing GET /about");
    let about_request = create_route_request("/about", "GET");
    let about_response = app.process(&about_request).await?;
    log::info!("  Status: {}", about_response.status());
    log::info!("  Body size: {} bytes\n", about_response.body().len());

    // Test the contact page route
    log::info!("Testing GET /contact");
    let contact_request = create_route_request("/contact", "GET");
    let contact_response = app.process(&contact_request).await?;
    log::info!("  Status: {}", contact_response.status());
    log::info!("  Body size: {} bytes\n", contact_response.body().len());

    // Test the JSON API route
    log::info!("Testing GET /api/status");
    let api_request = create_route_request("/api/status", "GET");
    let api_response = app.process(&api_request).await?;
    log::info!("  Status: {}", api_response.status());
    log::info!(
        "  Content-Type: {}",
        api_response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
    );
    let json_body = String::from_utf8_lossy(api_response.body());
    log::info!("  Body: {json_body}\n");

    log::info!("✓ All routes processed successfully!");
    log::info!("\nThis example demonstrates:");
    log::info!("  - Creating an HttpApp with routing");
    log::info!("  - Processing HTTP requests");
    log::info!("  - Generating HTML responses with server-side rendering");
    log::info!("  - Returning JSON responses for API endpoints");
    log::info!("\nTo integrate this with a real HTTP server:");
    log::info!("  1. Add your HTTP server dependency (Hyper, Actix, Axum, etc.)");
    log::info!("  2. Convert server requests to RouteRequest");
    log::info!("  3. Call app.process(&request).await");
    log::info!("  4. Convert the response to your server's response type");

    Ok(())
}
