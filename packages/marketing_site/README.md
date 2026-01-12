# MoosicBox Marketing Website

A multi-platform marketing website built with the HyperChad framework, supporting multiple rendering targets including web, desktop, and serverless deployment.

## Overview

The MoosicBox Marketing Website showcases the MoosicBox music server with support for:

- **Multi-Platform Rendering**: Web (HTML), Desktop (Egui/FLTK), Serverless (Lambda)
- **Modern Web Technologies**: Vanilla JS for client-side interactions
- **Responsive Design**: Optimized for all device sizes
- **Static Route Generation**: Pre-generated routes for improved performance
- **Server-Side Rendering**: Dynamic content generation via HyperChad framework
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

| Variable        | Description                      | Default  |
| --------------- | -------------------------------- | -------- |
| `WINDOW_WIDTH`  | Initial window width (pixels)    | `1000.0` |
| `WINDOW_HEIGHT` | Initial window height (pixels)   | `600.0`  |
| `WINDOW_X`      | Initial window X position        | (unset)  |
| `WINDOW_Y`      | Initial window Y position        | (unset)  |
| `MAX_THREADS`   | Max blocking threads             | `64`     |
| `TOKIO_CONSOLE` | Enable tokio console (1 or true) | (unset)  |
| `RUST_LOG`      | Logging level                    | (unset)  |

### Runtime Configuration

```bash
# Development mode with debugging
RUST_LOG=debug cargo run --features "dev,console-subscriber"

# Production mode
cargo run --release --features "default" --no-default-features
```

## Features

The marketing site supports various feature combinations:

### Rendering Backends

- `html` - HTML web rendering with server-side generation
- `vanilla-js` - Enhanced with vanilla JavaScript interactions (includes routing and navigation plugins)
- `egui-wgpu` - Native desktop UI with GPU acceleration via wgpu
- `egui-glow` - Native desktop UI with OpenGL via glow
- `fltk` - Native desktop UI with FLTK toolkit

### Deployment Targets

- `actix` - Actix Web server for standalone deployment
- `lambda` - AWS Lambda serverless deployment
- `static-routes` - Pre-generated static routes
- `assets` - Enable static asset serving from `public/` directory

### Platform-Specific Features

- `wayland` - Wayland window system support (Linux)
- `x11` - X11 window system support (Linux)
- `windows-console` - Show console window on Windows in release builds

### Development Features

- `dev` - Development mode (enables `assets` and `static-routes`)
- `debug` - Enhanced debugging output
- `console-subscriber` - Tokio console integration
- `profiling-puffin` - Performance profiling with Puffin
- `profiling-tracing` - Performance profiling with tracing
- `profiling-tracy` - Performance profiling with Tracy
- `unsafe` - Enable performance optimizations
- `benchmark` - Enable benchmarking features
- `format` - Enable formatting features

## Deployment

### Standalone Web Server

```bash
# Build for production
cargo build --release --bin moosicbox_marketing_site

# Run the binary
./target/release/moosicbox_marketing_site
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
EXPOSE 8343
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

The `static-routes` feature enables pre-generated static routes for improved performance. Build with these features:

```bash
cargo build --release --features "static-routes,assets"
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

- **Hardware acceleration** (via `egui-wgpu` with GPU acceleration)
- **Cross-platform support** (Linux with X11/Wayland, macOS, Windows)
- **Native window integration**

## Development

### Local Development

```bash
# Start development server with debugging
cargo run --features "dev,console-subscriber,debug"

# Monitor performance with Puffin profiling
cargo run --features "profiling-puffin,dev"

# Enable Tokio console
TOKIO_CONSOLE=1 cargo run --features "console-subscriber"
```

### Asset Management

The `assets` feature enables serving static assets from the `public/` directory. Assets are bundled at build time and served through static routes.

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

- **Component-based architecture** - Using HyperChad's template system with the `container!` macro
- **Routing system** - Static and dynamic route handling via `Router`
- **Asset pipeline** - Serving static assets from the `public/` directory
- **Responsive design** - Breakpoint-based responsive triggers for mobile/desktop layouts
- **Multi-renderer support** - Same component code renders to HTML, Egui, or FLTK

### Rendering Pipeline

1. **Route Resolution**: Router matches incoming requests to route handlers
2. **Component Rendering**: HyperChad templates generate UI component trees
3. **Renderer Transformation**: Component trees are transformed to target format (HTML/Native UI)
4. **Response Generation**: Platform-specific output delivered to client

## Performance

### Web Performance

- **Server-Side Rendering** for fast initial page loads
- **Progressive Enhancement** with optional vanilla-js features
- **Static Route Generation** reduces runtime overhead
- **Efficient Asset Serving** from bundled public directory

### Desktop Performance

- **GPU Acceleration** with wgpu backend
- **Native Performance** with minimal overhead
- **Memory Efficiency** with optimized rendering

## Content Management

### Project Structure

```
packages/marketing_site/
├── src/
│   ├── main.rs              # Main desktop/web binary entry point
│   ├── lib.rs               # Core library with routing and initialization
│   ├── lambda.rs            # Lambda runtime wrapper
│   ├── lambda_vanilla_js.rs # Lambda binary entry point
│   └── download.rs          # Download page route handler
├── ui/                      # Separate workspace package (moosicbox_marketing_site_ui)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           # UI components (header, pages, layouts)
│       └── download.rs      # Download page UI
├── public/                  # Static assets (images, favicon)
└── hyperchad/              # HyperChad framework integration files
```

### Adding New Pages

Edit `src/lib.rs` to add routes to the `ROUTER` static:

```rust
pub static ROUTER: LazyLock<Router> = LazyLock::new(|| {
    Router::new()
        .with_static_route(&["/", "/home"], |_| async {
            moosicbox_marketing_site_ui::home()
        })
        .with_static_route(&["/your-new-page"], |_| async {
            moosicbox_marketing_site_ui::your_new_page()
        })
        // Add more routes here
});
```

Then implement the corresponding UI function in `ui/src/lib.rs`.

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

# Profile performance with Puffin
cargo run --features "profiling-puffin"

# Enable Tokio console for async debugging
TOKIO_CONSOLE=1 cargo run --features "console-subscriber"
```

## See Also

- [HyperChad Framework](../hyperchad/README.md) - Underlying UI framework
- [MoosicBox Server](../server/README.md) - Main application being marketed
- [MoosicBox Native App](../app/native/README.md) - Desktop client application
