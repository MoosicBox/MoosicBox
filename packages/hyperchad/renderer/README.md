# HyperChad Renderer

Core rendering abstractions and traits for HyperChad UI frameworks.

## Overview

The HyperChad Renderer package provides:

- **Renderer Traits**: Abstract interfaces for different rendering backends
- **Content Types**: Structured content representation (View, PartialView, JSON)
- **Event System**: Renderer event handling and processing
- **HTML Generation**: HTML tag rendering and CSS generation
- **Asset Management**: Optional static asset handling
- **Canvas Support**: Optional canvas rendering capabilities

## Features

### Core Abstractions
- **Renderer Trait**: Main rendering interface for backends
- **RenderRunner**: Application execution and lifecycle management
- **ToRenderRunner**: Conversion trait for renderer instances
- **HtmlTagRenderer**: HTML-specific rendering capabilities

### Content System
- **View**: Full page view with immediate and future content
- **PartialView**: Targeted partial content updates
- **Content Enum**: Unified content representation (View, PartialView, Raw)
- **JSON Support**: Optional JSON content handling (with `json` feature)

### Event Handling
- **RendererEvent**: Event types for renderer communication
- **Custom Events**: User-defined event processing
- **Canvas Events**: Optional canvas update events
- **Async Events**: Async event emission and handling

### HTML Rendering
- **Tag Generation**: HTML element and attribute generation
- **CSS Media Queries**: Responsive CSS generation
- **Root HTML**: Complete HTML document generation
- **Partial HTML**: Fragment HTML generation

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
# With default features (assets, canvas, html, viewport, viewport-immediate, viewport-retained)
hyperchad_renderer = { path = "../hyperchad/renderer" }

# With custom features
hyperchad_renderer = {
    path = "../hyperchad/renderer",
    default-features = false,
    features = ["json", "html", "canvas"]
}
```

## Usage

### Implementing a Renderer

```rust
use hyperchad_renderer::{Renderer, View, PartialView, Color, Handle};
use async_trait::async_trait;

struct MyRenderer {
    // Renderer state
}

#[async_trait]
impl Renderer for MyRenderer {
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Initialize renderer with window properties
        Ok(())
    }

    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Render full view
        Ok(())
    }

    async fn render_partial(&self, partial: PartialView) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Render partial content update
        Ok(())
    }

    async fn emit_event(&self, event_name: String, event_value: Option<String>) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Handle event emission
        Ok(())
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        // Add responsive breakpoint trigger
    }
}
```

### Content Creation

```rust
use hyperchad_renderer::{Content, View, PartialView};
use hyperchad_transformer::Container;

// Create view content
let view_content = Content::view(View {
    future: None,
    immediate: Container::default(),
});

// Create partial view content
let partial_content = Content::partial_view(
    "#content",
    Container::default()
);

// From string (as raw HTML)
let string_content: Content = "<div>Hello World</div>".try_into()?;

// From container
let container_content = Content::from(Container::default());
```

### HTML Tag Renderer

```rust
use hyperchad_renderer::{HtmlTagRenderer, Color};
use hyperchad_transformer::{Container, ResponsiveTrigger};
use std::collections::BTreeMap;

struct MyHtmlRenderer {
    responsive_triggers: BTreeMap<String, ResponsiveTrigger>,
}

impl HtmlTagRenderer for MyHtmlRenderer {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.responsive_triggers.insert(name, trigger);
    }

    fn element_attrs_to_html(
        &self,
        f: &mut dyn std::io::Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        // Generate HTML attributes for container
        Ok(())
    }

    fn reactive_conditions_to_css(
        &self,
        f: &mut dyn std::io::Write,
        container: &Container,
    ) -> Result<(), std::io::Error> {
        // Generate CSS media queries for responsive triggers
        Ok(())
    }

    fn root_html(
        &self,
        headers: &BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
    ) -> String {
        format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <meta name="description" content="{}">
    <meta name="viewport" content="{}">
</head>
<body style="background-color: {}">
    {}
</body>
</html>
        "#,
            title.unwrap_or("HyperChad App"),
            description.unwrap_or(""),
            viewport.unwrap_or("width=device-width, initial-scale=1"),
            background.map(|c| c.to_string()).unwrap_or_else(|| "white".to_string()),
            content
        )
    }

    fn partial_html(
        &self,
        headers: &BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
    ) -> String {
        content
    }
}
```

### Event Handling

```rust
use hyperchad_renderer::RendererEvent;

