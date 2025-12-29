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
hyperchad_template = { path = "../hyperchad/template" }

# Or customize features (logic and assets are enabled by default)
hyperchad_app = {
    path = "../hyperchad/app",
    default-features = false,
    features = ["egui", "logic"]
}
hyperchad_template = { path = "../hyperchad/template" }
```

## Usage

### Basic Application Setup

```rust
use hyperchad_app::{AppBuilder, Error};
use hyperchad_router::Router;
use hyperchad_renderer::{Color, Content};
use hyperchad_template::container;

// Create router
let router = Router::new()
    .with_route("/", |_req| async {
        Content::builder()
            .with_primary(container! {
                h1 { "Home" }
            })
            .build()
    })
    .with_route("/about", |_req| async {
        Content::builder()
            .with_primary(container! {
                h1 { "About" }
            })
            .build()
    });

// Build application
let app = AppBuilder::new()
    .with_router(router)
    .with_initial_route("/")
    .with_title("My HyperChad App".to_string())
    .with_size(800.0, 600.0)
    .with_background(Color::from_hex("#1a1a1a"))
    .build_default()?;

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
    .build_default()?;
```

### Action Handling

Note: Action handling requires the `logic` feature (enabled by default).

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
    .build_default()?;
```

### Static Asset Routes

Note: Static asset routes require the `assets` feature (enabled by default).

```rust
use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};
use bytes::Bytes;

let app = AppBuilder::new()
    .with_router(router)
    .with_static_asset_route(StaticAssetRoute {
        route: "/static/css/style.css".to_string(),
        target: AssetPathTarget::FileContents(
            Bytes::from(include_str!("../assets/style.css"))
        ),
        not_found_behavior: None,
    })
    .with_static_asset_route(StaticAssetRoute {
        route: "/static/js/app.js".to_string(),
        target: AssetPathTarget::FileContents(
            Bytes::from(include_str!("../assets/app.js"))
        ),
        not_found_behavior: None,
    })
    .build_default()?;
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
    .build_default()?;
```

### Error Handling

```rust
use hyperchad_app::{Error, BuilderError};

match AppBuilder::new().build_default() {
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

Start development server.

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

### Default Features

The following features are enabled by default:

- **`actix`**: Actix web server support
- **`assets`**: Static asset management
- **`egui-wgpu`**: Egui renderer with WGPU backend
- **`fltk`**: FLTK renderer support
- **`format`**: Code formatting support
- **`html`**: HTML rendering support
- **`json`**: JSON content support
- **`lambda`**: AWS Lambda support
- **`logic`**: Action logic and conditional handling
- **`static-routes`**: Static route generation
- **`vanilla-js`**: Vanilla JavaScript renderer

### Additional Features

- **`egui`**, **`egui-glow`**, **`egui-v1`**, **`egui-v2`**: Egui renderer variants
- **`actions`**, **`sse`**: Server-sent events and action support
- **`web-server`**, **`web-server-actix`**, **`web-server-simulator`**: Web server variants
- **`wayland`**, **`x11`**: Linux display server support
- **`debug`**: Debug mode
- **`profiling-puffin`**, **`profiling-tracing`**, **`profiling-tracy`**: Profiling backends
- **`syntax-highlighting`**: Code syntax highlighting
- **`unsafe`**: Unsafe optimizations
- **`benchmark`**: Benchmarking support
- **`all-plugins`**: Enable all vanilla-js plugins
- **`plugin-*`**: Individual vanilla-js plugins (actions, canvas, form, idiomorph, nav, routing, sse, tauri-event, uuid, etc.)

## Dependencies

### Core Dependencies

- **hyperchad_router**: Application routing system
- **hyperchad_renderer**: Rendering abstraction layer
- **hyperchad_actions**: Interactive action system
- **switchy**: Async runtime abstraction with Tokio support
- **switchy_env**: Environment variable utilities
- **moosicbox_env_utils**: MoosicBox environment utilities
- **moosicbox_assert**: Assertion utilities

### Optional Renderer Dependencies

- **hyperchad_renderer_egui**: Egui rendering backend (optional)
- **hyperchad_renderer_fltk**: FLTK rendering backend (optional)
- **hyperchad_renderer_html**: HTML rendering backend (optional)
- **hyperchad_renderer_vanilla_js**: Vanilla JS rendering backend (optional)

### Utility Dependencies

- **async-trait**: Async trait support
- **clap**: Command-line argument parsing
- **flume**: Multi-producer multi-consumer channels
- **log**: Logging facade
- **serde_json**: JSON serialization
- **thiserror**: Error derive macros

## Integration

This package is designed for:

- **Desktop Applications**: Native desktop app development
- **Web Applications**: Browser-based applications
- **Static Sites**: Static site generation
- **Development Tools**: Development server and asset management
- **Cross-Platform**: Consistent API across different platforms
