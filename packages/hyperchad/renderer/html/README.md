# HyperChad HTML Renderer

Server-side HTML renderer for HyperChad with support for multiple web frameworks and deployment targets.

## Overview

The HyperChad HTML Renderer provides:

- **Server-side Rendering**: Generate static and dynamic HTML from HyperChad components
- **Framework Integration**: Support for Actix Web, Lambda, and generic HTTP servers
- **Responsive Design**: CSS media queries and responsive breakpoints
- **Static Assets**: Asset serving and management
- **HTML Tag Rendering**: Complete HTML element generation with styling
- **Partial Updates**: HTMX-compatible partial page updates
- **SEO Optimization**: Server-rendered HTML for search engine optimization

## Features

### HTML Generation

- **Complete HTML Output**: Full HTML documents with DOCTYPE, head, and body
- **CSS Styling**: Inline styles and CSS classes generation
- **Responsive CSS**: Media queries for responsive design
- **Element Attributes**: Data attributes, IDs, classes, and custom attributes
- **Semantic HTML**: Proper semantic HTML element generation

### Framework Support

- **Actix Web**: Full integration with Actix Web framework
- **AWS Lambda**: Serverless deployment support
- **Generic HTTP**: Works with any HTTP server implementation
- **Static Assets**: File serving and asset management

### Rendering Modes

- **Full Page Rendering**: Complete HTML documents
- **Partial Rendering**: HTMX-compatible partial updates
- **Component Rendering**: Individual component HTML generation
- **Template Rendering**: Reusable template rendering

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html = { path = "../hyperchad/renderer/html" }

# With Actix Web support
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["actix"]
}

# With Lambda support
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["lambda"]
}

# With asset serving
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["assets"]
}
```

## Usage

### Basic HTML Rendering with Router

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, router_to_actix};
use hyperchad_router::Router;
use hyperchad_renderer::{Renderer, Handle, ToRenderRunner};

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create HTML tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create router and define routes
    let router = Router::default();
    // ... configure your routes

    // Create HTML renderer with Actix integration
    let mut renderer = router_to_actix(tag_renderer, router)
        .with_title(Some("My App".to_string()))
        .with_description(Some("A HyperChad application".to_string()));

    // Initialize renderer
    renderer.init(
        800.0,    // width
        600.0,    // height
        None,     // x position
        None,     // y position
        None,     // background color
        Some("My App"), // title
        Some("My HyperChad App"), // description
        Some("width=device-width, initial-scale=1"), // viewport
    ).await?;

    // Convert to runner to start the application
    let runner = renderer.to_runner(Handle::current())?;

    Ok(())
}
```

### Direct HTML Generation

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, html::container_element_to_html_response};
use hyperchad_router::Container;
use std::collections::BTreeMap;

fn generate_html(container: &Container) -> Result<String, std::io::Error> {
    let tag_renderer = DefaultHtmlTagRenderer::default();
    let headers = BTreeMap::new();

    container_element_to_html_response(
        &headers,
        container,
        Some("width=device-width, initial-scale=1"),
        None,
        Some("My Page"),
        Some("Page description"),
        &tag_renderer,
    )
}
```

### Actix Web Integration

```rust
use actix_web::{web, App, HttpServer};
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, router_to_actix};
use hyperchad_router::Router;
use hyperchad_renderer::{Handle, ToRenderRunner};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Create HTML tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create and configure your router with routes
    let router = Router::default();
    // ... configure routes

    // Create HTML renderer with Actix integration
    let renderer = router_to_actix(tag_renderer, router)
        .with_title(Some("My App".to_string()))
        .with_viewport(Some("width=device-width, initial-scale=1".to_string()));

    // Convert to runner and start server
    let runner = renderer.to_runner(Handle::current())?;

    HttpServer::new(move || {
        App::new()
            // ... configure your Actix app
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

### Lambda Integration

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, router_to_lambda};
use hyperchad_router::Router;
use hyperchad_renderer::{Handle, ToRenderRunner};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    // Create HTML tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create and configure your router with routes
    let router = Router::default();
    // ... configure routes

    // Create HTML renderer with Lambda integration
    let renderer = router_to_lambda(tag_renderer, router)
        .with_title(Some("My Serverless App".to_string()))
        .with_viewport(Some("width=device-width, initial-scale=1".to_string()));

    // Convert to runner and start Lambda handler
    let runner = renderer.to_runner(Handle::current())?;
    runner.run()?;

    Ok(())
}
```

### Responsive Design

```rust
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_transformer::ResponsiveTrigger;

// Create tag renderer with responsive breakpoints
let tag_renderer = DefaultHtmlTagRenderer::default()
    .with_responsive_trigger(
        "mobile",
        ResponsiveTrigger::MaxWidth(hyperchad_transformer::Number::Real(768.0))
    )
    .with_responsive_trigger(
        "tablet",
        ResponsiveTrigger::MaxWidth(hyperchad_transformer::Number::Real(1024.0))
    );

// The tag renderer will generate appropriate CSS media queries
// for responsive overrides defined in your HyperChad components
```

### Static Asset Serving

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, router_to_actix};
use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};
use hyperchad_router::Router;
use std::path::PathBuf;

let tag_renderer = DefaultHtmlTagRenderer::default();
let router = Router::default();

// Configure static asset routes
let renderer = router_to_actix(tag_renderer, router)
    .with_static_asset_routes(vec![
        StaticAssetRoute {
            route: "/css/style.css".to_string(),
            target: AssetPathTarget::File(PathBuf::from("assets/style.css")),
        },
        StaticAssetRoute {
            route: "/js/app.js".to_string(),
            target: AssetPathTarget::File(PathBuf::from("assets/app.js")),
        },
        StaticAssetRoute {
            route: "/images/".to_string(),
            target: AssetPathTarget::Directory(PathBuf::from("assets/images")),
        },
    ]);
```

