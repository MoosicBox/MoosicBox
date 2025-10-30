#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic `HyperChad` App Example
//!
//! This example demonstrates the fundamental usage of the `hyperchad_app` framework,
//! including setting up a router, creating routes, and starting a web server.

use hyperchad_app::AppBuilder;
use hyperchad_renderer::{Color, View, assets::StaticAssetRoute};
use hyperchad_renderer_vanilla_js::{SCRIPT, SCRIPT_NAME_HASHED};
use hyperchad_router::{RouteRequest, Router};
use hyperchad_template::{Containers, container};
use log::info;
use std::sync::LazyLock;

/// Static assets served by the application
static ASSETS: LazyLock<Vec<StaticAssetRoute>> = LazyLock::new(|| {
    vec![StaticAssetRoute {
        route: format!("js/{}", SCRIPT_NAME_HASHED.as_str()),
        target: hyperchad_renderer::assets::AssetPathTarget::FileContents(SCRIPT.as_bytes().into()),
    }]
});

/// Creates the home page view
#[allow(clippy::too_many_lines)]
fn create_home_page() -> Containers {
    container! {
        div class="page" {
            // Header section with title and description
            header
                padding=24
                background="#2563eb"
                color=white
                text-align=center
            {
                h1 { "Welcome to HyperChad App" }
                span { "A basic example demonstrating the app framework" }
            }

            // Main content area
            main
                padding=24
                max-width=800
                margin="0 auto"
                gap=24
            {
                // Introduction section
                section
                    padding=24
                    background=white
                    border-radius=8
                    gap=16
                {
                    h2 { "Getting Started" }
                    div {
                        "This example shows how to build a basic HyperChad application with routing, "
                        "static assets, and a web server backend."
                    }
                }

                // Features section
                section
                    padding=24
                    background="#f3f4f6"
                    border-radius=8
                    gap=16
                {
                    h2 { "Key Features" }
                    ul padding-left=20 gap=8 {
                        li { "Router-based navigation" }
                        li { "Multiple page routes" }
                        li { "Static asset serving" }
                        li { "Actix web server backend" }
                        li { "Vanilla JavaScript interactivity" }
                    }
                }

                // Navigation section
                section
                    padding=24
                    background="#dbeafe"
                    border-radius=8
                    gap=16
                {
                    h2 { "Available Routes" }
                    div { "This application has multiple routes you can visit:" }
                    ul padding-left=20 gap=8 {
                        li { "/ - The main landing page (you are here)" }
                        li { "/about - Information about this example" }
                        li { "/demo - A demonstration page" }
                    }
                    div {
                        "Type these URLs in your browser's address bar to navigate between pages."
                    }
                }
            }

            // Footer
            footer
                padding=24
                text-align=center
                background="#1f2937"
                color=white
            {
                span { "Built with HyperChad App Framework" }
            }
        }
    }
}

/// Creates the about page view
#[allow(clippy::too_many_lines)]
fn create_about_page() -> Containers {
    container! {
        div class="page" {
            header
                padding=24
                background="#059669"
                color=white
                text-align=center
            {
                h1 { "About This Example" }
            }

            main
                padding=24
                max-width=800
                margin="0 auto"
                gap=24
            {
                section
                    padding=24
                    background=white
                    border-radius=8
                    gap=16
                {
                    h2 { "What This Example Demonstrates" }
                    div {
                        "This example shows the core functionality of hyperchad_app, including:"
                    }
                    ul padding-left=20 gap=8 {
                        li { "Creating a Router with multiple routes" }
                        li { "Building an AppBuilder with configuration" }
                        li { "Serving static assets (JavaScript files)" }
                        li { "Setting window properties (title, size, background)" }
                        li { "Using the Actix web server backend" }
                    }
                }

                section
                    padding=24
                    background="#fef3c7"
                    border-radius=8
                    gap=16
                {
                    h2 { "Architecture" }
                    div {
                        "HyperChad uses a modular architecture with pluggable renderers. "
                        "This example uses the HTML renderer with Actix web server and "
                        "vanilla JavaScript for client-side interactivity."
                    }
                }

                section
                    padding=24
                    background="#e0e7ff"
                    border-radius=8
                    gap=12
                {
                    h3 { "Other Pages" }
                    ul padding-left=20 gap=8 {
                        li { "Visit http://localhost:8080/ for the home page" }
                        li { "Visit http://localhost:8080/demo for the demo page" }
                    }
                }
            }

            footer
                padding=24
                text-align=center
                background="#1f2937"
                color=white
            {
                span { "Built with HyperChad App Framework" }
            }
        }
    }
}