// Handle renderer events
match event {
    RendererEvent::View(view) => {
        // Handle full view update
    }
    RendererEvent::Partial(partial) => {
        // Handle partial content update
    }
    RendererEvent::Event { name, value } => {
        // Handle custom event
        println!("Event: {} = {:?}", name, value);
    }
    #[cfg(feature = "canvas")]
    RendererEvent::CanvasUpdate(update) => {
        // Handle canvas update
    }
}
```

### Canvas Support (with `canvas` feature)

```rust
#[cfg(feature = "canvas")]
use hyperchad_renderer::canvas::CanvasUpdate;

#[cfg(feature = "canvas")]
#[async_trait]
impl Renderer for MyRenderer {
    async fn render_canvas(&self, update: CanvasUpdate) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        // Handle canvas rendering
        Ok(())
    }
}
```

### Asset Management (with `assets` feature)

```rust
#[cfg(feature = "assets")]
use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};

#[cfg(feature = "assets")]
{
    let asset_route = StaticAssetRoute {
        route: "/static/style.css".to_string(),
        target: AssetPathTarget::File("path/to/style.css".into()),
    };
}
```

## Content Types

### View
- **immediate**: Content available immediately
- **future**: Content that will be available asynchronously
- **Container**: HyperChad container structure

### PartialView
- **target**: CSS selector for target element
- **container**: Content to insert/replace

### Content Enum
- **View**: Full page content
- **PartialView**: Partial content update
- **Raw**: Raw data with content type
- **Json**: JSON response (with `json` feature)

## Traits

### Renderer
Core rendering interface with initialization, rendering, and event handling.

### RenderRunner
Application execution interface for running renderer instances.

### ToRenderRunner
Conversion trait for creating runner instances from renderers.

### HtmlTagRenderer
HTML-specific rendering with CSS generation and document structure.

## Feature Flags

### Default Features
- **`assets`**: Static asset management
- **`canvas`**: Canvas rendering capabilities
- **`html`**: HTML rendering support
- **`viewport`**: Viewport utilities
- **`viewport-immediate`**: Immediate mode viewport rendering
- **`viewport-retained`**: Retained mode viewport rendering

### Optional Features
- **`json`**: JSON content support
- **`logic`**: Logic components support
- **`serde`**: Serialization/deserialization support
- **`profiling-puffin`**: Puffin profiler integration
- **`profiling-tracing`**: Tracing profiler integration
- **`profiling-tracy`**: Tracy profiler integration
- **`benchmark`**: Benchmarking utilities
- **`fail-on-warnings`**: Treat warnings as errors

## Dependencies

Core dependencies:
- **hyperchad_transformer**: UI transformation and container system (with `html` feature)
- **hyperchad_color**: Color handling and conversion
- **switchy_async**: Async runtime abstraction (with `rt-multi-thread` and `tokio` features)
- **async-trait**: Async trait support
- **bytes**: Byte buffer utilities
- **log**: Logging support

Optional dependencies:
- **serde**: Serialization framework (with `serde` feature)
- **serde_json**: JSON serialization (with `json` feature)

## Integration

This package is designed for:
- **Rendering Backends**: Implementation base for different renderers
- **UI Frameworks**: Core rendering abstractions
- **Web Applications**: HTML and CSS generation
- **Desktop Applications**: Native rendering interfaces
- **Static Generation**: Static site and asset generation
