//! Native desktop application entry point for the `MoosicBox` marketing site.
//!
//! This binary provides a native desktop application for the marketing site,
//! supporting multiple rendering backends including FLTK and egui. It configures
//! logging, async runtime, and window parameters from environment variables.
//!
//! # Environment Variables
//!
//! * `WINDOW_WIDTH` - Window width in pixels (default: 1000.0)
//! * `WINDOW_HEIGHT` - Window height in pixels (default: 600.0)
//! * `WINDOW_X` - Window X position in pixels (optional)
//! * `WINDOW_Y` - Window Y position in pixels (optional)
//! * `MAX_THREADS` - Maximum blocking threads for async runtime (default: 64)
//! * `TOKIO_CONSOLE` - Enable tokio console subscriber when set to "1" or "true" (requires `console-subscriber` feature)

// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use moosicbox_marketing_site::{ROUTER, VIEWPORT};
use switchy_env::{var_parse_opt, var_parse_or};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(feature = "profiling-tracing") {
        // no global tracing defined here
    } else {
        #[allow(unused_mut)]
        let mut layers = vec![];

        #[cfg(feature = "console-subscriber")]
        if matches!(
            switchy_env::var("TOKIO_CONSOLE").as_deref(),
            Ok("1" | "true")
        ) {
            use moosicbox_logging::free_log_client::DynLayer;

            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(target_os = "android")]
        let filename = None;
        #[cfg(not(target_os = "android"))]
        let filename = Some("moosicbox_app_native.log");

        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    }

    let threads = var_parse_or("MAX_THREADS", 64usize);
    log::debug!("Running with {threads} max blocking threads");
    let runtime = switchy_async::runtime::Builder::new()
        .max_blocking_threads(u16::try_from(threads).unwrap())
        .build()?;
    let runtime = Arc::new(runtime);

    let app = moosicbox_marketing_site::init()
        .with_viewport(VIEWPORT.clone())
        .with_router(ROUTER.clone())
        .with_runtime_handle(runtime.handle());

    let mut builder = app.with_size(
        var_parse_opt::<f32>("WINDOW_WIDTH")
            .unwrap_or(None)
            .unwrap_or(1000.0),
        var_parse_opt::<f32>("WINDOW_HEIGHT")
            .unwrap_or(None)
            .unwrap_or(600.0),
    );

    if let (Some(x), Some(y)) = (
        var_parse_opt::<i32>("WINDOW_X").unwrap_or(None),
        var_parse_opt::<i32>("WINDOW_Y").unwrap_or(None),
    ) {
        builder.position(x, y);
    }

    #[cfg(any(feature = "_egui", feature = "fltk"))]
    builder.initial_route("/");

    moosicbox_marketing_site::build_app(builder)?.run()?;

    Ok(())
}
