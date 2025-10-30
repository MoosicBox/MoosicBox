#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic interactive example demonstrating the `HyperChad` Vanilla JS Renderer.
//!
//! This example shows how to use the vanilla JavaScript renderer to create an interactive
//! web application with client-side actions, event handling, and dynamic UI updates.

use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{RouteRequest, Router},
    template::{self as hyperchad_template, Containers, container},
};
use log::info;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

/// Static assets served by the application (JavaScript runtime).
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

/// Creates the main page demonstrating various interactive features.
#[allow(clippy::too_many_lines)]
fn create_main_page() -> Containers {
    container! {
        div class="page" {
            // Header section
            header
                class="header"
                padding=24
                background="#2563eb"
                color=white
                text-align=center
            {
                h1 { "HyperChad Vanilla JS Renderer" }
                span { "Interactive Web Components with Vanilla JavaScript" }
            }

            // Main content area
            div
                direction=row
                justify-content=center
                width=100%
            {
                main
                    class="main"
                    padding=24
                    max-width=900
                    width=100%
                    gap=24
                {
                    // Section 1: Show/Hide Actions
                    section
                        class="visibility-demo"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Show/Hide Actions" }
                        span color="#6b7280" {
                            "Demonstrate showing and hiding elements with client-side actions"
                        }

                        div gap=12 margin-top=16 {
                            div direction=row gap=8 {
                                button
                                    fx-click=fx { show("message-box") }
                                    class="btn-primary"
                                    padding=12
                                    background="#10b981"
                                    color=white
                                    border-radius=6
                                    cursor=pointer
                                {
                                    "Show Message"
                                }

                                button
                                    fx-click=fx { hide("message-box") }
                                    class="btn-secondary"
                                    padding=12
                                    background="#ef4444"
                                    color=white
                                    border-radius=6
                                    cursor=pointer
                                {
                                    "Hide Message"
                                }
                            }

                            div
                                id="message-box"
                                padding=16
                                background="#dcfce7"
                                color="#166534"
                                border-radius=6
                                margin-top=12
                                visibility=hidden
                            {
                                "✓ This message can be shown or hidden using the buttons above!"
                            }
                        }
                    }

                    // Section 2: Display Actions
                    section
                        class="display-demo"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Display Actions" }
                        span color="#6b7280" {
                            "Control element display property for dynamic layouts"
                        }

                        div gap=12 margin-top=16 {
                            div direction=row gap=8 {
                                button
                                    fx-click=fx { display("content-panel") }
                                    padding=12
                                    background="#8b5cf6"
                                    color=white
                                    border-radius=6
                                    cursor=pointer
                                {
                                    "Show Content"
                                }

                                button
                                    fx-click=fx { no_display("content-panel") }
                                    padding=12
                                    background="#ec4899"
                                    color=white
                                    border-radius=6
                                    cursor=pointer
                                {
                                    "Hide Content"
                                }
                            }

                            div
                                id="content-panel"
                                padding=16
                                background="#fef3c7"
                                color="#92400e"
                                border-radius=6
                                margin-top=12
                                hidden
                            {
                                "📦 This content panel is hidden by default. "
                                "Unlike visibility, hidden elements don't take up space in the layout."
                            }
                        }
                    }

                    // Section 3: Multiple Actions
                    section
                        class="multi-action-demo"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Multiple Actions" }
                        span color="#6b7280" {
                            "Execute multiple actions with a single click"
                        }

                        div gap=12 margin-top=16 {
                            button
                                fx-click=fx {
                                    show("panel-1");
                                    show("panel-2");
                                    log("Panels shown!")
                                }
                                padding=12
                                background="#059669"
                                color=white
                                border-radius=6
                                cursor=pointer
                            {
                                "Show All Panels"
                            }

                            button
                                fx-click=fx {
                                    hide("panel-1");
                                    hide("panel-2");
                                    log("Panels hidden!")
                                }
                                padding=12
                                background="#dc2626"
                                color=white
                                border-radius=6
                                cursor=pointer
                                margin-left=8
                            {
                                "Hide All Panels"
                            }

                            div
                                id="panel-1"
                                padding=16
                                background="#dcfce7"
                                border-radius=6
                                margin-top=12
                                visibility=hidden
                            {
                                "Panel 1: Executing multiple actions in sequence"
                            }

                            div
                                id="panel-2"
                                padding=16
                                background="#dbeafe"
                                border-radius=6
                                margin-top=8
                                visibility=hidden
                            {
                                "Panel 2: All actions are processed in order"
                            }
                        }
                    }

                    // Section 4: Logging Actions
                    section
                        class="logging-demo"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Logging Actions" }
                        span color="#6b7280" {
                            "Log messages to the browser console (open DevTools to see)"
                        }

                        div gap=12 margin-top=16 {
                            button
                                fx-click=fx { log("Hello from HyperChad!") }
                                padding=12
                                background="#3b82f6"
                                color=white
                                border-radius=6
                                cursor=pointer
                            {
                                "Log Message"
                            }

                            button
                                fx-click=fx {
                                    log("Info: Button clicked");
                                    show("log-indicator")
                                }
                                padding=12
                                background="#f59e0b"
                                color=white
                                border-radius=6
                                cursor=pointer
                                margin-left=8
                            {
                                "Log & Show"
                            }

                            div
                                id="log-indicator"
                                padding=12
                                background="#fef3c7"
                                color="#92400e"
                                border-radius=6
                                margin-top=12
                                visibility=hidden
                            {
                                "✓ Check the browser console (F12) to see the logged message!"
                            }
                        }
                    }

                    // Info section
                    section
                        class="info-section"
                        padding=24
                        background="#eff6ff"
                        border-radius=8
                        gap=16
                    {
                        h3 { "Key Features Demonstrated" }
                        ul padding-left=20 gap=8 {
                            li { "Visibility actions (show, hide)" }
                            li { "Display actions (display, no_display)" }
                            li { "Logging actions (log)" }
                            li { "Multiple actions in sequence" }
                            li { "Event triggers (fx-click)" }
                            li { "Pure vanilla JavaScript - no framework dependencies" }
                        }

                        h3 margin-top=16 { "Open Browser DevTools" }
                        span {
                            "Press F12 to open the browser console and see logged messages. "
                            "This is useful for debugging and understanding action execution."
                        }
                    }
                }
            }

            // Footer
            footer
                class="footer"
                padding=24
                text-align=center
                background="#f3f4f6"
            {
                span { "Built with HyperChad Vanilla JS Renderer" }
            }
        }
    }
}

/// Creates the application router with a single root route.
fn create_router() -> Router {
    Router::new().with_route("/", |_req: RouteRequest| async move {
        View::builder().with_primary(create_main_page()).build()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    info!("Starting HyperChad Vanilla JS Interactive Example");

    // Create async runtime
    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    // Create router
    let router = create_router();

    info!("Server running on http://localhost:8080");
    info!("Press Ctrl+C to stop");

    // Build and configure the application
    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad Vanilla JS - Interactive Example".to_string())
        .with_description(
            "Demonstrating interactive features of the HyperChad Vanilla JS Renderer".to_string(),
        );

    // Register static assets (JavaScript runtime)
    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset)?;
    }

    // Run the application
    app.build_default()?.run()?;

    Ok(())
}
