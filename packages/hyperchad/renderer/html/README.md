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
use hyperchad_renderer::Renderer;

#[tokio::main]
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
        &[],  // css_urls
        &[],  // css_paths
        &[],  // inline_css
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create HTML tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create and configure your router with routes
    let router = Router::default();
    // ... configure routes

    // Create HTML renderer with Lambda integration
    let _renderer = router_to_lambda(tag_renderer, router)
        .with_title(Some("My Serverless App".to_string()))
        .with_viewport(Some("width=device-width, initial-scale=1".to_string()));

    // The Lambda runtime and handler are managed internally
    // Use this renderer within your Lambda function handler

    Ok(())
}
```

### Responsive Design

```rust
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_transformer::{ResponsiveTrigger, Number};

// Create tag renderer with responsive breakpoints
let tag_renderer = DefaultHtmlTagRenderer::default()
    .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Real(768.0)))
    .with_responsive_trigger("tablet", ResponsiveTrigger::MaxWidth(Number::Real(1024.0)));

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
            not_found_behavior: None,
        },
        StaticAssetRoute {
            route: "/js/app.js".to_string(),
            target: AssetPathTarget::File(PathBuf::from("assets/app.js")),
            not_found_behavior: None,
        },
        StaticAssetRoute {
            route: "/images/".to_string(),
            target: AssetPathTarget::Directory(PathBuf::from("assets/images")),
            not_found_behavior: None,
        },
    ]);
```

### Partial Updates

The HTML renderer supports partial page updates through the `View` type. When a route returns a `View` with fragments, the renderer:

- Generates only the updated HTML content for fragments
- Sets custom headers to communicate fragment information
- Works seamlessly with HTMX and similar frameworks

```rust
use hyperchad_renderer::{View, Content, ReplaceContainer};
use hyperchad_router::Container;
use hyperchad_transformer::models::Selector;

// In your route handler, return a View with fragments
async fn update_handler() -> Content {
    let updated_content = Container::default(); // your updated content

    Content::View(Box::new(View {
        primary: None,
        fragments: vec![ReplaceContainer {
            selector: "#content".try_into().unwrap(),
            container: updated_content,
        }],
        delete_selectors: vec![],
    }))
}
```

## Feature Flags

- **`actix`**: Enable Actix Web integration (implies `extend`)
- **`lambda`**: Enable AWS Lambda integration
- **`web-server`**: Enable generic web server integration (implies `extend`)
- **`web-server-actix`**: Enable Actix-based web server (implies `web-server`)
- **`web-server-simulator`**: Enable simulator-based web server (implies `web-server`)
- **`assets`**: Enable static asset serving
- **`extend`**: Enable renderer extension system
- **`json`**: Enable JSON content support
- **`actions`**: Enable action handling (requires `actix`)
- **`sse`**: Enable Server-Sent Events support (requires `actix`)
- **`debug`**: Enable debug features
- **`fail-on-warnings`**: Treat compiler warnings as errors

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
- **Maud**: Type-safe HTML template generation
- **html-escape**: Safe HTML escaping
- **uaparser**: User agent parsing for client detection
- **flume**: Async channel communication
- **switchy**: HTTP models and utilities
- **hyperchad_renderer_html_actix**: Actix Web integration (optional)
- **hyperchad_renderer_html_lambda**: AWS Lambda integration (optional)
- **hyperchad_renderer_html_web_server**: Generic web server integration (optional)

## Integration

This renderer is designed for:

- **Web Applications**: Server-side rendered web apps with Actix Web
- **Serverless**: AWS Lambda deployments
- **Microservices**: Lightweight HTML rendering services
- **SEO-critical Sites**: Applications requiring search engine optimization

## Architecture

The package provides:

1. **`DefaultHtmlTagRenderer`**: Core HTML tag rendering implementation
2. **`HtmlRenderer<T>`**: Generic renderer wrapper for different app types
3. **`router_to_actix()`**: Helper to create Actix Web-integrated renderers
4. **`router_to_lambda()`**: Helper to create Lambda-integrated renderers
5. **`router_to_web_server()`**: Helper to create generic web server renderers (optional)
6. **Extension System**: Via `ExtendHtmlRenderer` trait for custom rendering logic

## Performance Considerations

- **Server-side Rendering**: HTML generation happens on the server, reducing client-side work
- **Type-safe HTML**: Maud provides compile-time HTML validation
- **Efficient Rendering**: Direct byte-level HTML writing for performance
- **Partial Updates**: Support for efficient partial page updates
