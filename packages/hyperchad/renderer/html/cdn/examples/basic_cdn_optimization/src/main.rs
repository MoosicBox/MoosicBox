#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad::{
    renderer::{Content, ToRenderRunner},
    renderer_html::{DefaultHtmlTagRenderer, router_to_web_server},
    router::{Container, RouteRequest, Router},
    template::{self as hyperchad_template, container},
    transformer as hyperchad_transformer,
};
use hyperchad_renderer_html_cdn::setup_cdn_optimization;
use log::info;
use serde_json::json;

/// Creates the home page with dynamic content
fn create_home_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                div class="container" {
                    h1 { "CDN-Optimized HyperChad App" }
                    span { "This page is served via CDN optimization!" }
                }
            }
            main class="main" {
                div class="container" {
                    section class="content" {
                        h2 { "How CDN Optimization Works" }
                        span {
                            "The initial HTML skeleton is served from a CDN edge location, "
                            "while the dynamic content is fetched via JavaScript from the origin server."
                        }
                        div class="benefits-section" {
                            h3 { "Benefits:" }
                            ul {
                                li { "Fast initial page load from CDN edge" }
                                li { "Reduced origin server load for static assets" }
                                li { "Dynamic content still fully functional" }
                                li { "Cost-efficient scaling" }
                            }
                        }
                        div class="technical-section" {
                            h3 { "Technical Details:" }
                            span {
                                "Open your browser's Network tab to see how the page loads. "
                                "You'll notice the initial HTML loads immediately (from CDN), "
                                "then the browser fetches the full content from /__hyperchad_dynamic_root__."
                            }
                        }
                    }
                }
            }
            footer class="footer" {
                div class="container" {
                    span { "Built with HyperChad + CDN Optimization" }
                }
            }
        }
    }
    .into()
}

/// Creates the about page
fn create_about_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                div class="container" {
                    h1 { "About CDN Optimization" }
                    anchor href="/" { "Back to Home" }
                }
            }
            main class="main" {
                div class="container" {
                    h2 { "How It Works Under the Hood" }
                    span {
                        "The setup_cdn_optimization() function transforms your router by:"
                    }
                    ul {
                        li { "Replacing the root route (/) with a static skeleton HTML" }
                        li { "Creating a new dynamic endpoint at /__hyperchad_dynamic_root__" }
                        li { "The skeleton uses fetch() to load the full content at runtime" }
                        li { "document.open()/write()/close() replaces the entire page seamlessly" }
                    }
                }
            }
            footer class="footer" {
                div class="container" {
                    span { "Built with HyperChad + CDN Optimization" }
                }
            }
        }
    }
    .into()
}

fn create_router() -> Router {
    let router = Router::new();

    // Home route - will be automatically optimized for CDN
    router.add_route_result("/", |_req: RouteRequest| async move {
        Ok(create_home_page()) as Result<Container, Box<dyn std::error::Error>>
    });

    // About route - regular dynamic route
    router.add_route_result("/about", |_req: RouteRequest| async move {
        Ok(create_about_page()) as Result<Container, Box<dyn std::error::Error>>
    });

    // API endpoint - returns JSON data
    router.add_route_result("/api/info", |_req: RouteRequest| async move {
        let response = json!({
            "cdn_enabled": true,
            "message": "CDN optimization is active!",
            "timestamp": switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });
        Ok(Content::Raw {
            data: response.to_string().into(),
            content_type: "application/json".to_string(),
        }) as Result<Content, Box<dyn std::error::Error>>
    });

    router
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    info!("Starting CDN Optimization Example");

    // Create router with dynamic routes
    let router = create_router();

    // Apply CDN optimization - this will transform the root route
    // to use a static skeleton HTML that fetches dynamic content
    let router = setup_cdn_optimization(
        router,
        Some("CDN-Optimized HyperChad App"),         // Page title
        Some("width=device-width, initial-scale=1"), // Viewport meta tag
    );

    info!("CDN optimization configured - skeleton HTML will be served for /");

    // Create the web server app
    let app = router_to_web_server(DefaultHtmlTagRenderer::default(), router)
        .with_title(Some("CDN-Optimized HyperChad Example".to_string()))
        .with_description(Some(
            "Demonstrates CDN optimization for HyperChad applications".to_string(),
        ));

    // Start the server
    let runtime = switchy::unsync::runtime::Runtime::new();
    let handle = runtime.handle();
    let mut runner = app
        .to_runner(handle)
        .map_err(|e| format!("Failed to create runner: {e}"))?;

    info!("Server starting on http://localhost:8343");
    info!("Visit the page and check the Network tab to see CDN optimization in action");

    runner
        .run()
        .map_err(|e| format!("Failed to run server: {e}"))?;

    Ok(())
}
