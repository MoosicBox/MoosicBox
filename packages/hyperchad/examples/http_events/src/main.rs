#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! HTTP Events Example for `HyperChad`
//!
//! This example demonstrates how to use HTTP request lifecycle event handlers in `HyperChad`.
//! It showcases all six HTTP event types that can be attached to elements:
//!
//! * `fx-http-before-request` - Fires before the HTTP request starts
//! * `fx-http-after-request` - Fires after request completes (success or error)
//! * `fx-http-success` - Fires on successful response (2xx status)
//! * `fx-http-error` - Fires on HTTP error or network failure
//! * `fx-http-abort` - Fires when request is aborted
//! * `fx-http-timeout` - Fires when request exceeds timeout (30s default)
//!
//! The example creates a simple task creation interface with three test buttons:
//! - **Add Task**: Normal successful request with 500ms delay
//! - **Test Error**: Always fails to demonstrate error handling
//! - **Test Slow**: 3-second delay to test loading states
//!
//! Open the browser console to see `log()` messages and the Network tab to observe
//! `fetch()` calls being intercepted by the actions-http-events plugin.

use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{Container, RouteRequest, Router},
    template::{self as hyperchad_template, Containers, container},
};
use log::info;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

/// Static assets to be served by the web server.
///
/// This includes the `HyperChad` vanilla-js runtime script with a hashed filename for cache busting.
#[cfg(feature = "assets")]
static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "vanilla-js")]
        hyperchad::renderer::assets::StaticAssetRoute {
            route: format!(
                "js/{}",
                hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
            ),
            target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
                hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
            ),
        },
    ]
});

/// Creates the "Add Task" button with HTTP event handlers.
///
/// This button demonstrates a successful HTTP request flow with event handlers for:
/// * `before-request` - Shows loading spinner, hides messages
/// * `after-request` - Hides loading spinner
/// * `success` - Shows success message
/// * `error` - Shows error message
/// * `abort` - Logs abort message
/// * `timeout` - Shows error message and logs timeout
fn create_add_task_button() -> Containers {
    container! {
        button
            hx-post="/api/tasks"
            type=button
            padding-y=12
            padding-x=24
            background=#3b82f6
            color=white
            border-radius=6
            cursor=pointer

            fx-http-before-request=fx {
                display("loading-spinner");
                no_display("success-message");
                no_display("error-message");
                log("Starting task creation request...");
            }

            fx-http-after-request=fx {
                no_display("loading-spinner");
                log("Request completed");
            }

            fx-http-success=fx {
                display("success-message");
                log("✅ Task created successfully!");
            }

            fx-http-error=fx {
                display("error-message");
                log("❌ Failed to create task");
            }

            fx-http-abort=fx {
                log("⚠️ Request was aborted");
            }

            fx-http-timeout=fx {
                display("error-message");
                log("⏱️ Request timed out");
            }
        {
            "Add Task"
        }
    }
}

/// Creates the "Test Error" button that always triggers an error response.
///
/// This button demonstrates error handling in HTTP event handlers:
/// * `before-request` - Shows loading spinner, hides messages
/// * `after-request` - Hides loading spinner
/// * `error` - Shows error message
fn create_error_button() -> Containers {
    container! {
        button
            hx-post="/api/tasks/error"
            type=button
            padding-y=12
            padding-x=24
            background=#dc2626
            color=white
            border-radius=6
            cursor=pointer

            fx-http-before-request=fx {
                display("loading-spinner");
                no_display("success-message");
                no_display("error-message");
                log("Testing error scenario...");
            }

            fx-http-after-request=fx {
                no_display("loading-spinner");
            }

            fx-http-error=fx {
                display("error-message");
                log("Error test completed");
            }
        {
            "Test Error"
        }
    }
}

