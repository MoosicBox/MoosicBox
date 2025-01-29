// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use clap::{Parser, Subcommand};
use moosicbox_app_native_lib::{
    renderer::{Color, Renderer},
    router::{ClientInfo, ClientOs, RequestInfo, RouteRequest, Router},
};
use moosicbox_env_utils::{default_env_usize, option_env_f32, option_env_i32};
use moosicbox_logging::free_log_client::DynLayer;
use tokio::{io::AsyncWriteExt as _, sync::RwLock};

mod download;

static ROUTER: OnceLock<Router> = OnceLock::new();
static RENDERER: OnceLock<Arc<RwLock<Box<dyn Renderer>>>> = OnceLock::new();

static DEFAULT_OUTPUT_DIR: &str = "gen";
static CARGO_MANIFEST_DIR: std::sync::LazyLock<Option<std::path::PathBuf>> =
    std::sync::LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum Commands {
    Gen {
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
        let mut layers = vec![];

        if std::env::var("TOKIO_CONSOLE") == Ok("1".to_string()) {
            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(target_os = "android")]
        let filename = None;
        #[cfg(not(target_os = "android"))]
        let filename = Some("moosicbox_app_native.log");

        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    }

    let args = Args::parse();
    log::info!("args={args:?}");

    let router = Router::new()
        .with_static_route(&["/", "/home"], |_| async {
            moosicbox_marketing_site_ui::home()
        })
        .with_static_route(&["/download"], |_| async {
            moosicbox_marketing_site_ui::download::download()
        })
        .with_route_result(&["/releases"], |req| async {
            download::releases_route(req).await
        });

    moosicbox_assert::assert_or_panic!(ROUTER.set(router.clone()).is_ok(), "Already set ROUTER");

    if let Commands::Gen { output } = args.cmd {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async move {
            let output = output.unwrap_or_else(|| {
                CARGO_MANIFEST_DIR
                    .as_ref()
                    .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                    .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
            });
            let output_path: PathBuf = output.into();
            let router = ROUTER.get().unwrap().clone();
            let static_routes = router.static_routes.read().unwrap().clone();

            for (path, handler) in &static_routes {
                let path_str = match path {
                    moosicbox_app_native_lib::router::RoutePath::Literal(path) => path,
                    moosicbox_app_native_lib::router::RoutePath::Literals(paths) => {
                        if let Some(path) = paths.first() {
                            path
                        } else {
                            continue;
                        }
                    }
                };
                let path_str = path_str.strip_prefix('/').unwrap_or(path_str);
                let path_str = if path_str.is_empty() {
                    "index"
                } else {
                    path_str
                };

                let req = RouteRequest {
                    path: path_str.to_string(),
                    query: HashMap::new(),
                    info: RequestInfo {
                        client: Arc::new(ClientInfo {
                            os: ClientOs {
                                name: "n/a".to_string(),
                            },
                        }),
                    },
                };

                match handler(req).await {
                    Ok(view) => {
                        let html = view.immediate.to_string();
                        let output_path = output_path.join(format!("{path_str}.html"));
                        tokio::fs::create_dir_all(&output_path.parent().unwrap())
                            .await
                            .expect("Failed to create dirs");

                        log::debug!("gen path={path_str} -> {output_path:?}\n{html}");

                        let mut file = tokio::fs::File::options()
                            .truncate(true)
                            .write(true)
                            .create(true)
                            .open(&output_path)
                            .await
                            .expect("Failed to open file");

                        file.write_all(html.as_bytes())
                            .await
                            .expect("Failed to write file");
                    }
                    Err(e) => {
                        panic!("Failed to fetch route view: {e:?}");
                    }
                }
            }
        });

        return Ok(());
    }

    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(threads)
        .build()
        .unwrap();

    let runtime = Arc::new(runtime);

    let mut app = moosicbox_app_native_lib::NativeAppBuilder::new()
        .with_router(router)
        .with_runtime_arc(runtime.clone())
        .with_background(Color::from_hex("#181a1b"))
        .with_size(
            option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
            option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
        );

    #[cfg(feature = "assets")]
    {
        app = app.with_static_asset_route_result(
            moosicbox_app_native_lib::renderer::assets::StaticAssetRoute {
                route: "public".to_string(),
                target: CARGO_MANIFEST_DIR
                    .as_ref()
                    .map_or_else(
                        || "public".to_string(),
                        |dir| dir.join("public").to_str().unwrap().to_string(),
                    )
                    .try_into()?,
            },
        )?;
    }

    let runner_runtime = runtime;

    let mut runner = runner_runtime.block_on(async move {
        if let (Some(x), Some(y)) = (
            option_env_i32("WINDOW_X").unwrap(),
            option_env_i32("WINDOW_Y").unwrap(),
        ) {
            app = app.with_position(x, y);
        }
        log::debug!("app_native: setting up routes");

        log::debug!("app_native: starting app");
        let app = app
            .start()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

        moosicbox_assert::assert_or_panic!(
            RENDERER.set(app.renderer.clone()).is_ok(),
            "Already set RENDERER"
        );

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

        app.to_runner().await
    })?;

    log::debug!("app_native: running");
    runner.run().unwrap();

    Ok(())
}