### Partial Updates

The HTML renderer supports partial page updates through the `PartialView` type. When a route returns a `PartialView`, the renderer:

- Generates only the updated HTML content
- Sets the `v-fragment` header with the target element selector
- Works seamlessly with HTMX and similar frameworks

```rust
use hyperchad_renderer::{PartialView, Content};
use hyperchad_router::Container;

// In your route handler, return a PartialView
async fn update_handler() -> Content {
    let updated_content = Container::default(); // your updated content

    Content::PartialView(PartialView {
        target: "content".to_string(),
        container: updated_content,
    })
}
```

### Extension System (requires `extend` feature)

The extension system allows you to hook into the rendering lifecycle:

```rust
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, router_to_actix, extend::{ExtendHtmlRenderer, HtmlRendererEventPub}};
use hyperchad_renderer::{PartialView, View, canvas::CanvasUpdate};
use hyperchad_router::Router;
use async_trait::async_trait;

// Implement custom extension
struct MyExtension;

#[async_trait]
impl ExtendHtmlRenderer for MyExtension {
    async fn render(
        &self,
        _pub: HtmlRendererEventPub,
        view: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Custom logic when rendering a view
        println!("Rendering view");
        Ok(())
    }

    async fn render_partial(
        &self,
        _pub: HtmlRendererEventPub,
        partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Custom logic when rendering a partial view
        println!("Rendering partial: {}", partial.target);
        Ok(())
    }
}

// Use the extension
let tag_renderer = DefaultHtmlTagRenderer::default();
let router = Router::default();
let renderer = router_to_actix(tag_renderer, router)
    .with_extend_html_renderer(MyExtension);
```

## Feature Flags

- **`actix`**: Enable Actix Web integration (enables `extend` feature)
- **`lambda`**: Enable AWS Lambda integration
- **`web-server`**: Enable generic web server integration (enables `extend` feature)
- **`web-server-actix`**: Enable Actix-based web server (enables `web-server` feature)
- **`web-server-simulator`**: Enable simulator-based web server (enables `web-server` feature)
- **`assets`**: Enable static asset serving across all backends
- **`extend`**: Enable renderer extension system via `ExtendHtmlRenderer` trait
- **`json`**: Enable JSON content support
- **`actions`**: Enable action handling (requires `actix` feature)
- **`sse`**: Enable Server-Sent Events support (requires `actix` feature)
- **`debug`**: Enable debug features
- **`fail-on-warnings`**: Treat compiler warnings as errors

**Default features**: `actix`, `assets`, `debug`, `extend`, `json`, `lambda`

## HTML Output Features

### CSS Generation

- **Inline Styles**: Component styles rendered as inline CSS attributes
- **CSS Classes**: Automatic CSS class application from HyperChad components
- **Media Queries**: Responsive breakpoint CSS via `@media` queries
- **Flexbox & Grid**: CSS flexbox and grid layout generation

### SEO Optimization

- **Semantic HTML**: Proper HTML5 semantic elements (div, aside, main, header, footer, section, etc.)
- **Meta Tags**: Title, description, and viewport meta tags
- **Server-side Rendering**: Full HTML documents for search engine crawlers
- **Accessibility**: Proper HTML structure and attributes

## Core Dependencies

- **hyperchad_renderer**: Core renderer traits and types
- **hyperchad_router**: Routing and navigation system
- **hyperchad_transformer**: Element transformation and styling
- **maud**: Type-safe HTML template generation
- **html-escape**: Safe HTML escaping
- **uaparser**: User agent parsing for client detection
- **flume**: Async channel communication
- **switchy**: HTTP models and utilities
- **switchy_http_models**: HTTP model types
- **qstring**: Query string parsing
- **bytes**: Efficient byte buffer handling
- **hyperchad_renderer_html_actix**: Actix Web integration (optional, enabled with `actix` feature)
- **hyperchad_renderer_html_lambda**: AWS Lambda integration (optional, enabled with `lambda` feature)
- **hyperchad_renderer_html_web_server**: Generic web server integration (optional, enabled with `web-server` feature)

## Integration

This renderer is designed for:

- **Web Applications**: Server-side rendered web apps with Actix Web
- **Serverless**: AWS Lambda deployments
- **Microservices**: Lightweight HTML rendering services
- **SEO-critical Sites**: Applications requiring search engine optimization

## Architecture

The package provides:

1. **`DefaultHtmlTagRenderer`**: Core HTML tag rendering implementation that implements `HtmlTagRenderer` trait
2. **`HtmlRenderer<T>`**: Generic renderer wrapper for different app types
3. **`router_to_actix()`**: Helper to create Actix Web-integrated renderers (requires `actix` feature)
4. **`router_to_lambda()`**: Helper to create Lambda-integrated renderers (requires `lambda` feature)
5. **`router_to_web_server()`**: Helper to create generic web server renderers (requires `web-server` feature)
6. **Extension System**: Via `ExtendHtmlRenderer` trait for custom rendering logic (requires `extend` feature)
7. **HTML Generation**: Functions in `html` module including `container_element_to_html()` and `container_element_to_html_response()`

## Performance Considerations

- **Server-side Rendering**: HTML generation happens on the server, reducing client-side work
- **Type-safe HTML**: Maud provides compile-time HTML validation
- **Efficient Rendering**: Direct byte-level HTML writing for performance
- **Partial Updates**: Support for efficient partial page updates
