# HyperChad HTML Renderer

HTML rendering backend for the HyperChad UI framework.

This crate provides HTML rendering capabilities for HyperChad applications,
converting HyperChad containers into HTML elements with CSS styling. It supports
responsive design through media queries and can integrate with various web frameworks.

## Features

- HTML rendering with CSS styling and responsive design
- Support for multiple backend integrations (Actix, Lambda, custom web servers)
- Static asset routing
- Extensible renderer with custom event handling
- Canvas rendering support

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html = "0.1"
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `actix` | Enables Actix web framework integration (default) |
| `lambda` | Enables AWS Lambda integration (default) |
| `web-server` | Enables custom web server support |
| `assets` | Enables static asset routing (default) |
| `extend` | Enables renderer extension capabilities (default) |
| `sse` | Enables server-sent events support (requires `actix`) |
| `json` | Enables JSON serialization support (default) |
| `debug` | Enables debug mode (default) |

## Usage

### Basic HTML Rendering

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, HtmlRenderer};
use hyperchad_renderer_html::stub::StubApp;

let tag_renderer = DefaultHtmlTagRenderer::default();
let app = StubApp::new(tag_renderer);
let renderer = HtmlRenderer::new(app);
```

### With Responsive Triggers

```rust
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_transformer::{ResponsiveTrigger, Number};

let tag_renderer = DefaultHtmlTagRenderer::default()
    .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Integer(768)))
    .with_responsive_trigger("tablet", ResponsiveTrigger::MaxWidth(Number::Integer(1024)));
```

### Actix Web Integration

With the `actix` feature enabled:

```rust
use hyperchad_renderer_html::{router_to_actix, DefaultHtmlTagRenderer};

let tag_renderer = DefaultHtmlTagRenderer::default();
let router = hyperchad_router::Router::new();
let renderer = router_to_actix(tag_renderer, router);
```

### AWS Lambda Integration

With the `lambda` feature enabled:

```rust
use hyperchad_renderer_html::{router_to_lambda, DefaultHtmlTagRenderer};

let tag_renderer = DefaultHtmlTagRenderer::default();
let router = hyperchad_router::Router::new();
let renderer = router_to_lambda(tag_renderer, router);
```

### Custom Web Server Integration

With the `web-server` feature enabled:

```rust
use hyperchad_renderer_html::{router_to_web_server, DefaultHtmlTagRenderer};

let tag_renderer = DefaultHtmlTagRenderer::default();
let router = hyperchad_router::Router::new();
let renderer = router_to_web_server(tag_renderer, router);
```

## License

MPL-2.0
