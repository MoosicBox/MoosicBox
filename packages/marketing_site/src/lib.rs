//! `MoosicBox` marketing website implementation.
//!
//! This crate provides the web application infrastructure for the `MoosicBox` marketing site,
//! including routing, static asset serving, and application initialization. It uses the
//! hyperchad framework to render HTML pages and handle HTTP requests.
//!
//! # Features
//!
//! * Multiple renderer backends (HTML, FLTK, egui)
//! * Static and dynamic routing
//! * GitHub releases integration for download pages
//! * Responsive design support
//! * Optional AWS Lambda deployment
//!
//! # Usage
//!
//! Initialize and build the application:
//!
//! ```rust,no_run
//! # use moosicbox_marketing_site::{init, build_app};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let builder = init();
//! let app = build_app(builder)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Main Entry Points
//!
//! * [`init`] - Initialize application builder with default configuration
//! * [`build_app`] - Build the final application from a builder
//! * [`ROUTER`] - Pre-configured router with all site routes

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use hyperchad::{
    app::{App, AppBuilder, renderer::DefaultRenderer},
    color::Color,
    router::Router,
};
use serde_json::json;
use switchy_env::var_parse_opt;

mod download;

static BACKGROUND_COLOR: LazyLock<Color> = LazyLock::new(|| Color::from_hex("#181a1b"));

/// Default viewport meta tag content for responsive design.
pub static VIEWPORT: LazyLock<String> = LazyLock::new(|| "width=device-width".to_string());

#[cfg(feature = "assets")]
static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
    LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

/// Application router with all configured routes for the marketing site.
///
/// Includes routes for home, download, try-now, releases, and health check endpoints.
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

/// Static asset routes for the marketing site.
///
/// Includes JavaScript files, favicon, and public directory assets.
///
/// # Panics
///
/// * If the public assets directory path cannot be converted to a string (during initialization)
/// * If asset path conversion to [`AssetPathTarget`] fails (during initialization)
///
/// [`AssetPathTarget`]: hyperchad::renderer::assets::AssetPathTarget
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

/// Initializes the application builder with default configuration.
///
/// # Panics
///
/// * If static asset route registration fails (when `assets` feature is enabled)
#[must_use]
pub fn init() -> AppBuilder {
    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(ROUTER.clone())
        .with_background(*BACKGROUND_COLOR)
        .with_title("MoosicBox".to_string())
        .with_description("MoosicBox: A music app for cows".to_string())
        .with_size(
            var_parse_opt::<f32>("WINDOW_WIDTH")
                .unwrap_or(None)
                .unwrap_or(1000.0),
            var_parse_opt::<f32>("WINDOW_HEIGHT")
                .unwrap_or(None)
                .unwrap_or(600.0),
        );

    #[cfg(feature = "assets")]
    for assets in ASSETS.iter().cloned() {
        app.static_asset_route_result(assets).unwrap();
    }

    app
}

/// Builds the application from the provided builder.
///
/// # Errors
///
/// * If the application fails to build from the provided configuration
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
