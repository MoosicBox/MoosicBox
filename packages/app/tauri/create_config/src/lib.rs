//! Configuration file generation for the `MoosicBox` Tauri application.
//!
//! This crate provides utilities to generate TypeScript configuration files
//! that define build-time settings for the `MoosicBox` app, such as whether
//! web or app interfaces are enabled and whether the build is bundled.
//!
//! # Examples
//!
//! ```rust,no_run
//! use moosicbox_app_create_config::generate;
//!
//! // Generate a configuration file for a bundled app build
//! generate(true, "src/config.ts");
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{io::Write as _, path::Path};

use serde::Serialize;

/// Configuration for the `MoosicBox` app build.
#[derive(Serialize)]
pub struct Config {
    /// Whether the web interface is enabled.
    pub web: bool,
    /// Whether the app interface is enabled.
    pub app: bool,
    /// Whether the build is bundled.
    pub bundled: bool,
}

impl Config {
    /// Converts the configuration to a JSON string formatted for TypeScript consumption.
    ///
    /// # Panics
    ///
    /// * If the `Config` fails to serialize
    #[must_use]
    pub fn to_json(&self) -> String {
        let json = serde_json::to_string(self).unwrap();

        format!("export const config = {json} as const;",)
    }
}

/// Generates a configuration file for the `MoosicBox` app.
///
/// Creates a TypeScript configuration file with web disabled, app enabled,
/// and the bundled flag set according to the `bundled` parameter.
///
/// # Panics
///
/// * If the file fails to open
/// * If writing to the file fails
pub fn generate<P: AsRef<Path>>(bundled: bool, output: P) {
    let config = Config {
        web: false,
        app: true,
        bundled,
    };

    let mut file = switchy_fs::sync::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)
        .unwrap();

    file.write_all(config.to_json().as_bytes()).unwrap();
}