/// Creates the "Test Slow" button that simulates a slow 3-second request.
///
/// This button demonstrates handling of slow requests with loading states:
/// * `before-request` - Shows loading spinner, hides messages
/// * `after-request` - Hides loading spinner
/// * `success` - Shows success message
fn create_slow_button() -> Containers {
    container! {
        button
            hx-post="/api/tasks/slow"
            type=button
            padding-y=12
            padding-x=24
            background=#f59e0b
            color=white
            border-radius=6
            cursor=pointer

            fx-http-before-request=fx {
                display("loading-spinner");
                no_display("success-message");
                no_display("error-message");
                log("Starting slow request (3 seconds)...");
            }

            fx-http-after-request=fx {
                no_display("loading-spinner");
                log("Slow request finished");
            }

            fx-http-success=fx {
                display("success-message");
                log("Slow request succeeded!");
            }
        {
            "Test Slow (3s)"
        }
    }
}

/// Creates the task input field.
///
/// This is a simple text input for entering task names, which gets cleared
/// after successful task creation via fragment replacement.
fn task_input() -> Containers {
    container! {
        input
            id="task-input"
            type=text
            name="task"
            placeholder="Enter task name"
            padding=8
            width=100%
            border-radius=4
            value="";
    }
}

/// Creates the main page layout with all UI components.
///
/// The page includes:
/// * Header with title and description
/// * Success/error message displays (initially hidden)
/// * Loading spinner (fixed position, initially hidden)
/// * Task creation form with input and three test buttons
/// * Event explanation section describing all six HTTP event types
/// * Developer info section with debugging tips
/// * Footer
#[allow(clippy::too_many_lines)]
fn create_main_page() -> Container {
    container! {
        div class="page" {
            header
                class="header"
                padding=24
                background=#1f2937
                color=white
            {
                h1 { "HyperChad HTTP Events Demo" }
                span { "Demonstrating all 6 HTTP lifecycle event handlers" }
            }

            // Centering wrapper for main content
            div
                direction=row
                justify-content=center
                width=100%
            {
                main
                    class="main"
                    padding=24
                    max-width=800
                    width=100%
                    gap=24
                {
                    // Success message (hidden by default)
                    div
                        id="success-message"
                        hidden
                        class="message success"
                        padding=16
                        background=#10b981
                        color=white
                        border-radius=8
                    {
                        span { "✅ Task created successfully!" }
                    }

                    // Error message (hidden by default)
                    div
                        id="error-message"
                        hidden
                        class="message error"
                        padding=16
                        background=#ef4444
                        color=white
                        border-radius=8
                    {
                        span { "❌ An error occurred" }
                    }

                    // Loading spinner (hidden by default, fixed position)
                    div
                        id="loading-spinner"
                        hidden
                        class="spinner"
                        position=fixed
                        top=20
                        right=20
                        padding=16
                        background=#3b82f6
                        color=white
                        border-radius=8
                    {
                        span { "⏳ Loading..." }
                    }

                    // Task form section
                    section
                        class="task-form"
                        padding=24
                        background=white
                        border-radius=8
                    {
                        h2 { "Create New Task" }

                        form gap=16 {
                            div class="form-group" gap=8 {
                                span
                                    font-weight=bold
                                {
                                    "Task Name:"
                                }
                                (task_input())
                            }

                            div class="button-group" direction=row gap=8 {
                                (create_add_task_button())
                                (create_error_button())
                                (create_slow_button())
                            }
                        }
                    }

                    // Event explanation section
                    section
                        class="event-info"
                        padding=24
                        background=#f9fafb
                        border-radius=8
                        gap=16
                    {
                        h3 { "HTTP Event Types" }
                        div class="event-list" gap=12 {
                            div class="event-item" {
                                span font-weight=bold { "fx-http-before-request:" }
                                span { " Fires before the HTTP request starts" }
                            }
                            div class="event-item" {
                                span font-weight=bold { "fx-http-after-request:" }
                                span { " Fires after request completes (success or error)" }
                            }
                            div class="event-item" {
                                span font-weight=bold { "fx-http-success:" }
                                span { " Fires on successful response (2xx status)" }
                            }
                            div class="event-item" {
                                span font-weight=bold { "fx-http-error:" }
                                span { " Fires on HTTP error or network failure" }
                            }
                            div class="event-item" {
                                span font-weight=bold { "fx-http-abort:" }
                                span { " Fires when request is aborted" }
                            }
                            div class="event-item" {
                                span font-weight=bold { "fx-http-timeout:" }
                                span { " Fires when request exceeds timeout (30s default)" }
                            }
                        }
                    }

                    // Developer info
                    section
                        class="dev-info"
                        padding=24
                        background=#eff6ff
                        border-radius=8
                        gap=16
                    {
                        h3 { "Developer Info" }
                        ul padding-left=20 gap=8 {
                            li {
                                "Open browser console to see log() messages"
                            }
                            li {
                                "Check Network tab to see fetch() calls being intercepted"
                            }
                            li {
                                "Events emit as hyperchad:http-* custom DOM events"
                            }
                            li {
                                "The actions-http-events.ts plugin wraps window.fetch()"
                            }
                        }
                    }
                }
            }

            footer
                class="footer"
                padding=24
                text-align=center
                background=#f3f4f6
            {
                span { "Built with HyperChad • HTTP Events Plugin Demo" }
            }
        }
    }
    .into()
}

