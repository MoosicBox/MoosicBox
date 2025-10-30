#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic FLTK Window Example
//!
//! This example demonstrates how to create a simple desktop window using the
//! HyperChad FLTK renderer. It shows the minimal setup required to initialize
//! the renderer, create a basic UI with text and buttons, and run the event loop.

use hyperchad_color::Color;
use hyperchad_renderer::{Renderer, ToRenderRunner, View};
use hyperchad_renderer_fltk::FltkRenderer;
use hyperchad_template::container;

#[switchy::main(tokio)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see renderer debug messages
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting FLTK Basic Window Example");

    // Create a communication channel for actions triggered by UI elements
    // This allows the UI to send messages back to your application
    let (action_tx, _action_rx) = flume::unbounded();

    // Create a new FLTK renderer instance
    log::info!("Initializing FLTK renderer");
    let mut renderer = FltkRenderer::new(action_tx);

    // Initialize the window with desired properties
    // Parameters: width, height, x_position, y_position, background_color, title, description, viewport
    renderer
        .init(
            800.0,                                         // Window width in pixels
            600.0,                                         // Window height in pixels
            None,                                          // X position (None = center screen)
            None,                                          // Y position (None = center screen)
            Some(Color::from_hex("#181a1b")),              // Dark background color
            Some("HyperChad FLTK - Basic Window Example"), // Window title
            Some("A simple desktop window example"),       // Description
            None,                                          // Viewport configuration
        )
        .await?;

    log::info!("Creating UI layout");

    // Build the UI using HyperChad's template macro
    // This creates a hierarchical layout similar to HTML/CSS
    let view = container! {
        // Root container with full window dimensions
        div
            width=780      // Leave padding for window edges
            height=580
            direction="column"  // Stack children vertically
            padding=20
            gap=15              // Space between child elements
        {
            // Header section with title
            h1
                font-size=32
                text-align="center"
                margin-bottom=10
            {
                "Welcome to FLTK!"
            }

            // Description text
            div
                text-align="center"
                margin-bottom=20
            {
                "This is a basic desktop window rendered using the HyperChad FLTK renderer."
            }

            // Information section with styled background
            div
                width=740
                padding=20
                background="#2d2d2d"
                direction="column"
                gap=10
            {
                h2
                    font-size=24
                    margin-bottom=10
                {
                    "Features Demonstrated:"
                }

                // List of features
                div
                    direction="column"
                    gap=8
                {
                    div { "✓ Window initialization with custom size and position" }
                    div { "✓ Hierarchical layout using flexbox (column direction)" }
                    div { "✓ Text rendering with different heading levels" }
                    div { "✓ Custom colors and styling" }
                    div { "✓ Padding and gap spacing" }
                    div { "✓ FLTK native event loop" }
                }
            }

            // Info box
            div
                width=740
                padding=15
                background="#1e3a5f"
                margin-top=10
            {
                div
                    font-size=14
                {
                    "This window is running on FLTK (Fast Light Toolkit), "
                    "a lightweight cross-platform GUI library. "
                    "The UI is defined using HyperChad's template system and "
                    "rendered as native FLTK widgets."
                }
            }

            // Footer with instructions
            div
                text-align="center"
                margin-top=20
                font-size=12
                color="#888888"
            {
                "Close this window to exit the application"
            }
        }
    };

    log::info!("Rendering view");
    // Render the view to the window
    renderer.render(View::from(view)).await?;

    log::info!("Starting event loop");
    // Convert renderer to a runner and start the FLTK event loop
    // This blocks until the window is closed
    let mut runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run()?;

    log::info!("Application shutting down");
    Ok(())
}
