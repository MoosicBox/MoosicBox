//! Free log integration for initializing and configuring structured logging.
//!
//! This module provides functionality to initialize the `free_log_client` logging
//! system with environment-based configuration, optional file output, and custom
//! tracing layers.

#![allow(clippy::module_name_repetitions)]

use free_log_client::{DynLayer, FreeLogLayer};
use moosicbox_config::make_config_dir_path;
use moosicbox_env_utils::default_env;
use thiserror::Error;

/// Re-export of the `free_log_client` crate for convenient access to its types and traits.
///
/// This allows users of `moosicbox_logging` to access `free_log_client` functionality
/// (such as [`DynLayer`](free_log_client::DynLayer) and
/// [`FreeLogLayer`](free_log_client::FreeLogLayer)) without adding a separate dependency.
pub use free_log_client;

/// Error type for logging initialization failures.
#[derive(Debug, Error)]
pub enum InitError {
    /// Failed to initialize the logging system.
    #[error(transparent)]
    Logs(#[from] free_log_client::LogsInitError),
    /// Failed to build the logs configuration.
    #[error(transparent)]
    BuildLogsConfig(#[from] free_log_client::BuildLogsConfigError),
    /// Failed to build the file writer configuration.
    #[error(transparent)]
    BuildFileWriterConfig(#[from] free_log_client::BuildFileWriterConfigError),
}

/// Initializes the logging system with optional file output and custom layers.
///
/// Configures environment-based log filtering using `MOOSICBOX_LOG` or `RUST_LOG`
/// environment variables, with default log levels of `trace` in debug builds and
/// `info` in release builds. When a filename is provided, logs are written to
/// `{config_dir}/logs/{filename}`.
///
/// # Parameters
///
/// * `filename` - Optional log file name to write logs to in the config directory's logs subdirectory
/// * `layers` - Optional vector of custom tracing layers to add to the logging system
///
/// # Returns
///
/// Returns a `FreeLogLayer` that can be used to manage the logging subscription.
///
/// # Errors
///
/// * `InitError::Logs` - Failed to initialize the logging system
/// * `InitError::BuildLogsConfig` - Failed to build the logs configuration
/// * `InitError::BuildFileWriterConfig` - Failed to build the file writer configuration
#[must_use = "the returned FreeLogLayer must be kept alive for logging to work"]
pub fn init(
    filename: Option<&str>,
    layers: Option<Vec<DynLayer>>,
) -> Result<FreeLogLayer, InitError> {
    #[cfg(debug_assertions)]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=trace";
    #[cfg(not(debug_assertions))]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=info";

    let mut logs_config = free_log_client::LogsConfig::builder();

    if let Some(layers) = layers {
        logs_config = logs_config.with_layers(layers);
    }

    if let Some(filename) = filename {
        if let Some(log_dir) = make_config_dir_path().map(|p| p.join("logs")) {
            logs_config = logs_config.with_file_writer(
                free_log_client::FileWriterConfig::builder()
                    .file_path(log_dir.join(filename))
                    .log_level(free_log_client::Level::Debug),
            )?;
        } else {
            log::warn!("Could not get config dir to put the logs into");
        }
    }

    let layer = free_log_client::init(logs_config.env_filter(default_env!(
        "MOOSICBOX_LOG",
        default_env!("RUST_LOG", DEFAULT_LOG_LEVEL)
    )))?;

    Ok(layer)
}