/// Creates the application router with all route handlers.
///
/// Routes:
/// * `/` - Main page displaying the HTTP events demo interface
/// * `/api/tasks` - Normal task creation endpoint (500ms delay)
/// * `/api/tasks/error` - Error endpoint that always fails
/// * `/api/tasks/slow` - Slow endpoint with 3-second delay
fn create_router() -> Router {
    Router::new()
        // Main page
        .with_route_result("/", |_req: RouteRequest| async move {
            Ok(create_main_page()) as Result<Container, Box<dyn std::error::Error>>
        })
        // Normal task creation (500ms delay)
        .with_route("/api/tasks", |_req: RouteRequest| async move {
            switchy::unsync::time::sleep(std::time::Duration::from_millis(500)).await;

            // Return button + clear input using fragments
            View::builder()
                .with_primary(create_add_task_button())
                .with_fragment(task_input())
                .build()
        })
        // Error endpoint (always fails)
        .with_route_result("/api/tasks/error", |_req: RouteRequest| async move {
            switchy::unsync::time::sleep(std::time::Duration::from_millis(100)).await;
            Err("Simulated error for testing HTTP error events".into())
                as Result<Container, Box<dyn std::error::Error>>
        })
        // Slow endpoint (3 second delay)
        .with_route("/api/tasks/slow", |_req: RouteRequest| async move {
            switchy::unsync::time::sleep(std::time::Duration::from_secs(3)).await;

            // Return button + clear input using fragments
            View::builder()
                .with_primary(create_slow_button())
                .with_fragment(task_input())
                .build()
        })
}

/// Entry point for the HTTP Events example application.
///
/// This function:
/// * Initializes logging via `env_logger`
/// * Creates an async runtime using `switchy`
/// * Sets up the router with demo routes
/// * Builds and runs the `HyperChad` application on `http://localhost:8080`
///
/// # Errors
///
/// Returns an error if:
/// * The async runtime fails to initialize
/// * Static asset registration fails (when `assets` feature is enabled)
/// * The web server fails to start or bind to port 8080
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    info!("Starting HyperChad HTTP Events Example");

    // Create async runtime
    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    // Create router
    let router = create_router();

    // Create and run app using AppBuilder
    info!("Server running on http://localhost:8080");
    info!("Press Ctrl+C to stop");

    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad HTTP Events Demo".to_string())
        .with_description("Demonstrating HTTP request lifecycle events in HyperChad".to_string());

    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset).unwrap();
    }

    app.build_default()?.run()?;

    Ok(())
}
