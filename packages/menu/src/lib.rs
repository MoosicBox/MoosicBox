#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

pub mod library;
pub use moosicbox_menu_models as models;
