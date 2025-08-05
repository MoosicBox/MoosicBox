# HyperChad HTML Web Server Renderer

This package provides a web server backend for HyperChad HTML rendering using the MoosicBox web server framework.

## Features

- **Web Server Backend**: Uses `moosicbox_web_server` for HTTP handling
- **Actix Support**: Optional actix-web backend support via the `actix` feature
- **Simulator Support**: Easy testing and simulation via the `simulator` feature
- **Actions**: Support for HyperChad actions
- **SSE**: Server-sent events support
- **Assets**: Static asset serving

## Usage

```rust
use hyperchad_renderer_html_web_server::*;

// Create a web server app with your tag renderer and router
let app = router_to_web_server(tag_renderer, router);
```

## Features

- `actix` - Enable actix-web backend support
- `simulator` - Enable simulation capabilities for testing
- `actions` - Enable HyperChad actions support
- `sse` - Enable server-sent events
- `assets` - Enable static asset serving