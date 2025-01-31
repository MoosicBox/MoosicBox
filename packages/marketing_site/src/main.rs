// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    path::PathBuf,
    sync::{Arc, LazyLock, OnceLock},
};

use clap::{Parser, Subcommand};
use moosicbox_app_native_lib::{
    renderer::{Color, Renderer},
    router::{RoutePath, Router},
};
use moosicbox_env_utils::{default_env_usize, option_env_f32, option_env_i32};
use moosicbox_logging::free_log_client::DynLayer;
use tokio::sync::RwLock;

mod download;

static ROUTER: OnceLock<Router> = OnceLock::new();
static RENDERER: OnceLock<Arc<RwLock<Box<dyn Renderer>>>> = OnceLock::new();

static DEFAULT_OUTPUT_DIR: &str = "gen";
static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
    LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

static BACKGROUND_COLOR: LazyLock<Color> = LazyLock::new(|| Color::from_hex("#181a1b"));

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

    #[cfg(feature = "assets")]
    let assets_dir = CARGO_MANIFEST_DIR.as_ref().map_or_else(
        || <PathBuf as std::str::FromStr>::from_str("public").unwrap(),
        |dir| dir.join("public"),
    );

    let args = Args::parse();

    let router = Router::new()
        .with_static_route(&["/", "/home"], |_| async {
            moosicbox_marketing_site_ui::home()
        })
        .with_static_route(&["/not-found"], |_| async {
            moosicbox_marketing_site_ui::not_found()
        })
        .with_static_route(&["/download"], |_| async {
            moosicbox_marketing_site_ui::download::download()
        })
        .with_route_result(&["/releases"], |req| async {
            download::releases_route(req).await
        });

    moosicbox_assert::assert_or_panic!(ROUTER.set(router.clone()).is_ok(), "Already set ROUTER");

    if let Commands::Clean { output } = args.cmd {
        let output = output.unwrap_or_else(|| {
            CARGO_MANIFEST_DIR
                .as_ref()
                .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
        });
        let output_path: PathBuf = output.into();

        if output_path.is_dir() {
            std::fs::remove_dir_all(&output_path)?;
        }

        return Ok::<_, Box<dyn std::error::Error>>(());
    }

    if args.cmd == Commands::DynamicRoutes {
        let static_routes = router.routes.read().unwrap().clone();

        for (path, _) in &static_routes {
            println!(
                "{}",
                match path {
                    RoutePath::Literal(path) => path,
                    RoutePath::Literals(paths) => {
                        if let Some(path) = paths.first() {
                            path
                        } else {
                            continue;
                        }
                    }
                }
            );
        }

        return Ok::<_, Box<dyn std::error::Error>>(());
    }

    #[cfg(not(feature = "_html"))]
    if let Commands::Gen { .. } = args.cmd {
        panic!("Must be an html renderer to gen");
    }

    #[cfg(all(feature = "_html", feature = "static-routes"))]
    if let Commands::Gen { output } = args.cmd {
        use gigachad_renderer_html::{html::container_element_to_html_response, HeaderMap};
        use moosicbox_app_native_lib::{
            router::{ClientInfo, ClientOs, RequestInfo, RouteRequest},
            RendererType,
        };
        use tokio::io::AsyncWriteExt as _;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let runtime = Arc::new(runtime);

        let output = output.unwrap_or_else(|| {
            CARGO_MANIFEST_DIR
                .as_ref()
                .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
        });
        let output_path: PathBuf = output.into();
        let router = ROUTER.get().unwrap().clone();
        let static_routes = router.static_routes.read().unwrap().clone();

        let app = moosicbox_app_native_lib::NativeAppBuilder::new()
            .with_router(router)
            .with_runtime_arc(runtime.clone())
            .with_background(*BACKGROUND_COLOR);

        runtime.block_on(async move {
            let tag_renderer = match app.get_renderer()? {
                #[cfg(feature = "egui")]
                RendererType::Egui(..) => panic!("Invalid renderer"),
                #[cfg(feature = "fltk")]
                RendererType::Fltk(..) => panic!("Invalid renderer"),
                #[cfg(feature = "html")]
                RendererType::Html(renderer) => renderer.app.tag_renderer,
                #[cfg(feature = "htmx")]
                RendererType::Htmx(renderer) => renderer.html_renderer.app.tag_renderer,
                #[cfg(feature = "datastar")]
                RendererType::Datastar(renderer) => renderer.html_renderer.app.tag_renderer,
                #[cfg(feature = "vanilla-js")]
                RendererType::VanillaJs(renderer) => renderer.html_renderer.app.tag_renderer,
            };

            if output_path.is_dir() {
                tokio::fs::remove_dir_all(&output_path).await?;
            }

            for (path, handler) in &static_routes {
                let path_str = match path {
                    RoutePath::Literal(path) => path,
                    RoutePath::Literals(paths) => {
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
                    query: std::collections::HashMap::new(),
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
                        let html = container_element_to_html_response(
                            &HeaderMap::new(),
                            &view.immediate,
                            Some(*BACKGROUND_COLOR),
                            &**tag_renderer,
                        )?;
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

            #[cfg(feature = "assets")]
            {
                use std::path::Path;

                #[async_recursion::async_recursion]
                async fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
                    tokio::fs::create_dir_all(&dst).await?;
                    let mut read_dir = tokio::fs::read_dir(src).await?;
                    while let Ok(Some(entry)) = read_dir.next_entry().await {
                        let ty = entry.file_type().await?;
                        if ty.is_dir() {
                            copy_dir_all(&entry.path(), &dst.join(entry.file_name())).await?;
                        } else {
                            tokio::fs::copy(entry.path(), dst.join(entry.file_name())).await?;
                        }
                    }
                    Ok(())
                }

                let assets_gen = output_path.join(assets_dir.file_name().unwrap());

                tokio::fs::create_dir_all(&assets_gen)
                    .await
                    .expect("Failed to create dirs");

                copy_dir_all(&assets_dir, &assets_gen).await?;
            }

            Ok::<_, Box<dyn std::error::Error>>(())
        })?;

        return Ok::<_, Box<dyn std::error::Error>>(());
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
        .with_background(*BACKGROUND_COLOR)
        .with_size(
            option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
            option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
        );

    #[cfg(feature = "assets")]
    {
        app = app.with_static_asset_route_result(
            moosicbox_app_native_lib::renderer::assets::StaticAssetRoute {
                route: "public".to_string(),
                target: assets_dir.try_into()?,
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
