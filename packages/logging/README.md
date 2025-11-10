# `MoosicBox` Logging

Logging utilities with feature-gated modules for `MoosicBox` applications.

## Overview

The `MoosicBox` Logging package provides:

- **Free Log Integration**: Initialize and configure `free_log_client` for structured logging
- **Logging Macros**: Conditional logging macros (e.g., `debug_or_trace!`)
- **API Support**: Optional API feature for `free_log_client`
- **Feature-Gated Modules**: Enable only the logging components you need

## Current Implementation

### Core Components

- **Free Log Module**: Provides `init()` function to configure `free_log_client` with file writing and custom layers
- **Macro Module**: Provides `debug_or_trace!` macro for conditional logging based on log level
- **Re-exports**: Exposes `free_log_client` and `log` crates for convenience

### Available Features

- **`api`**: Enables API features in `free_log_client` (enabled by default)
- **`free_log`**: Enables `free_log` integration module with init function (enabled by default)
- **`macros`**: Enables logging macro utilities (enabled by default)

## Installation

### Cargo Dependencies

```toml
[dependencies]
# With default features (api, free_log, macros)
moosicbox_logging = { path = "../logging" }

# Disable default features and enable specific ones
moosicbox_logging = {
    path = "../logging",
    default-features = false,
    features = ["free_log"]
}
```

## Usage

### Initializing Free Log

```rust
# #[cfg(feature = "free_log")]
# {
use moosicbox_logging::{init, InitError};

fn setup_logging() -> Result<(), InitError> {
    // Initialize with a log file
    let _layer = init(Some("app.log"), None)?;

    // Or initialize without a file
    let _layer = init(None, None)?;

    // Or initialize with custom layers
    # #[cfg(feature = "api")]
    # {
    use moosicbox_logging::free_log_client::DynLayer;
    let custom_layers: Vec<DynLayer> = vec![/* your layers */];
    let _layer = init(Some("app.log"), Some(custom_layers))?;
    # }

    Ok(())
}
# }
```

The `init` function:

- Configures environment-based log filtering (`MOOSICBOX_LOG` or `RUST_LOG` environment variables)
- Sets default log level to `trace` in debug builds, `info` in release builds
- Optionally writes logs to a file in the config directory's `logs` subdirectory
- Supports custom tracing layers

### Using Logging Macros

```rust
# #[cfg(feature = "macros")]
# {
use moosicbox_logging::log;
# #[cfg(feature = "macros")]
use moosicbox_logging::debug_or_trace;

fn example() {
    // Standard log macros (re-exported from `log` crate)
    log::info!("Application started");
    log::debug!("Debug information");

    # #[cfg(feature = "macros")]
    # {
    // Conditional macro: logs at trace level if enabled, otherwise debug
    debug_or_trace!(
        ("Short debug message"),
        ("Detailed trace message with extra context")
    );
    # }
}
# }
```

## Implementation Notes

- All features (`api`, `free_log`, `macros`) are enabled by default
- Without any features enabled, the package provides no functionality (empty lib)
- The `free_log` feature requires `moosicbox_config` and `moosicbox_env_utils` dependencies
- The `api` feature enables API functionality in the underlying `free_log_client`
- Log files are written to `{config_dir}/logs/{filename}` when a filename is provided
- Features can be selectively disabled if not needed

## Features

- **Default**: Includes `api`, `free_log`, and `macros` features
- **`api`**: Enables API support in `free_log_client`
- **`free_log`**: Enables `free_log` integration module with `init()` function
- **`macros`**: Enables logging macro utilities (`debug_or_trace!` macro)

## API Reference

### Free Log Module (feature = "`free_log`")

#### `init` Function

```rust,no_run
# #[cfg(all(feature = "free_log", feature = "api"))]
# {
use moosicbox_logging::free_log_client::{DynLayer, FreeLogLayer};
use moosicbox_logging::InitError;

pub fn init(
    filename: Option<&str>,
    layers: Option<Vec<DynLayer>>,
) -> Result<FreeLogLayer, InitError>
# {
#     todo!()
# }
# }
```

Initializes the logging system with optional file output and custom layers.

**Parameters:**

- `filename`: Optional log file name (written to `{config_dir}/logs/{filename}`)
- `layers`: Optional vector of custom tracing layers

**Returns:** `Result<FreeLogLayer, InitError>`

**Errors:**

- `InitError::Logs`: Failed to initialize logs
- `InitError::BuildLogsConfig`: Failed to build logs config
- `InitError::BuildFileWriterConfig`: Failed to build file writer config

#### Re-exports

- `pub use free_log_client;` - Exposes the entire `free_log_client` crate

### Macros Module (feature = "macros")

#### `debug_or_trace!` Macro

```rust,no_run
# #[cfg(feature = "macros")]
# {
# use moosicbox_logging::debug_or_trace;
// Conditionally logs at trace level if enabled, otherwise logs at debug level
debug_or_trace!(
    ("Debug message"),
    ("Trace message")
);
# }
```

Conditionally logs at trace level if enabled, otherwise logs at debug level.

#### Re-exports

- `pub use log;` - Exposes the standard `log` crate

## Dependencies

### Core Dependencies (always included)

- `free_log_client`: Free log client for structured logging
- `log`: Standard Rust logging facade
- `thiserror`: Error handling

### Feature-Specific Dependencies

- `moosicbox_config` (when `free_log` is enabled): Config directory utilities
- `moosicbox_env_utils` (when `free_log` is enabled): Environment variable helpers

## Package Structure

```text
src/
├── lib.rs           # Feature-gated module exports
├── free_log.rs      # Free log initialization (feature = "free_log")
└── macros.rs        # Logging macros (feature = "macros")
```
