# MoosicBox Logging

Basic logging utilities with feature-gated modules for MoosicBox applications.

## Overview

The MoosicBox Logging package provides:

- **Feature-Gated Modules**: Optional logging implementations
- **Free Log Integration**: Optional free_log module (requires `free_log` feature)
- **Logging Macros**: Optional logging macro utilities (requires `macros` feature)
- **Minimal Core**: Lightweight base with optional extensions

## Current Implementation

### Core Components
- **Feature-Gated Architecture**: Modular logging components behind feature flags
- **Free Log Module**: Integration with free_log system (optional)
- **Macro Module**: Logging macro utilities (optional)

### Available Features
- **`free_log`**: Enables free_log integration module
- **`macros`**: Enables logging macro utilities

## Installation

### Cargo Dependencies

```toml
[dependencies]
moosicbox_logging = { path = "../logging" }

# Enable specific features
moosicbox_logging = {
    path = "../logging",
    features = ["free_log", "macros"]
}
```

## Usage

### With Free Log Feature

```rust
#[cfg(feature = "free_log")]
use moosicbox_logging::*; // Free log functionality

#[cfg(feature = "free_log")]
async fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Use free_log integration
    // (implementation details depend on the free_log module)
    Ok(())
}
```

### With Macros Feature

```rust
#[cfg(feature = "macros")]
use moosicbox_logging::*; // Logging macros

#[cfg(feature = "macros")]
fn use_logging_macros() {
    // Use logging macro utilities
    // (implementation details depend on the macros module)
}
```

### Basic Usage

```rust
// Basic usage without features
// (minimal functionality available)

#[cfg(all(feature = "free_log", feature = "macros"))]
async fn full_logging_setup() -> Result<(), Box<dyn std::error::Error>> {
    // Both free_log and macros available
    // Use complete logging functionality
    Ok(())
}
```

## Implementation Notes

- The package provides minimal core functionality without features
- Logging capabilities are contained within feature-gated modules
- Free log integration requires the `free_log` feature
- Logging macros require the `macros` feature
- Features can be used independently or together

## Features

- **Default**: Minimal core (no logging functionality)
- **`free_log`**: Enables free_log integration module
- **`macros`**: Enables logging macro utilities

## Development Status

This package currently provides:

1. **Modular Architecture**: Feature-gated logging components
2. **Free Log Integration**: Optional integration with free_log system
3. **Macro Support**: Optional logging macro utilities
4. **Minimal Overhead**: Include only needed logging features

The actual logging implementations are contained within the feature-gated modules. Enable the appropriate features to access logging functionality.

## Usage Patterns

```rust
// Feature-gated imports
#[cfg(feature = "free_log")]
use moosicbox_logging::*; // Free log functions

#[cfg(feature = "macros")]
use moosicbox_logging::*; // Logging macros

// Conditional compilation based on features
#[cfg(feature = "free_log")]
fn setup_free_log() {
    // Free log setup
}

#[cfg(feature = "macros")]
fn use_macros() {
    // Use logging macros
}
```

This design allows consumers to include only the logging components they need, keeping the package lightweight while providing extensible logging capabilities.
