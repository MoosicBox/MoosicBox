# MoosicBox Menu

A simple menu library providing basic menu-related functionality and models for the MoosicBox ecosystem.

## Features

- **Menu Models**: Re-exports menu data models and structures
- **Library Integration**: Menu functionality for library browsing
- **API Support**: Optional API endpoints for menu operations

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_menu = "0.1.1"

# Enable API features
moosicbox_menu = { version = "0.1.1", features = ["api"] }
```

## Usage

### Basic Usage

```rust
use moosicbox_menu::models;

fn main() {
    // Access menu models and structures
    // Models are re-exported from moosicbox_menu_models
}
```

### Library Menu Operations

```rust
use moosicbox_menu::library;

// Library-specific menu functionality
// Implementation details depend on the library module
```

### API Integration

With the `api` feature enabled:

```rust
// API endpoints for menu operations are available
// when the "api" feature is enabled
```

## Modules

- **`models`** - Re-exported menu data models from `moosicbox_menu_models`
- **`library`** - Library-specific menu functionality
- **`api`** - Optional API endpoints (requires `api` feature)

## Features

- `api` - Enable API endpoint functionality

## Dependencies

- `moosicbox_menu_models` - Core menu data models and structures
