# HyperChad HTML Web Server Renderer

This package provides a web server backend for HyperChad HTML rendering using the MoosicBox web server framework.

## Features

- **Web Server Backend**: Uses `moosicbox_web_server` for HTTP handling
- **Actix Support**: Optional actix-web backend support via the `actix` feature
- **Simulator Support**: Easy testing and simulation via the `simulator` feature
- **Assets**: Static asset serving via the `assets` feature

## Usage

```rust
use hyperchad_renderer_html_web_server::*;
use hyperchad_renderer::{Handle, ToRenderRunner};
use flume::unbounded;

// Create a web server app with a response processor
let (tx, rx) = unbounded();
let app = WebServerApp::new(processor, rx);

// Convert to a runner and start
let handle = Handle::current();
let mut runner = app.to_runner(handle)?;
runner.run()?;
```

## Cargo Features

- `actix` - Enable actix-web backend support in `moosicbox_web_server`
- `simulator` - Enable simulation capabilities for testing in `moosicbox_web_server`
- `assets` - Enable static asset serving (enabled by default)
- `debug` - Enable debug features (enabled by default)