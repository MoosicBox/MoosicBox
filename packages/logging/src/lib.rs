#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::fs::create_dir_all;

use free_log_client::FreeLogLayer;
use moosicbox_config::get_config_dir_path;
use moosicbox_env_utils::default_env;
use thiserror::Error;

pub use free_log_client;

#[derive(Debug, Error)]
pub enum InitError {
    #[error(transparent)]
    Logs(#[from] free_log_client::LogsInitError),
    #[error(transparent)]
    BuildLogsConfig(#[from] free_log_client::BuildLogsConfigError),
    #[error(transparent)]
    BuildFileWriterConfig(#[from] free_log_client::BuildFileWriterConfigError),
}

pub fn init(filename: &str) -> Result<FreeLogLayer, InitError> {
    #[cfg(debug_assertions)]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=trace";
    #[cfg(not(debug_assertions))]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=info";

    let mut logs_config = free_log_client::LogsConfig::builder();

    if let Some(log_dir) = get_config_dir_path().map(|p| p.join("logs")) {
        if create_dir_all(&log_dir).is_ok() {
            logs_config = logs_config.with_file_writer(
                free_log_client::FileWriterConfig::builder()
                    .file_path(log_dir.join(filename))
                    .log_level(free_log_client::Level::Debug),
            )?;
        } else {
            log::warn!("Could not create directory path for logs files at {log_dir:?}");
        }
    } else {
        log::warn!("Could not get config dir to put the logs into");
    }

    let layer = free_log_client::init(logs_config.env_filter(default_env!(
        "MOOSICBOX_LOG",
        default_env!("RUST_LOG", DEFAULT_LOG_LEVEL)
    )))?;

    Ok(layer)
}
