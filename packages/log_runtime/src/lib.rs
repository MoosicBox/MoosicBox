//! Runtime helpers for resolving and initializing logging paths.

use std::path::PathBuf;

/// Resolved filesystem locations used by the logging runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRuntimePaths {
    /// Directory where runtime state is stored.
    pub state_dir: PathBuf,
    /// Directory where log files are written.
    pub log_dir: PathBuf,
}

/// Configuration for discovering runtime state and log directories.
#[derive(Debug, Clone)]
pub struct LogRuntimePathsConfig<'a> {
    /// Application name used in default platform-specific directory layouts.
    pub app_name: &'a str,
    /// Environment variable that overrides the default state directory.
    pub state_dir_env: &'a str,
    /// Environment variable that overrides the default log directory.
    pub log_dir_env: &'a str,
}

/// Resolves platform-appropriate state and log directories.
///
/// Environment variable overrides from [`LogRuntimePathsConfig`] are applied
/// before platform defaults.
///
/// # Examples
///
/// ```
/// use moosicbox_log_runtime::{LogRuntimePathsConfig, resolve_paths};
///
/// let config = LogRuntimePathsConfig {
///     app_name: "MoosicBox",
///     state_dir_env: "MOOSICBOX_STATE_DIR",
///     log_dir_env: "MOOSICBOX_LOG_DIR",
/// };
///
/// let paths = resolve_paths(&config);
/// assert!(!paths.state_dir.as_os_str().is_empty());
/// assert!(!paths.log_dir.as_os_str().is_empty());
/// ```
#[must_use]
pub fn resolve_paths(config: &LogRuntimePathsConfig<'_>) -> LogRuntimePaths {
    let state_dir = resolve_state_dir(config);
    let log_dir = resolve_log_dir(config, &state_dir);
    LogRuntimePaths { state_dir, log_dir }
}

/// Creates the state and log directories if they do not exist.
///
/// # Errors
///
/// Returns an error when directory creation fails:
/// * Creating `paths.state_dir` fails.
/// * Creating `paths.log_dir` fails.
pub fn ensure_paths(paths: &LogRuntimePaths) -> std::io::Result<()> {
    std::fs::create_dir_all(&paths.state_dir)?;
    std::fs::create_dir_all(&paths.log_dir)?;
    Ok(())
}

fn resolve_state_dir(config: &LogRuntimePathsConfig<'_>) -> PathBuf {
    if let Some(path) = std::env::var_os(config.state_dir_env) {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map_or_else(
            || PathBuf::from(".").join(config.app_name).join("state"),
            |home| {
                home.join("Library")
                    .join("Application Support")
                    .join(config.app_name)
                    .join("State")
            },
        )
    }

    #[cfg(target_os = "windows")]
    {
        return dirs::data_local_dir().map_or_else(
            || PathBuf::from(".").join(config.app_name).join("state"),
            |base| base.join(config.app_name).join("State"),
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_STATE_HOME").map_or_else(
            || {
                dirs::home_dir().map_or_else(
                    || PathBuf::from(".").join(config.app_name).join("state"),
                    |home| home.join(".local").join("state").join(config.app_name),
                )
            },
            |base| PathBuf::from(base).join(config.app_name),
        )
    }
}

