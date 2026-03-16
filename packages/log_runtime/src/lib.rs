use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRuntimePaths {
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LogRuntimePathsConfig<'a> {
    pub app_name: &'a str,
    pub state_dir_env: &'a str,
    pub log_dir_env: &'a str,
}

pub fn resolve_paths(config: &LogRuntimePathsConfig<'_>) -> LogRuntimePaths {
    let state_dir = resolve_state_dir(config);
    let log_dir = resolve_log_dir(config, &state_dir);
    LogRuntimePaths { state_dir, log_dir }
}

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
        return std::env::var_os("XDG_STATE_HOME").map_or_else(
            || {
                dirs::home_dir().map_or_else(
                    || PathBuf::from(".").join(config.app_name).join("state"),
                    |home| home.join(".local").join("state").join(config.app_name),
                )
            },
            |base| PathBuf::from(base).join(config.app_name),
        );
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
    use super::{LogRuntimePaths, ensure_paths};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LogLevel {
        Error,
        Warn,
        Info,
        Debug,
        Trace,
    }

    impl LogLevel {
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

    #[derive(Debug, thiserror::Error)]
    pub enum InitError {
        #[error("failed ensuring log directories")]
        EnsurePaths(#[source] std::io::Error),
        #[cfg(feature = "file")]
        #[error("failed creating log directory")]
        CreateLogDir(#[source] std::io::Error),
    }

    #[derive(Debug, Clone)]
    pub struct InitConfig<'a> {
        pub paths: &'a LogRuntimePaths,
        pub level: LogLevel,
        pub with_target: bool,
        #[cfg(feature = "file")]
        pub file_prefix: &'a str,
    }

    #[derive(Debug)]
    pub struct LoggingHandle {
        #[cfg(feature = "file")]
        _guard: tracing_appender::non_blocking::WorkerGuard,
    }

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