/// Creates the demo page view
#[allow(clippy::too_many_lines)]
fn create_demo_page() -> Containers {
    container! {
        div class="page" {
            header
                padding=24
                background="#dc2626"
                color=white
                text-align=center
            {
                h1 { "Demo Page" }
                span { "Interactive demonstration" }
            }

            main
                padding=24
                max-width=800
                margin="0 auto"
                gap=24
            {
                section
                    padding=24
                    background=white
                    border-radius=8
                    gap=16
                {
                    h2 { "Color Styles" }
                    div gap=12 {
                        div
                            padding=16
                            background="#3b82f6"
                            color=white
                            border-radius=6
                        {
                            "Blue styled section"
                        }
                        div
                            padding=16
                            background="#10b981"
                            color=white
                            border-radius=6
                        {
                            "Green styled section"
                        }
                        div
                            padding=16
                            background="#f59e0b"
                            color=white
                            border-radius=6
                        {
                            "Orange styled section"
                        }
                    }
                }

                section
                    padding=24
                    background="#f3f4f6"
                    border-radius=8
                    gap=16
                {
                    h2 { "Layout Examples" }
                    div { "Demonstrating HyperChad's layout capabilities:" }

                    div
                        direction=row
                        gap=12
                        justify-content=space-between
                    {
                        div
                            padding=16
                            background=white
                            border-radius=6
                            flex=1
                        {
                            "Box 1"
                        }
                        div
                            padding=16
                            background=white
                            border-radius=6
                            flex=1
                        {
                            "Box 2"
                        }
                        div
                            padding=16
                            background=white
                            border-radius=6
                            flex=1
                        {
                            "Box 3"
                        }
                    }
                }

                section
                    padding=24
                    background="#e0e7ff"
                    border-radius=8
                    gap=12
                {
                    h3 { "Other Pages" }
                    ul padding-left=20 gap=8 {
                        li { "Visit http://localhost:8080/ for the home page" }
                        li { "Visit http://localhost:8080/about for the about page" }
                    }
                }
            }

            footer
                padding=24
                text-align=center
                background="#1f2937"
                color=white
            {
                span { "Built with HyperChad App Framework" }
            }
        }
    }
}

/// Creates the router with all application routes
fn create_router() -> Router {
    Router::new()
        // Home route
        .with_route("/", |_req: RouteRequest| async move {
            View::builder().with_primary(create_home_page()).build()
        })
        // About route
        .with_route("/about", |_req: RouteRequest| async move {
            View::builder().with_primary(create_about_page()).build()
        })
        // Demo route
        .with_route("/demo", |_req: RouteRequest| async move {
            View::builder().with_primary(create_demo_page()).build()
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the logger
    env_logger::init();
    info!("Starting HyperChad App Basic Usage Example");

    // Create an async runtime
    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    // Create the router with routes
    let router = create_router();

    // Log the server information
    info!("Server running on http://localhost:8080");
    info!("Available routes:");
    info!("  - http://localhost:8080/");
    info!("  - http://localhost:8080/about");
    info!("  - http://localhost:8080/demo");
    info!("Press Ctrl+C to stop");

    // Build the application using AppBuilder
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad App - Basic Usage".to_string())
        .with_description("Basic example of hyperchad_app framework".to_string())
        .with_size(1024.0, 768.0)
        .with_background(Color::from_hex("#f9fafb"));

    // Add static assets (JavaScript files)
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset)?;
    }

    // Build and run the application with default renderer (Actix + Vanilla JS)
    app.build_default()?.run()?;

    Ok(())
}
