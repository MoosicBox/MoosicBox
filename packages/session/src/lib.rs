#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

pub mod db;
pub mod models;

#[cfg(feature = "api")]
pub mod api;
