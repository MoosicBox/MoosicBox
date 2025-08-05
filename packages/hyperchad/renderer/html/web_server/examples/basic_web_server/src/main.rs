#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad::{
    renderer::{Content, ToRenderRunner},
    renderer_html::{router_to_web_server, DefaultHtmlTagRenderer},
    router::{Container, RouteRequest, Router},
    template::{self as hyperchad_template, container},
};
use log::info;
use serde_json::json;

fn create_home_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                div class="nav" {
                    div class="nav-brand" {
                        h1 { "HyperChad Demo" }
                    }
                    div class="nav-links" {
                        anchor href="/" { "Home" }
                        anchor href="/about" { "About" }
                        anchor href="/contact" { "Contact" }
                    }
                }
            }
            main class="main" {
                section class="hero" {
                    div class="hero-content" {
                        h1 class="hero-title" { "Welcome to HyperChad!" }
                        span class="hero-subtitle" { "A modern web framework built with Rust" }
                    }
                }
                section class="features" {
                    div class="container" {
                        h2 { "Features" }
                        div class="feature-grid" {
                            div class="feature-card" {
                                h3 { "🚀 Fast" }
                                span { "Built with Rust for maximum performance" }
                            }
                            div class="feature-card" {
                                h3 { "🎯 Type-Safe" }
                                span { "Compile-time guarantees for reliability" }
                            }
                            div class="feature-card" {
                                h3 { "🎨 Modern" }
                                span { "Beautiful and responsive design" }
                            }
                        }
                    }
                }
            }
            footer class="footer" {
                div class="container" {
                    span { "Built with ❤️ using HyperChad" }
                }
            }
        }
    }
    .into()
}

fn create_about_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                div class="nav" {
                    div class="nav-brand" {
                        h1 { "HyperChad Demo" }
                    }
                    div class="nav-links" {
                        anchor href="/" { "Home" }
                        anchor href="/about" class="active" { "About" }
                        anchor href="/contact" { "Contact" }
                    }
                }
            }
            main class="main" {
                div class="container" {
                    section class="content" {
                        h1 { "About HyperChad" }
                        span {
                            "HyperChad is a modern web framework built with Rust, designed for "
                            "performance and developer experience."
                        }
                        h2 { "Key Features" }
                        ul {
                            li { "Type-safe HTML generation" }
                            li { "Component-based architecture" }
                            li { "Server-side rendering" }
                            li { "Built-in routing" }
                            li { "Static asset serving" }
                        }
                    }
                }
            }
            footer class="footer" {
                div class="container" {
                    span { "Built with ❤️ using HyperChad" }
                }
            }
        }
    }
    .into()
}

fn create_contact_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                div class="nav" {
                    div class="nav-brand" {
                        h1 { "HyperChad Demo" }
                    }
                    div class="nav-links" {
                        anchor href="/" { "Home" }
                        anchor href="/about" { "About" }
                        anchor href="/contact" class="active" { "Contact" }
                    }
                }
            }
            main class="main" {
                div class="container" {
                    section class="content" {
                        h1 { "Contact Us" }
                        span { "Get in touch with us using the form below." }
                        form {
                            div class="form-group" {
                                span { "Name:" }
                                input type="text";
                            }
                            div class="form-group" {
                                span { "Email:" }
                                input type="email";
                            }
                            div class="form-group" {
                                span { "Message:" }
                                input type="text";
                            }
                            button { "Send Message" }
                        }
                    }
                }
            }
            footer class="footer" {
                div class="container" {
                    span { "Built with ❤️ using HyperChad" }
                }
            }
        }
    }
    .into()
}

fn create_router() -> Router {
    let router = Router::new();

    // Home route
    router.add_route_result("/", |_req: RouteRequest| async move {
        Ok(create_home_page()) as Result<Container, Box<dyn std::error::Error>>
    });

    // About route
    router.add_route_result("/about", |_req: RouteRequest| async move {
        Ok(create_about_page()) as Result<Container, Box<dyn std::error::Error>>
    });

    // Contact route
    router.add_route_result("/contact", |_req: RouteRequest| async move {
        Ok(create_contact_page()) as Result<Container, Box<dyn std::error::Error>>
    });

    // API: Status endpoint
    router.add_route_result("/api/status", |_req: RouteRequest| async move {
        let response = json!({
            "status": "ok",
            "message": "HyperChad Web Server is running!",
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
    info!("Starting HyperChad Web Server Example");

    // Create router with all our routes
    let router = create_router();

    // Create the web server app using the web_server backend
    let app = router_to_web_server(DefaultHtmlTagRenderer::default(), router)
        .with_title(Some("HyperChad Web Server Example".to_string()))
        .with_description(Some(
            "A modern web application built with HyperChad".to_string(),
        ));

    // Start the server
    let runtime = switchy::unsync::runtime::Runtime::new();
    let handle = runtime.handle();
    let mut runner = app
        .to_runner(handle)
        .map_err(|e| format!("Failed to create runner: {e}"))?;
    runner
        .run()
        .map_err(|e| format!("Failed to run server: {e}"))?;

    Ok(())
}
