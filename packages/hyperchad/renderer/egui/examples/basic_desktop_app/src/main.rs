#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic desktop application example using the `HyperChad` egui renderer.
//!
//! This example demonstrates how to create a simple desktop application with:
//! - Custom layout calculator with font metrics
//! - Interactive UI elements (buttons)
//! - Action handling
//! - Window initialization
//!
//! Run with: `cargo run --manifest-path packages/hyperchad/renderer/egui/examples/basic_desktop_app/Cargo.toml`

use std::sync::Arc;

use flume::unbounded;
use hyperchad_actions::logic::Value;
use hyperchad_renderer::{
    Handle, Renderer, ToRenderRunner, View,
    transformer::layout::calc::{Calculator, CalculatorDefaults},
};
use hyperchad_renderer_egui::{
    EguiRenderer, eframe::egui, font_metrics::EguiFontMetrics, layout::EguiCalc,
};
use hyperchad_router::{ClientInfo, Container, RouteRequest, Router};
use hyperchad_template::container;
use log::info;
use switchy_async::runtime;

/// Calculator that implements both `Calc` and `EguiCalc` traits.
///
/// This calculator integrates egui font metrics with `HyperChad`'s layout system,
/// providing accurate text measurement and layout calculations for desktop rendering.
#[derive(Clone)]
struct MyCalculator {
    /// Inner calculator with font metrics. Initialized when egui context is available.
    inner: Option<Arc<Calculator<EguiFontMetrics>>>,
}

impl MyCalculator {
    /// Creates a new calculator without egui context.
    ///
    /// The actual calculator will be initialized when `with_context` is called
    /// by the renderer during startup.
    const fn new() -> Self {
        Self { inner: None }
    }
}

impl hyperchad_transformer::layout::Calc for MyCalculator {
    fn calc(&self, container: &mut Container) -> bool {
        // Delegate to the inner calculator once initialized
        self.inner.as_ref().unwrap().calc(container)
    }
}

impl EguiCalc for MyCalculator {
    fn with_context(mut self, context: egui::Context) -> Self {
        // Initialize the calculator with egui font metrics
        // Using a scaling factor (DELTA) to adjust font sizes for desktop rendering
        const DELTA: f32 = 14.0 / 16.0;

        self.inner = Some(Arc::new(Calculator::new(
            EguiFontMetrics::new(context),
            CalculatorDefaults {
                // Base font settings
                font_size: 16.0 * DELTA,
                font_margin_top: 0.0,
                font_margin_bottom: 0.0,
                // H1 heading settings
                h1_font_size: 32.0 * DELTA,
                h1_font_margin_top: 21.44 * DELTA,
                h1_font_margin_bottom: 21.44 * DELTA,
                // H2 heading settings
                h2_font_size: 24.0 * DELTA,
                h2_font_margin_top: 19.92 * DELTA,
                h2_font_margin_bottom: 19.92 * DELTA,
                // H3 heading settings
                h3_font_size: 18.72 * DELTA,
                h3_font_margin_top: 18.72 * DELTA,
                h3_font_margin_bottom: 18.72 * DELTA,
                // H4 heading settings
                h4_font_size: 16.0 * DELTA,
                h4_font_margin_top: 21.28 * DELTA,
                h4_font_margin_bottom: 21.28 * DELTA,
                // H5 heading settings
                h5_font_size: 13.28 * DELTA,
                h5_font_margin_top: 22.1776 * DELTA,
                h5_font_margin_bottom: 22.1776 * DELTA,
                // H6 heading settings
                h6_font_size: 10.72 * DELTA,
                h6_font_margin_top: 24.9776 * DELTA,
                h6_font_margin_bottom: 24.9776 * DELTA,
            },
        )));

        self
    }
}

