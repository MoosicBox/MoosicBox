# HyperChad App

Application framework and builder for HyperChad applications with routing and rendering.

## Overview

The HyperChad App package provides:

- **App Builder**: Fluent application configuration and setup
- **Routing Integration**: Built-in router and navigation support
- **Renderer Abstraction**: Support for multiple rendering backends
- **Command Line Interface**: CLI for generation, cleaning, and serving
- **Action Handling**: Interactive action processing and event handling
- **Asset Management**: Static asset routing and management

## Features

### Application Builder
- **Fluent API**: Chain configuration methods for easy setup
- **Router Integration**: Built-in routing with initial route support
- **Window Configuration**: Position, size, background, and metadata
- **Runtime Management**: Async runtime handling and configuration
- **Event Handling**: Action handlers and resize listeners

### CLI Commands
- **Serve**: Start development server
- **Generate**: Build static assets and routes
- **Clean**: Clean generated assets
- **Dynamic Routes**: List available dynamic routes

### Renderer Support
- **Multiple Backends**: Support for different rendering targets
- **Generator Interface**: Static site generation capabilities
- **Cleaner Interface**: Asset cleanup and management
- **Render Runner**: Application execution and lifecycle

### Action System
- **Action Handlers**: Custom action processing
- **Logic Integration**: Conditional action handling
- **Event Processing**: User interaction event handling
- **Resize Handling**: Window resize event management

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_app = { path = "../hyperchad/app" }

# Enable additional features
hyperchad_app = {
    path = "../hyperchad/app",
    features = ["logic", "assets"]
}
```

## Usage

### Basic Application Setup

```rust
use hyperchad_app::{AppBuilder, Error};
use hyperchad_router::Router;
use hyperchad_renderer::Color;

// Create router
let router = Router::new()
    .with_route("/", |_req| async {
        Some(hyperchad_renderer::Content::view("<h1>Home</h1>"))
    })
    .with_route("/about", |_req| async {
        Some(hyperchad_renderer::Content::view("<h1>About</h1>"))
    });

// Build application
let app = AppBuilder::new()
    .with_router(router)
    .with_initial_route("/")
    .with_title("My HyperChad App".to_string())
    .with_size(800.0, 600.0)
    .with_background(Color::from_hex("#1a1a1a"))
    .build(renderer)?;

// Run application
app.run()?;
```

### Window Configuration

```rust
let app = AppBuilder::new()
    .with_router(router)
    .with_position(100, 100)           // Window position
    .with_size(1024.0, 768.0)         // Window size
    .with_background(Color::WHITE)     // Background color
    .with_title("App Title".to_string())
    .with_description("App description".to_string())
    .with_viewport("width=device-width, initial-scale=1".to_string())
    .build(renderer)?;
```

### Action Handling

```rust
use hyperchad_actions::logic::Value;

let app = AppBuilder::new()
    .with_router(router)
    .with_action_handler(|action, value| {
        match action {
            "custom-action" => {
                println!("Custom action triggered with value: {:?}", value);
                Ok(true) // Action handled
            }
            _ => Ok(false) // Action not handled
        }
    })
    .with_on_resize(|width, height| {
        println!("Window resized to {}x{}", width, height);
        Ok(())
    })
    .build(renderer)?;
```

### Static Asset Routes

```rust
use hyperchad_renderer::assets::StaticAssetRoute;

let app = AppBuilder::new()
    .with_router(router)
    .with_static_asset_route(StaticAssetRoute {
        route: "/static/css/style.css".to_string(),
        content_type: "text/css".to_string(),
        content: include_str!("../assets/style.css").to_string(),
    })
    .with_static_asset_route(StaticAssetRoute {
        route: "/static/js/app.js".to_string(),
        content_type: "application/javascript".to_string(),
        content: include_str!("../assets/app.js").to_string(),
    })
    .build(renderer)?;
```

### CLI Usage

```bash
# Serve application in development mode
my-app serve

# Generate static assets
my-app gen --output ./dist

# Clean generated assets
my-app clean --output ./dist

# List dynamic routes
my-app dynamic-routes
```

### Runtime Management

```rust
use switchy::unsync::runtime::Handle;

// Use custom runtime handle
let runtime_handle = Handle::current();

let app = AppBuilder::new()
    .with_router(router)
    .with_runtime_handle(runtime_handle)
    .build(renderer)?;
```

### Error Handling

```rust
use hyperchad_app::{Error, BuilderError};

match AppBuilder::new().build(renderer) {
    Ok(app) => {
        // Application built successfully
        app.run()?;
    }
    Err(BuilderError::MissingRouter) => {
        println!("Router is required");
    }
    Err(BuilderError::MissingRuntime) => {
        println!("Runtime is required");
    }
}
```

## App Structure

### AppBuilder
- **Router**: Application routing configuration
- **Initial Route**: Starting route for the application
- **Window Properties**: Position, size, background, title, description
- **Runtime**: Async runtime configuration
- **Event Handlers**: Action and resize event handlers
- **Assets**: Static asset route configuration

### App
- **Renderer**: Rendering backend implementation
- **Router**: Request routing and handling
- **Runtime**: Async runtime management
- **Configuration**: Window and application settings

## CLI Commands

### serve
Start development server with hot reloading and dynamic routing.

### gen
Generate static assets and pre-rendered routes for production deployment.

### clean
Remove generated assets and clean build artifacts.

### dynamic-routes
Display available dynamic routes for debugging and development.

## Traits

### Generator
```rust
#[async_trait]
pub trait Generator {
    async fn generate(&self, router: &Router, output: Option<String>) -> Result<(), Error>;
}
```

### Cleaner
```rust
#[async_trait]
pub trait Cleaner {
    async fn clean(&self, output: Option<String>) -> Result<(), Error>;
}
```

## Feature Flags

- **`logic`**: Enable action logic and conditional handling
- **`assets`**: Enable static asset management

## Dependencies

- **HyperChad Router**: Application routing system
- **HyperChad Renderer**: Rendering abstraction layer
- **HyperChad Actions**: Interactive action system
- **Clap**: Command-line argument parsing
- **Async Trait**: Async trait support
- **Switchy**: Async runtime abstraction

## Integration

This package is designed for:
- **Desktop Applications**: Native desktop app development
- **Web Applications**: Browser-based applications
- **Static Sites**: Static site generation
- **Development Tools**: Development server and asset management
- **Cross-Platform**: Consistent API across different platforms
