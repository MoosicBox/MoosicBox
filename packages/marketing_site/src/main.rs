// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::Arc;

use clap::{Parser, Subcommand};
use moosicbox_env_utils::{default_env_usize, option_env_f32, option_env_i32};
use moosicbox_marketing_site::VIEWPORT;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum Commands {
    DynamicRoutes,
    Gen {
        #[arg(short, long)]
        output: Option<String>,
    },
    Clean {
        #[arg(short, long)]
        output: Option<String>,
    },
    Serve,
}

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

    let args = Args::parse();

    match args.cmd {
        Commands::DynamicRoutes => {
            moosicbox_marketing_site::dynamic_routes();
            return Ok(());
        }
        Commands::Clean { output } => {
            return moosicbox_marketing_site::clean(output);
        }
        Commands::Gen { .. } | Commands::Serve => {
            let is_gen = matches!(args.cmd, Commands::Gen { .. });
            let mut runtime = tokio::runtime::Builder::new_multi_thread();
            let runtime = runtime.enable_all();
            let runtime = if is_gen {
                runtime
            } else {
                let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
                log::debug!("Running with {threads} max blocking threads");
                runtime.max_blocking_threads(threads)
            };
            let runtime = runtime.build().unwrap();
            let runtime = Arc::new(runtime);

            let app = moosicbox_marketing_site::init()
                .with_viewport(VIEWPORT.clone())
                .with_runtime_arc(runtime.clone());

            if let Commands::Gen { output } = args.cmd {
                return runtime.block_on(async move {
                    let renderer = moosicbox_marketing_site::start(app).await?.renderer;
                    moosicbox_marketing_site::gen(renderer, output).await
                });
            }

            let mut builder = app.with_size(
                option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
                option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
            );

            #[cfg(feature = "assets")]
            {
                for assets in moosicbox_marketing_site::ASSETS.iter().cloned() {
                    builder = builder.with_static_asset_route_result(assets)?;
                }
            }

            let mut runner = runtime.block_on(async move {
                if let (Some(x), Some(y)) = (
                    option_env_i32("WINDOW_X").unwrap(),
                    option_env_i32("WINDOW_Y").unwrap(),
                ) {
                    builder = builder.with_position(x, y);
                }

                log::debug!("app_native: starting app");
                let app = moosicbox_marketing_site::start(builder)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

                #[cfg(any(feature = "egui", feature = "fltk"))]
                {
                    log::debug!("app_native: navigating to home");
                    let _handle = app.router.navigate_spawn(
                        "/",
                        moosicbox_app_native_lib::router::RequestInfo {
                            client: moosicbox_app_native_lib::CLIENT_INFO.clone(),
                        },
                    );
                }

                app.to_runner()
            })?;

            log::debug!("app_native: running");
            runner.run().unwrap();
        }
    }

    Ok(())
}