fn resolve_log_dir(config: &LogRuntimePathsConfig<'_>, _state_dir: &std::path::Path) -> PathBuf {
    if let Some(path) = std::env::var_os(config.log_dir_env) {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map_or_else(
            || PathBuf::from(".").join(config.app_name).join("logs"),
            |home| home.join("Library").join("Logs").join(config.app_name),
        )
    }

    #[cfg(target_os = "windows")]
    {
        return dirs::data_local_dir().map_or_else(
            || PathBuf::from(".").join(config.app_name).join("logs"),
            |base| base.join(config.app_name).join("Logs"),
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        _state_dir.join("logs")
    }
}

#[cfg(feature = "init")]
pub mod init {
    //! Logging initialization helpers.

    use super::{LogRuntimePaths, ensure_paths};

    /// Log verbosity level used by runtime initialization.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LogLevel {
        /// Error-level logging only.
        Error,
        /// Warning and error logging.
        Warn,
        /// Informational, warning, and error logging.
        Info,
        /// Debug and above logging.
        Debug,
        /// Trace and above logging.
        Trace,
    }

    impl LogLevel {
        /// Converts this level into the corresponding tracing level.
        #[must_use]
        pub const fn as_tracing_level(self) -> tracing::Level {
            match self {
                Self::Error => tracing::Level::ERROR,
                Self::Warn => tracing::Level::WARN,
                Self::Info => tracing::Level::INFO,
                Self::Debug => tracing::Level::DEBUG,
                Self::Trace => tracing::Level::TRACE,
            }
        }
    }

    /// Errors that can occur while initializing runtime logging.
    #[derive(Debug, thiserror::Error)]
    pub enum InitError {
        /// Creating one of the required runtime directories failed.
        #[error("failed ensuring log directories")]
        EnsurePaths(#[source] std::io::Error),
        #[cfg(feature = "file")]
        /// Creating the configured log output directory failed.
        #[error("failed creating log directory")]
        CreateLogDir(#[source] std::io::Error),
    }

    /// Configuration for setting up tracing subscribers.
    #[derive(Debug, Clone)]
    pub struct InitConfig<'a> {
        /// Pre-resolved runtime paths.
        pub paths: &'a LogRuntimePaths,
        /// Maximum log level to emit.
        pub level: LogLevel,
        /// Whether the event target should be included in output.
        pub with_target: bool,
        #[cfg(feature = "file")]
        /// File name prefix used for daily log file rotation.
        pub file_prefix: &'a str,
    }

    /// Handle that keeps logging resources alive for process lifetime.
    #[derive(Debug)]
    pub struct LoggingHandle {
        #[cfg(feature = "file")]
        _guard: tracing_appender::non_blocking::WorkerGuard,
    }

    /// Initializes tracing output for runtime logging.
    ///
    /// # Errors
    ///
    /// Returns [`InitError`] when initialization cannot complete:
    /// * [`InitError::EnsurePaths`] if required runtime directories cannot be created.
    /// * [`InitError::CreateLogDir`] if file logging is enabled and the log directory cannot be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use moosicbox_log_runtime::{LogRuntimePathsConfig, resolve_paths};
    /// use moosicbox_log_runtime::init::{InitConfig, LogLevel, init};
    ///
    /// let paths = resolve_paths(&LogRuntimePathsConfig {
    ///     app_name: "MoosicBox",
    ///     state_dir_env: "MOOSICBOX_STATE_DIR",
    ///     log_dir_env: "MOOSICBOX_LOG_DIR",
    /// });
    ///
    /// let _handle = init(InitConfig {
    ///     paths: &paths,
    ///     level: LogLevel::Info,
    ///     with_target: true,
    ///     #[cfg(feature = "file")]
    ///     file_prefix: "moosicbox",
    /// })?;
    /// # Ok::<(), moosicbox_log_runtime::init::InitError>(())
    /// ```
    pub fn init(config: InitConfig<'_>) -> Result<LoggingHandle, InitError> {
        ensure_paths(config.paths).map_err(InitError::EnsurePaths)?;

        let builder = tracing_subscriber::fmt()
            .with_max_level(config.level.as_tracing_level())
            .with_target(config.with_target)
            .with_ansi(false);

        #[cfg(feature = "file")]
        {
            std::fs::create_dir_all(&config.paths.log_dir).map_err(InitError::CreateLogDir)?;
            let file_appender =
                tracing_appender::rolling::daily(&config.paths.log_dir, config.file_prefix);
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            let _ = builder.with_writer(non_blocking).try_init();
            return Ok(LoggingHandle { _guard: guard });
        }

        #[cfg(not(feature = "file"))]
        {
            let _ = builder.try_init();
            Ok(LoggingHandle {})
        }
    }
}
