#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod api_logger;
pub mod service_info;

#[cfg(feature = "tunnel")]
pub mod tunnel_info;
