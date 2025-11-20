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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_init_succeeds_or_already_initialized() {
        // Test that init either succeeds or fails with SetLogger error
        // (which indicates the logger was already set by another test)
        let result = init(None, None);

        // Init should either succeed, or fail with SetLogger error if already initialized
        match result {
            Ok(_) | Err(InitError::Logs(free_log_client::LogsInitError::SetLogger(_))) => {
                // Success case - logger was initialized
                // OR expected failure - logger already initialized by another test
                // This is acceptable in a test environment where tests may run in parallel
            }
            Err(e) => panic!("Unexpected error type: {e:?}"),
        }
    }

    #[test_log::test]
    fn test_init_configurations_are_valid() {
        // Test that different configuration combinations don't panic
        // We can't test actual initialization due to global logger state,
        // but we can verify the configurations are constructed correctly

        // Configuration 1: No filename, no layers
        let config1 = || {
            let _logs_config = free_log_client::LogsConfig::builder();
        };
        config1();

        // Configuration 2: With filename (requires config dir)
        let config2 = || {
            let logs_config = free_log_client::LogsConfig::builder();
            if let Some(log_dir) = make_config_dir_path().map(|p| p.join("logs")) {
                let _ = logs_config.with_file_writer(
                    free_log_client::FileWriterConfig::builder()
                        .file_path(log_dir.join("test.log"))
                        .log_level(free_log_client::Level::Debug),
                );
            }
        };
        config2();

        // Configuration 3: With empty layers
        let config3 = || {
            let _logs_config = free_log_client::LogsConfig::builder().with_layers(vec![]);
        };
        config3();
    }

    #[test_log::test]
    fn test_init_error_is_error() {
        // Test that InitError implements the Error trait properly
        use std::error::Error;

        // The type should implement Error and Debug
        fn assert_error<T: Error + std::fmt::Debug>() {}
        assert_error::<InitError>();
    }

    #[test_log::test]
    fn test_init_error_from_conversions() {
        // Test that From trait implementations work correctly for InitError
        use std::error::Error;

        // We can't construct the actual error types from free_log_client,
        // but we can verify the type relationships exist
        fn assert_from<T, E: Error>()
        where
            T: From<E>,
        {
        }

        assert_from::<InitError, free_log_client::LogsInitError>();
        assert_from::<InitError, free_log_client::BuildLogsConfigError>();
        assert_from::<InitError, free_log_client::BuildFileWriterConfigError>();
    }
}
