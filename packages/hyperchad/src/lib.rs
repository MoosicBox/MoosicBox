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
#[cfg(feature = "renderer-html-http")]
pub use hyperchad_renderer_html_http as renderer_html_http;
#[cfg(feature = "renderer-html-lambda")]
pub use hyperchad_renderer_html_lambda as renderer_html_lambda;
#[cfg(feature = "renderer-vanilla-js")]
pub use hyperchad_renderer_vanilla_js as renderer_vanilla_js;
#[cfg(feature = "router")]
pub use hyperchad_router as router;
#[cfg(feature = "state")]
pub use hyperchad_state as state;
#[cfg(feature = "transformer")]
pub use hyperchad_transformer as transformer;
#[cfg(feature = "transformer-models")]
pub use hyperchad_transformer_models as transformer_models;
