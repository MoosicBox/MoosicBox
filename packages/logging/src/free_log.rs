#![allow(clippy::module_name_repetitions)]

use free_log_client::{DynLayer, FreeLogLayer};
use moosicbox_config::make_config_dir_path;
use moosicbox_env_utils::default_env;
use thiserror::Error;

pub use free_log_client;

/// Error type for logging initialization failures.
#[derive(Debug, Error)]
pub enum InitError {
    #[error(transparent)]
    Logs(#[from] free_log_client::LogsInitError),
    #[error(transparent)]
    BuildLogsConfig(#[from] free_log_client::BuildLogsConfigError),
    #[error(transparent)]
    BuildFileWriterConfig(#[from] free_log_client::BuildFileWriterConfigError),
}

/// Initializes the logging system with optional file output and custom layers.
///
/// # Errors
///
/// * If the logs failed to initialize
/// * If failed to build the logs config
/// * If Failed to build the file writer config
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
