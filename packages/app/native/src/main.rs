//! Main entry point for the `MoosicBox` native desktop application.
//!
//! This executable initializes the application runtime, logging, router, and UI framework.
//! It handles platform-specific configurations and manages the application lifecycle from
//! startup through shutdown.
//!
//! # Features
//!
//! * **Runtime initialization** - Configures async runtime with configurable thread pool
//! * **Logging** - Initializes file and console logging with optional console subscriber for tokio
//! * **UI frameworks** - Supports multiple rendering backends (HTML, FLTK, egui)
//! * **Bundled mode** - Optional embedded server for standalone operation
//! * **Visualization** - Real-time audio waveform display (with `_canvas` feature)
//!
//! # Platform Support
//!
//! * **Windows** - Hides console window in release builds (unless `windows-console` feature is enabled)
//! * **Android** - Logging configured without file output
//! * **Desktop** - Full feature set with file logging

// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use flume::SendError;
use hyperchad::{actions::logic::Value, app::AppBuilder, renderer::Color};
use moosicbox_app_native::{
    PROFILE, RENDERER, STATE, STATE_LOCK, actions::handle_action, init_app_state,
};
use moosicbox_app_native_ui::Action;
use switchy_env::{var_parse_opt, var_parse_or};

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
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

    #[cfg(all(feature = "html", feature = "_canvas"))]
    moosicbox_app_native::visualization::disable_interval();

    let threads = var_parse_or("MAX_THREADS", 64usize);
    log::debug!("Running with {threads} max blocking threads");

    let runtime = switchy::unsync::runtime::Builder::new()
        .max_blocking_threads(u16::try_from(threads).unwrap())
        .build()
        .unwrap();

    let runtime = Arc::new(runtime);

    let router = moosicbox_app_native::init();

    let (action_tx, action_rx) = flume::unbounded();

    let width = var_parse_opt::<f32>("WINDOW_WIDTH")
        .unwrap_or(None)
        .unwrap_or(1000.0);
    let height = var_parse_opt::<f32>("WINDOW_HEIGHT")
        .unwrap_or(None)
        .unwrap_or(600.0);

    let mut app = AppBuilder::new()
        .with_title("MoosicBox".to_string())
        .with_description("A music app for cows".to_string())
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_background(Color::from_hex("#181a1b"))
        .with_action_handler(move |x, value| {
            Ok::<_, SendError<(Action, Option<Value>)>>(match Action::try_from(x) {
                Ok(action) => {
                    action_tx.send((action, value.cloned()))?;
                    true
                }
                Err(e) => {
                    log::error!("Failed to handle action: {e:?}");
                    false
                }
            })
        })
        .with_size(width, height);

    #[cfg(any(feature = "egui", feature = "fltk"))]
    app.initial_route("/");

    #[cfg(feature = "_canvas")]
    moosicbox_app_native::visualization::set_dimensions(
        width,
        f32::from(moosicbox_app_native_ui::VIZ_HEIGHT),
    );

    #[cfg(feature = "assets")]
    for asset in moosicbox_app_native::assets::ASSETS.iter().cloned() {
        log::trace!("app_native: adding static asset route: {asset:?}");
        app = app.with_static_asset_route_result(asset).unwrap();
    }

    let state = runtime
        .block_on(async move { init_app_state(moosicbox_app_state::AppState::new()).await })
        .unwrap();

    STATE_LOCK.set(state).unwrap();

    runtime.spawn(async move {
        while let Ok((action, value)) = action_rx.recv_async().await {
            if let Err(e) = handle_action(action, value).await {
                log::error!("Failed to handle action: {e:?}");
            }
        }
    });

    #[cfg(feature = "bundled")]
    let (join_app_server, app_server_handle) = {
        use moosicbox_app_native_bundled::service::Commander as _;

        log::debug!("Starting app server");

        let context = moosicbox_app_native_bundled::Context::new(&runtime.handle());
        let server = moosicbox_app_native_bundled::service::Service::new(context);

        let app_server_handle = server.handle();
        let (tx, rx) = switchy::unsync::sync::oneshot::channel();

        let join_app_server = server.start_on(&runtime.handle());

        app_server_handle
            .send_command(moosicbox_app_native_bundled::Command::WaitForStartup { sender: tx })
            .expect("Failed to send WaitForStartup command");

        log::debug!("Waiting for app server to start");

        runtime.block_on(rx).expect("Failed to start app server");

        log::debug!("App server started");

        (join_app_server, app_server_handle)
    };

    if let (Some(x), Some(y)) = (
        var_parse_opt::<i32>("WINDOW_X").unwrap_or(None),
        var_parse_opt::<i32>("WINDOW_Y").unwrap_or(None),
    ) {
        app = app.with_position(x, y);
    }
    log::debug!("app_native: setting up routes");

    log::debug!("app_native: creating app");
    let app = app.build_default()?;

    moosicbox_assert::assert_or_panic!(
        RENDERER.set(Box::new(app.renderer.clone())).is_ok(),
        "Already set RENDERER"
    );

    runtime.spawn(async move {
        let api_url = STATE
            .get_current_connection()
            .await
            .unwrap()
            .map(|x| x.api_url);
        let connection_name = STATE.get_connection_name().await.unwrap();
        let connection_id = STATE.get_or_init_connection_id().await.unwrap();

        STATE
            .set_state(moosicbox_app_state::UpdateAppState {
                connection_id: Some(Some(connection_id)),
                connection_name: Some(connection_name),
                api_url: Some(api_url),
                profile: Some(Some(PROFILE.to_string())),
                ..Default::default()
            })
            .await?;

        Ok::<_, moosicbox_app_state::AppStateError>(())
    });

    log::debug!("app_native: running");
    app.run()?;

    #[cfg(feature = "bundled")]
    {
        use moosicbox_app_native_bundled::service::Commander as _;

        log::debug!("Shutting down app server..");
        if let Err(e) = app_server_handle.shutdown() {
            moosicbox_assert::die_or_error!("AppServer failed to shutdown: {e:?}");
        }

        log::debug!("Joining app server...");
        match runtime.block_on(join_app_server) {
            Err(e) => {
                moosicbox_assert::die_or_error!("Failed to join app server: {e:?}");
            }
            Ok(Err(e)) => {
                moosicbox_assert::die_or_error!("Failed to join app server: {e:?}");
            }
            _ => {}
        }
    }

    Ok(())
}