/// Main application entry point.
///
/// Sets up the async runtime, creates the renderer, and runs the desktop application.
#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting HyperChad egui basic desktop app example");

    // Create async runtime for async operations
    let runtime = runtime::Builder::new().build()?;

    runtime.block_on(async {
        // Create communication channels
        // action_tx: Send action events from UI to handlers
        // resize_tx: Send window resize events
        let (action_tx, action_rx) = unbounded();
        let (resize_tx, resize_rx) = unbounded();

        // Create router for navigation and routing
        let router = Router::new().with_route("/", |_req: RouteRequest| async move {
            // Create the UI view using HyperChad template macros
            View::builder()
                .with_primary(container! {
                    div
                        width=800
                        height=600
                        background="#f0f0f0"
                        direction="column"
                        padding=20
                        gap=20
                    {
                        h1
                            color="#2563eb"
                        {
                            "Welcome to HyperChad!"
                        }

                        div
                            background="white"
                            padding=20
                            border-radius=8
                            gap=10
                        {
                            span
                                font-size=14
                                color="#64748b"
                            {
                                "This is a basic desktop application built with HyperChad and egui."
                            }
                        }

                        div
                            direction="row"
                            gap=10
                        {
                            button
                                background="#10b981"
                                color="white"
                                padding=10
                                border-radius=6
                                fx-click=fx { show("message") }
                            {
                                "Show Message"
                            }

                            button
                                background="#ef4444"
                                color="white"
                                padding=10
                                border-radius=6
                                fx-click=fx { hide("message") }
                            {
                                "Hide Message"
                            }
                        }

                        div
                            id="message"
                            background="#fef3c7"
                            padding=15
                            border-radius=6
                            visibility="hidden"
                        {
                            span
                                color="#92400e"
                            {
                                "Hello from HyperChad! Click the buttons above to toggle this message."
                            }
                        }
                    }
                })
                .build()
        });

        // Create client info (default configuration)
        let client_info = Arc::new(ClientInfo::default());

        // Create layout calculator
        let calculator = MyCalculator::new();

        // Create egui renderer
        let mut renderer = EguiRenderer::new(
            router.clone(),
            action_tx,
            resize_tx,
            client_info,
            calculator,
        );

        info!("Initializing window...");

        // Initialize the window
        renderer
            .init(
                800.0,                           // width
                600.0,                           // height
                None,                            // x position (centered)
                None,                            // y position (centered)
                None,                            // background color
                Some("HyperChad Basic Example"), // window title
                Some("A basic desktop application using HyperChad and egui"), // description
                None, // viewport
            )
            .await
            .map_err(|e| format!("Failed to initialize window: {e}"))?;

        info!("Window initialized");

        // Navigate to root route to render the UI
        router
            .navigate("/")
            .await
            .map_err(|e| format!("Failed to navigate to root route: {e}"))?;

        info!("Navigated to root route");

        // Spawn task to handle action events from the UI
        Handle::current().spawn(async move {
            info!("Action handler started");
            while let Ok((_action_name, value)) = action_rx.recv_async().await {
                // In a real app, you would handle different action types here
                info!("Received action: {value:?}");

                // Example: handle custom actions
                if let Some(Value::String(action)) = value {
                    match action.as_str() {
                        "show" | "hide" => {
                            info!("Toggle visibility action");
                        }
                        _ => {
                            info!("Unknown action: {action}");
                        }
                    }
                }
            }
            info!("Action handler stopped");
        });

        // Spawn task to handle window resize events
        Handle::current().spawn(async move {
            info!("Resize handler started");
            while let Ok((width, height)) = resize_rx.recv_async().await {
                info!("Window resized to: {width}x{height}");
            }
            info!("Resize handler stopped");
        });

        info!("Creating render runner...");

        // Create and run the application
        let mut runner = renderer
            .to_runner(Handle::current())
            .map_err(|e| format!("Failed to create runner: {e}"))?;

        info!("Running application...");

        // This blocks until the window is closed
        runner
            .run()
            .map_err(|e| format!("Failed to run application: {e}"))?;

        info!("Application closed");

        Ok(())
    })
}
