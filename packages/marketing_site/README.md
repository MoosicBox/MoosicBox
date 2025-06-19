# MoosicBox Marketing Website

A multi-platform marketing website built with the HyperChad framework, supporting multiple rendering targets including web, desktop, and serverless deployment.

## Overview

The MoosicBox Marketing Website showcases the MoosicBox music server with support for:

- **Multi-Platform Rendering**: Web (HTML), Desktop (Egui/FLTK), Serverless (Lambda)
- **Modern Web Technologies**: Vanilla JS, HTMX-style interactions
- **Responsive Design**: Optimized for all device sizes
- **Static Site Generation**: Pre-built assets for fast loading
- **Server-Side Rendering**: Dynamic content generation
- **Progressive Enhancement**: Works with and without JavaScript

## Installation

### From Source

```bash
cargo install --path packages/marketing_site --features "default"
```

### Dependencies

- **System dependencies**: None required for basic web deployment
- **Optional**: X11 or Wayland (for desktop rendering)

## Usage

### Web Server Mode

Start the marketing website as a web server:

```bash
moosicbox_marketing_site
```

Or using cargo:

```bash
cargo run --bin moosicbox_marketing_site --features "actix,html,vanilla-js"
```

### Desktop Application Mode

Run as a native desktop application:

```bash
# Using Egui (recommended)
cargo run --bin moosicbox_marketing_site --features "egui-wgpu" --no-default-features

# Using FLTK
cargo run --bin moosicbox_marketing_site --features "fltk" --no-default-features
```

### Lambda Serverless Mode

Deploy to AWS Lambda:

```bash
cargo build --bin moosicbox_marketing_site_lambda_vanilla_js --features "lambda,vanilla-js"
```

## Configuration

### Environment Variables

| Variable             | Description             | Default        |
| -------------------- | ----------------------- | -------------- |
| `BIND_ADDRESS`       | Server bind address     | `0.0.0.0:3000` |
| `STATIC_ASSETS_PATH` | Path to static assets   | `./assets`     |
| `TEMPLATE_CACHE`     | Enable template caching | `true`         |
| `LOG_LEVEL`          | Logging level           | `info`         |

### Runtime Configuration

```bash
# Development mode with hot reloading
RUST_LOG=debug cargo run --features "dev,console-subscriber"

# Production mode
cargo run --release --features "default" --no-default-features
```

## Features

The marketing site supports various feature combinations:

### Rendering Backends

- `html` - HTML web rendering with server-side generation
- `vanilla-js` - Enhanced with vanilla JavaScript interactions
- `egui-wgpu` - Native desktop UI with GPU acceleration
- `egui-glow` - Native desktop UI with OpenGL
- `fltk` - Native desktop UI with FLTK

### Deployment Targets

- `actix` - Actix Web server for standalone deployment
- `lambda` - AWS Lambda serverless deployment
- `static-routes` - Pre-generated static routes

### Development Features

- `dev` - Development mode with hot reloading
- `debug` - Enhanced debugging output
- `console-subscriber` - Tokio console integration
- `profiling-puffin` - Performance profiling
- `unsafe` - Enable performance optimizations

## Deployment

### Standalone Web Server

```bash
# Build for production
cargo build --release --bin moosicbox_marketing_site

# Run with custom configuration
BIND_ADDRESS=0.0.0.0:8080 ./target/release/moosicbox_marketing_site
```

### Docker Deployment

```dockerfile
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin moosicbox_marketing_site --features "actix,html,vanilla-js"

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/moosicbox_marketing_site /usr/local/bin/
EXPOSE 3000
CMD ["moosicbox_marketing_site"]
```

### AWS Lambda Deployment

```bash
# Build Lambda function
cargo build --release --bin moosicbox_marketing_site_lambda_vanilla_js \
  --features "lambda,vanilla-js" --no-default-features

# Package for deployment
cp target/release/moosicbox_marketing_site_lambda_vanilla_js bootstrap
zip lambda-deployment.zip bootstrap
```

### Static Site Generation

Generate static assets for CDN deployment:

```bash
cargo run --features "static-routes,assets" -- --generate-static ./output
```

## Desktop Application

### Building Desktop App

```bash
# Cross-platform with Egui
cargo build --release --features "egui-wgpu" --no-default-features

# Linux-specific with better integration
cargo build --release --features "egui-wgpu,x11" --no-default-features

# macOS-specific
cargo build --release --features "egui-wgpu" --no-default-features --target x86_64-apple-darwin
```

### Desktop Features

- **Native file dialogs**
- **System tray integration**
- **Multi-window support**
- **Native menu integration**
- **Hardware acceleration**

## Development

### Local Development

```bash
# Start development server with hot reload
cargo run --features "dev,console-subscriber,debug"

# Monitor performance
cargo run --features "profiling-puffin,dev"
```

### Asset Management

```bash
# Rebuild assets
cargo run --features "assets" -- --rebuild-assets

# Optimize assets
cargo run --features "assets" -- --optimize-assets
```

### Testing Different Backends

```bash
# Test HTML rendering
cargo test --features "html"

# Test desktop rendering
cargo test --features "egui-wgpu"

# Test Lambda rendering
cargo test --features "lambda,vanilla-js"
```

## Architecture

### Multi-Platform Framework

The marketing site uses the HyperChad framework for:

- **Component-based architecture**
- **State management**
- **Routing system**
- **Asset pipeline**
- **Template engine**

### Rendering Pipeline

1. **Content Generation**: Dynamic content from templates
2. **Asset Processing**: CSS, JS, and image optimization
3. **Route Resolution**: Static and dynamic route handling
4. **Response Generation**: Platform-specific output

## Performance

### Web Performance

- **Server-Side Rendering** for fast initial page loads
- **Progressive Enhancement** for improved user experience
- **Asset Optimization** with automatic compression
- **CDN-Ready** static asset generation

### Desktop Performance

- **GPU Acceleration** with wgpu backend
- **Native Performance** with minimal overhead
- **Memory Efficiency** with optimized rendering

## Content Management

### Page Structure

```
src/
├── pages/          # Page components
├── components/     # Reusable UI components
├── assets/         # Static assets
├── styles/         # CSS/styling
└── templates/      # HTML templates
```

### Adding New Pages

```rust
// Add to routing configuration
pub fn routes() -> Vec<Route> {
    vec![
        Route::new("/", home_page),
        Route::new("/features", features_page),
        Route::new("/download", download_page),
        // Add new routes here
    ]
}
```

## Troubleshooting

### Common Issues

1. **Build failures**: Ensure all required features are enabled
2. **Asset loading**: Check static asset paths and permissions
3. **Desktop rendering**: Verify graphics drivers and window system
4. **Lambda deployment**: Check function memory and timeout settings

### Debug Information

```bash
# Enable detailed logging
RUST_LOG=moosicbox_marketing_site=debug cargo run

# Profile performance
cargo run --features "profiling-puffin" -- --profile
```

## See Also

- [HyperChad Framework](../hyperchad/README.md) - Underlying UI framework
- [MoosicBox Server](../server/README.md) - Main application being marketed
- [MoosicBox Native App](../app/native/README.md) - Desktop client application
