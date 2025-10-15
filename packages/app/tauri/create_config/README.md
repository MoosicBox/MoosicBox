# MoosicBox Tauri Config Generator

Configuration file generator for MoosicBox Tauri applications.

## Overview

The MoosicBox Tauri Config Generator provides:

- **Build Configuration**: Generate TypeScript configuration for Tauri apps
- **Feature Flags**: Configure web, app, and bundled features
- **TypeScript Export**: Export configuration as TypeScript constants
- **CLI Tool**: Command-line binary for configuration generation

## Features

### Configuration Generation

- **TypeScript Output**: Generate TypeScript configuration files
- **Feature Flags**: Configure app capabilities (web, app, bundled)
- **JSON Serialization**: Structured configuration output
- **File Management**: Automatic file creation and truncation

### Build Integration

- **Build-time Generation**: Generate config during build process
- **Path Flexibility**: Configurable output file paths
- **Atomic Writes**: Safe file writing with truncation

### Command-Line Interface

- **Binary Tool**: `moosicbox_create_config` CLI binary
- **Flexible Arguments**: Configure bundled flag and output path
- **Logging Integration**: Built-in logging support

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_create_config = { path = "../app/tauri/create_config" }
```

## Usage

### Basic Configuration Generation

```rust
use moosicbox_app_create_config::generate;

// Generate config file for bundled app
generate(true, "src/config.ts");

// Generate config file for non-bundled app
generate(false, "src/config.ts");
```

**Note**: The `generate` function will panic if the file fails to open or write.

### Manual Configuration

```rust
use moosicbox_app_create_config::Config;

let config = Config {
    web: false,
    app: true,
    bundled: true,
};

let typescript_output = config.to_json();
println!("{}", typescript_output);
// Output: export const config = {"web":false,"app":true,"bundled":true} as const;
```

**Note**: The `to_json` method will panic if serialization fails.

### Command-Line Usage

```bash
# Generate config for bundled app
moosicbox_create_config --bundled --output src/config.ts

# Generate config for non-bundled app
moosicbox_create_config --output src/config.ts
```

## Dependencies

- **serde**: Serialization framework
- **serde_json**: JSON serialization
- **switchy_fs**: Cross-platform file system operations
- **clap**: Command-line argument parsing (CLI binary)
- **log**: Logging facade
- **moosicbox_logging**: MoosicBox logging utilities
- **moosicbox_assert**: MoosicBox assertion utilities
