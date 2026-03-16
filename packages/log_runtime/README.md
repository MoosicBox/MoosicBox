# moosicbox_log_runtime

Generic log runtime paths and initialization.

## Description

`moosicbox_log_runtime` provides:

- OS-aware state and log directory resolution with environment variable overrides
- directory creation helpers for resolved paths
- optional tracing initialization (console or rolling file output)

## Features

- `init`: enables `init` module and tracing subscriber initialization APIs
- `file`: enables daily rolling file output (`tracing-appender`); implies `init`

## Installation

```toml
[dependencies]
moosicbox_log_runtime = "0.1.0"
```

With optional features:

```toml
[dependencies]
moosicbox_log_runtime = { version = "0.1.0", features = ["init", "file"] }
```

## Usage

Resolve and create runtime directories:

```rust
use moosicbox_log_runtime::{ensure_paths, resolve_paths, LogRuntimePathsConfig};

let paths = resolve_paths(&LogRuntimePathsConfig {
    app_name: "my_app",
    state_dir_env: "MY_APP_STATE_DIR",
    log_dir_env: "MY_APP_LOG_DIR",
});

ensure_paths(&paths)?;
```

Initialize tracing (requires `init`):

```rust
use moosicbox_log_runtime::init::{init, InitConfig, LogLevel};
use moosicbox_log_runtime::{resolve_paths, LogRuntimePathsConfig};

let paths = resolve_paths(&LogRuntimePathsConfig {
    app_name: "my_app",
    state_dir_env: "MY_APP_STATE_DIR",
    log_dir_env: "MY_APP_LOG_DIR",
});

let _handle = init(InitConfig {
    paths: &paths,
    level: LogLevel::Info,
    with_target: false,
    #[cfg(feature = "file")]
    file_prefix: "my_app",
})?;
```

Core public API:

- `LogRuntimePaths` (`state_dir`, `log_dir`)
- `LogRuntimePathsConfig` (`app_name`, `state_dir_env`, `log_dir_env`)
- `resolve_paths(&LogRuntimePathsConfig) -> LogRuntimePaths`
- `ensure_paths(&LogRuntimePaths) -> std::io::Result<()>`
- `init` feature module: `LogLevel`, `InitConfig`, `InitError`, `LoggingHandle`, `init(...)`

## License

MPL-2.0
