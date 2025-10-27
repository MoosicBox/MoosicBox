//! A template-based UI framework for building cross-platform applications.
//!
//! `HyperChad` enables developers to write UI templates once and deploy across desktop
//! (Egui, FLTK), web (HTML, Vanilla JS), and server-side (Actix, Lambda) environments.
//!
//! # Features
//!
//! * **Multi-Renderer Architecture**: Support for Egui, FLTK, HTML, Vanilla JS, and server-side rendering
//! * **Template-Based UI**: Build interfaces using the `container!` macro system
//! * **Routing System**: Async router with navigation support
//! * **Action System**: Event handling and data flow management
//! * **State Persistence**: Key-value state store with optional `SQLite` persistence
//! * **Color Management**: Consistent theming across all renderers
//!
//! # Examples
//!
//! ```rust,no_run
//! use hyperchad::app::{App, AppBuilder};
//! use hyperchad::router::{Router, RoutePath, RouteRequest};
//! use hyperchad::template::container;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let router = Router::new()
//!     .with_route(RoutePath::Literal("/".to_string()), |_req: RouteRequest| async move {
//!         let content = container! {
//!             div {
//!                 h1 { "Welcome to HyperChad" }
//!                 button { "Click Me" }
//!             }
//!         };
//!         content
//!     });
//!
//! let app = AppBuilder::new()
//!     .with_title("My App".to_string())
//!     .with_router(router)
//!     .build_default()?;
//!
//! app.run()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Modules
//!
//! The crate re-exports several sub-crates as modules:
//!
//! * [`actions`] - Event handling and action system
//! * [`app`] - Application builder and runtime
//! * [`color`] - Color management
//! * [`router`] - Routing functionality
//! * [`state`] - State persistence system
//! * [`template`] - Template macro system
//! * [`transformer`] - Container and element types
//! * [`transformer_models`] - Data models for transformers

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "actions")]
pub use hyperchad_actions as actions;
#[cfg(feature = "app")]
pub use hyperchad_app as app;
#[cfg(feature = "color")]
pub use hyperchad_color as color;
#[cfg(feature = "js-bundler")]
pub use hyperchad_js_bundler as js_bundler;
#[cfg(feature = "markdown")]
pub use hyperchad_markdown as markdown;
#[cfg(feature = "renderer")]
pub use hyperchad_renderer as renderer;
#[cfg(feature = "renderer-egui")]
pub use hyperchad_renderer_egui as renderer_egui;
#[cfg(feature = "renderer-fltk")]
pub use hyperchad_renderer_fltk as renderer_fltk;
#[cfg(feature = "renderer-html")]
pub use hyperchad_renderer_html as renderer_html;
#[cfg(feature = "renderer-html-actix")]
pub use hyperchad_renderer_html_actix as renderer_html_actix;
#[cfg(feature = "renderer-html-cdn")]
pub use hyperchad_renderer_html_cdn as renderer_html_cdn;
#[cfg(feature = "renderer-html-http")]
pub use hyperchad_renderer_html_http as renderer_html_http;
#[cfg(feature = "renderer-html-lambda")]
pub use hyperchad_renderer_html_lambda as renderer_html_lambda;
#[cfg(feature = "renderer-html-web-server")]
pub use hyperchad_renderer_html_web_server as renderer_html_web_server;
#[cfg(feature = "renderer-vanilla-js")]
pub use hyperchad_renderer_vanilla_js as renderer_vanilla_js;
#[cfg(feature = "router")]
pub use hyperchad_router as router;
#[cfg(feature = "state")]
pub use hyperchad_state as state;
#[cfg(feature = "template")]
pub use hyperchad_template as template;
#[cfg(feature = "transformer")]
pub use hyperchad_transformer as transformer;
#[cfg(feature = "transformer-models")]
pub use hyperchad_transformer_models as transformer_models;

// Simulation modules
#[cfg(feature = "simulator")]
pub use hyperchad_simulator as simulator;
#[cfg(feature = "test-utils")]
pub use hyperchad_test_utils as test_utils;
