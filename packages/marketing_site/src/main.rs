// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use moosicbox_env_utils::{default_env_usize, option_env_f32, option_env_i32};
use moosicbox_marketing_site::{ROUTER, VIEWPORT};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(feature = "profiling-tracing") {
        // no global tracing defined here
    } else {
        #[allow(unused_mut)]
        let mut layers = vec![];

        #[cfg(feature = "console-subscriber")]
        if std::env::var("TOKIO_CONSOLE").as_deref() == Ok("1") {
            use moosicbox_logging::free_log_client::DynLayer;

            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(target_os = "android")]
        let filename = None;
        #[cfg(not(target_os = "android"))]
        let filename = Some("moosicbox_app_native.log");

        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    }

    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");
    let runtime = switchy_async::runtime::Builder::new()
        .max_blocking_threads(u16::try_from(threads).unwrap())
        .build()?;
    let runtime = Arc::new(runtime);

    let app = moosicbox_marketing_site::init()
        .with_viewport(VIEWPORT.clone())
        .with_router(ROUTER.clone())
        .with_runtime_handle(runtime.handle().clone());

    let mut builder = app.with_size(
        option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
        option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
    );

    if let (Some(x), Some(y)) = (
        option_env_i32("WINDOW_X").unwrap(),
        option_env_i32("WINDOW_Y").unwrap(),
    ) {
        builder.position(x, y);
    }

    #[cfg(any(feature = "_egui", feature = "fltk"))]
    builder.initial_route("/");

    moosicbox_marketing_site::build_app(builder)?.run()?;

    Ok(())
}
