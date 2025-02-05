#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{path::PathBuf, sync::LazyLock};

use moosicbox_app_native_lib::{
    renderer::Color,
    router::{RoutePath, Router},
    NativeAppBuilder, RendererType,
};
use moosicbox_env_utils::option_env_f32;

mod download;

static DEFAULT_OUTPUT_DIR: &str = "gen";
static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
    LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

#[cfg(feature = "assets")]
static ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    CARGO_MANIFEST_DIR.as_ref().map_or_else(
        || <PathBuf as std::str::FromStr>::from_str("public").unwrap(),
        |dir| dir.join("public"),
    )
});

pub static ROUTER: LazyLock<Router> = LazyLock::new(|| {
    Router::new()
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
        })
});

#[cfg(feature = "assets")]
pub static ASSETS: LazyLock<Vec<gigachad_renderer::assets::StaticAssetRoute>> =
    LazyLock::new(|| {
        vec![
            moosicbox_app_native_lib::renderer::assets::StaticAssetRoute {
                route: "favicon.ico".to_string(),
                target: ASSETS_DIR.join("favicon.ico").try_into().unwrap(),
            },
            moosicbox_app_native_lib::renderer::assets::StaticAssetRoute {
                route: "public".to_string(),
                target: ASSETS_DIR.clone().try_into().unwrap(),
            },
        ]
    });

pub static BACKGROUND_COLOR: LazyLock<Color> = LazyLock::new(|| Color::from_hex("#181a1b"));

/// # Panics
///
/// * If an invalid number is given to `WINDOW_WIDTH` or `WINDOW_HEIGHT`
pub fn init() -> NativeAppBuilder {
    let app = moosicbox_app_native_lib::NativeAppBuilder::new()
        .with_router(ROUTER.clone())
        .with_background(*BACKGROUND_COLOR);

    #[allow(unused_mut)]
    let mut app = app.with_size(
        option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
        option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
    );

    #[cfg(feature = "assets")]
    {
        for assets in ASSETS.iter().cloned() {
            app = app.with_static_asset_route_result(assets).unwrap();
        }
    }

    app
}

/// # Errors
///
/// * IF
///
/// # Panics
///
/// * If failed to create a tokio runime
#[allow(clippy::too_many_lines, clippy::future_not_send, clippy::unused_async)]
pub async fn gen(
    #[allow(unused_variables)] renderer: RendererType,
    #[allow(unused_variables)] output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(cfg!(feature = "html"), "Must be an html renderer to gen");
    assert!(
        cfg!(feature = "static-routes"),
        "Must have `static-routes` enabled to gen"
    );

    #[cfg(all(feature = "html", feature = "static-routes"))]
    {
        use std::sync::Arc;

        use gigachad_renderer::HtmlTagRenderer;
        use gigachad_renderer_html::html::container_element_to_html_response;
        use moosicbox_app_native_lib::router::{ClientInfo, ClientOs, RequestInfo, RouteRequest};
        use tokio::io::AsyncWriteExt as _;

        let output = output.unwrap_or_else(|| {
            CARGO_MANIFEST_DIR
                .as_ref()
                .and_then(|x| x.join(DEFAULT_OUTPUT_DIR).to_str().map(ToString::to_string))
                .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string())
        });
        let output_path: PathBuf = output.into();
        let static_routes = ROUTER.static_routes.read().unwrap().clone();

        let tag_renderer: Option<Arc<Box<dyn HtmlTagRenderer + Send + Sync>>> = renderer.into();
        let tag_renderer = tag_renderer.unwrap();

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
                    client: std::sync::Arc::new(ClientInfo {
                        os: ClientOs {
                            name: "n/a".to_string(),
                        },
                    }),
                },
            };

            match handler(req).await {
                Ok(view) => {
                    let html = container_element_to_html_response(
                        &std::collections::HashMap::new(),
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
                    return Err(e);
                }
            }
        }

        #[cfg(feature = "assets")]
        {
            use moosicbox_app_native_lib::renderer::assets::AssetPathTarget;

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

            for route in ASSETS.iter() {
                let assets_output = output_path.join(&route.route);
                tokio::fs::create_dir_all(&assets_output.parent().unwrap())
                    .await
                    .expect("Failed to create dirs");
                match &route.target {
                    AssetPathTarget::File(file) => {
                        tokio::fs::copy(file, &assets_output).await?;
                    }
                    AssetPathTarget::Directory(dir) => {
                        copy_dir_all(dir, &assets_output).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// # Errors
///
/// * If the output directory fails to be deleted
pub fn clean(output: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}

/// # Panics
///
/// * If the `ROUTER.routes` `RwLock` fails to read
pub fn dynamic_routes() {
    let static_routes = ROUTER.routes.read().unwrap().clone();

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
}
