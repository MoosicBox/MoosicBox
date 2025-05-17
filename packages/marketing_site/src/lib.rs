#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use hyperchad::{
    app::{App, AppBuilder, renderer::DefaultRenderer},
    color::Color,
    router::Router,
};
use moosicbox_env_utils::option_env_f32;
use serde_json::json;

mod download;

static BACKGROUND_COLOR: LazyLock<Color> = LazyLock::new(|| Color::from_hex("#181a1b"));

pub static VIEWPORT: LazyLock<String> = LazyLock::new(|| "width=device-width".to_string());

#[cfg(feature = "assets")]
static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
    LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

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
        .with_static_route(&["/try-now"], |_| async {
            moosicbox_marketing_site_ui::try_now()
        })
        .with_route_result(&["/releases"], |req| async {
            download::releases_route(req).await
        })
        .with_route(&["/health"], |_| async {
            json!({
                "healthy": true,
                "hash": std::env!("GIT_HASH"),
            })
        })
});

#[cfg(feature = "assets")]
static ASSETS_DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
    CARGO_MANIFEST_DIR.as_ref().map_or_else(
        || <std::path::PathBuf as std::str::FromStr>::from_str("public").unwrap(),
        |dir| dir.join("public"),
    )
});

#[cfg(feature = "assets")]
pub static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> =
    LazyLock::new(|| {
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
            hyperchad::renderer::assets::StaticAssetRoute {
                route: "favicon.ico".to_string(),
                target: ASSETS_DIR.join("favicon.ico").try_into().unwrap(),
            },
            hyperchad::renderer::assets::StaticAssetRoute {
                route: "public".to_string(),
                target: ASSETS_DIR.clone().try_into().unwrap(),
            },
        ]
    });

/// # Panics
///
/// * If an invalid number is given to `WINDOW_WIDTH` or `WINDOW_HEIGHT`
pub fn init() -> AppBuilder {
    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(ROUTER.clone())
        .with_background(*BACKGROUND_COLOR)
        .with_title("MoosicBox".to_string())
        .with_description("MoosicBox: A music app for cows".to_string())
        .with_size(
            option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0),
            option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0),
        );

    #[cfg(feature = "assets")]
    for assets in ASSETS.iter().cloned() {
        app.static_asset_route_result(assets).unwrap();
    }

    app
}

/// # Errors
///
/// * If the `NativeApp` fails to start
pub fn build_app(builder: AppBuilder) -> Result<App<DefaultRenderer>, hyperchad::app::Error> {
    use hyperchad::renderer::Renderer as _;

    #[allow(unused_mut)]
    let mut app = builder.build_default()?;

    app.renderer.add_responsive_trigger(
        "mobile".into(),
        hyperchad::renderer::transformer::ResponsiveTrigger::MaxWidth(
            hyperchad::renderer::transformer::Number::Integer(600),
        ),
    );
    app.renderer.add_responsive_trigger(
        "mobile-large".into(),
        hyperchad::renderer::transformer::ResponsiveTrigger::MaxWidth(
            hyperchad::renderer::transformer::Number::Integer(1100),
        ),
    );

    Ok(app)
}
