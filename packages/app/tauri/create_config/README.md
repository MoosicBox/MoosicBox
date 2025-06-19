# MoosicBox Tauri Config Generator

Configuration file generator for MoosicBox Tauri applications.

## Overview

The MoosicBox Tauri Config Generator provides:

- **Build Configuration**: Generate TypeScript configuration for Tauri apps
- **Feature Flags**: Configure web, app, and bundled features
- **JSON Export**: Export configuration as TypeScript constants
- **Build Integration**: Integrate with Tauri build process

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

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_tauri_create_config = { path = "../app/tauri/create_config" }
```

## Usage

### Basic Configuration Generation

```rust
use moosicbox_app_tauri_create_config::generate;

// Generate config file for bundled app
generate(true, "src/config.ts");

// Generate config file for non-bundled app
generate(false, "src/config.ts");
```

### Manual Configuration

```rust
use moosicbox_app_tauri_create_config::Config;

let config = Config {
    web: false,
    app: true,
    bundled: true,
};

let typescript_output = config.to_json();
println!("{}", typescript_output);
// Output: export const config = {"web":false,"app":true,"bundled":true} as const;
```

## Dependencies

- **Serde**: JSON serialization
- **Switchy FS**: File system operations
