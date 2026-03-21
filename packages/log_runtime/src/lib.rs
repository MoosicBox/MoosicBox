#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

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
    let log_dir = resolve_log_dir(
        config,
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        &state_dir,
    );
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

fn resolve_log_dir(
    config: &LogRuntimePathsConfig<'_>,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] state_dir: &std::path::Path,
) -> PathBuf {
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
        state_dir.join("logs")
    }
}

#[cfg(feature = "layer-ext")]
pub type DynLayer =
    Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static>;

#[cfg(feature = "init")]
pub mod init {
    //! Logging initialization helpers.

    use super::{LogRuntimePaths, ensure_paths};
    use std::io::IsTerminal;
    use tracing_subscriber::fmt::writer::BoxMakeWriter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum SourceMode {
        LogOnly,
        TracingOnly,
        #[default]
        Both,
    }

    #[derive(Debug, Clone)]
    pub enum FileMode<'a> {
        Exact(&'a str),
        RollingDaily(&'a str),
    }

    #[derive(Debug, Clone)]
    pub struct FileSinkConfig<'a> {
        pub mode: FileMode<'a>,
    }

    #[derive(Debug, Clone)]
    pub struct SinkConfig<'a> {
        pub stderr: bool,
        pub file: Option<FileSinkConfig<'a>>,
    }

    impl Default for SinkConfig<'_> {
        fn default() -> Self {
            Self {
                stderr: true,
                file: None,
            }
        }
    }

    pub struct InitConfig<'a> {
        pub paths: &'a LogRuntimePaths,
        pub source_mode: SourceMode,
        pub env_filter: Option<String>,
        pub default_env_filter: Option<String>,
        pub use_moosicbox_log_env: bool,
        pub use_rust_log_env: bool,
        pub with_target: bool,
        pub sinks: SinkConfig<'a>,
        #[cfg(feature = "layer-ext")]
        pub extra_layers: Vec<crate::DynLayer>,
    }

    impl<'a> InitConfig<'a> {
        #[must_use]
        pub fn new(paths: &'a LogRuntimePaths) -> Self {
            Self {
                paths,
                source_mode: SourceMode::default(),
                env_filter: None,
                default_env_filter: None,
                use_moosicbox_log_env: true,
                use_rust_log_env: true,
                with_target: false,
                sinks: SinkConfig::default(),
                #[cfg(feature = "layer-ext")]
                extra_layers: Vec::new(),
            }
        }
    }

    /// Errors that can occur while initializing runtime logging.
    #[derive(Debug, thiserror::Error)]
    pub enum InitError {
        /// Creating one of the required runtime directories failed.
        #[error("failed ensuring log directories")]
        EnsurePaths(#[source] std::io::Error),
        /// Creating the configured log file directory failed.
        #[error("failed creating log file directory")]
        CreateLogDir(#[source] std::io::Error),
        /// Opening or creating the configured exact log file failed.
        #[error("failed creating log file")]
        CreateLogFile(#[source] std::io::Error),
        /// Initializing the `log` compatibility bridge failed.
        #[error("failed to initialize log compatibility bridge")]
        LogBridge(#[source] tracing_log::log_tracer::SetLoggerError),
        /// Initializing the global tracing subscriber failed.
        #[error("failed to initialize global tracing subscriber")]
        Subscriber(#[source] tracing_subscriber::util::TryInitError),
    }

    /// Handle that keeps logging resources alive for process lifetime.
    #[derive(Debug)]
    pub struct LoggingHandle {
        _guards: Vec<tracing_appender::non_blocking::WorkerGuard>,
    }

    /// Initializes the global tracing subscriber.
    ///
    /// # Errors
    ///
    /// Returns [`InitError`] when initialization cannot complete:
    /// * [`InitError::EnsurePaths`] if required runtime directories cannot be created.
    /// * [`InitError::CreateLogDir`] if the configured file sink directory cannot be created.
    /// * [`InitError::CreateLogFile`] if an exact file sink cannot open its output file.
    /// * [`InitError::LogBridge`] if log compatibility mode is enabled and bridge setup fails.
    /// * [`InitError::Subscriber`] if the global tracing subscriber has already been initialized or cannot be installed.
    pub fn init(config: InitConfig<'_>) -> Result<LoggingHandle, InitError> {
        ensure_paths(config.paths).map_err(InitError::EnsurePaths)?;
        let filter = resolve_filter(&config);

        let mut guards = Vec::new();
        let mut layers: Vec<
            Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
        > = Vec::new();

        if config.sinks.stderr {
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(std::io::stderr().is_terminal())
                .with_target(config.with_target)
                .with_writer(BoxMakeWriter::new(std::io::stderr));
            layers.push(Box::new(layer));
        }

        if let Some(file_sink) = &config.sinks.file {
            std::fs::create_dir_all(&config.paths.log_dir).map_err(InitError::CreateLogDir)?;
            let (writer, guard) = match &file_sink.mode {
                FileMode::Exact(name) => {
                    let file_path = config.paths.log_dir.join(name);
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file_path)
                        .map_err(InitError::CreateLogFile)?;
                    tracing_appender::non_blocking(file)
                }
                FileMode::RollingDaily(prefix) => {
                    let file_appender =
                        tracing_appender::rolling::daily(&config.paths.log_dir, prefix);
                    tracing_appender::non_blocking(file_appender)
                }
            };
            guards.push(guard);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(config.with_target)
                .with_writer(BoxMakeWriter::new(writer));
            layers.push(Box::new(layer));
        }

        #[cfg(feature = "layer-ext")]
        {
            layers.extend(config.extra_layers);
        }

        let subscriber = tracing_subscriber::registry().with(layers).with(filter);
        subscriber.try_init().map_err(InitError::Subscriber)?;
        Ok(LoggingHandle { _guards: guards })
    }

    fn resolve_filter(config: &InitConfig<'_>) -> tracing_subscriber::EnvFilter {
        let directive = config
            .env_filter
            .clone()
            .or_else(|| {
                if config.use_moosicbox_log_env {
                    std::env::var("MOOSICBOX_LOG").ok()
                } else {
                    None
                }
            })
            .or_else(|| {
                if config.use_rust_log_env {
                    std::env::var("RUST_LOG").ok()
                } else {
                    None
                }
            })
            .or_else(|| config.default_env_filter.clone())
            .unwrap_or_else(default_env_filter);

        tracing_subscriber::EnvFilter::new(directive)
    }

    fn default_env_filter() -> String {
        #[cfg(debug_assertions)]
        {
            "moosicbox=trace".to_string()
        }
        #[cfg(not(debug_assertions))]
        {
            "moosicbox=info".to_string()
        }
    }
}
