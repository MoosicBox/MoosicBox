# HyperChad

A template-based UI framework for building cross-platform applications with a unified codebase. HyperChad enables developers to write UI templates once and deploy across desktop (Egui, FLTK), web (HTML, Vanilla JS), and server-side (Actix, Lambda) environments.

## Features

- **Multi-Renderer Architecture**: Support for Egui, FLTK, HTML, Vanilla JS, and server-side rendering
- **Template-Based UI**: Build interfaces using the `container!` macro system
- **Routing System**: Async router with navigation support
- **Action System**: Event handling and data flow management
- **State Persistence**: Key-value state store with optional SQLite persistence
- **Color Management**: Consistent theming across all renderers
- **JavaScript Bundling**: Automatic bundling for web deployments
- **Responsive Design**: Responsive triggers for adaptive layouts
- **Cross-Platform**: Desktop, web, and server applications from single codebase

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad = "0.1.0"
```

## Usage

### Basic Application

```rust
use hyperchad::app::{App, AppBuilder};
use hyperchad::router::{Router, RoutePath, RouteRequest};
use hyperchad::template::container;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let router = Router::new()
        .with_route("/", |_req: RouteRequest| async move {
            container! {
                div {
                    h1 { "Welcome to HyperChad" }
                    button { "Click Me" }
                }
            }
        });

    let app = AppBuilder::new()
        .with_title("My App".to_string())
        .with_router(router)
        .build_default()?;

    app.run()?;
    Ok(())
}
```

### Template System

HyperChad uses a template macro system for building UIs:

```rust
use hyperchad::template::container;

let ui = container! {
    div id="user-card" class="card" {
        h2 { "User Profile" }
        p { "Email: user@example.com" }
        button fx-click=fx { navigate("/profile") } {
            "View Profile"
        }
    }
};
```

### Routing

```rust
use hyperchad::router::{Router, RoutePath, RouteRequest};

let router = Router::new()
    .with_route("/", |_req| async move {
        container! {
            div { "Home Page" }
        }
    })
    .with_route("/about", |_req| async move {
        container! {
            div { "About Page" }
        }
    })
    .with_route(RoutePath::LiteralPrefix("/user/".to_string()), |req| async move {
        let user_id = req.path.strip_prefix("/user/").unwrap_or("");
        container! {
            div {
                h1 { "User Profile" }
                p { format!("User ID: {}", user_id) }
            }
        }
    });
```

### State Management

HyperChad provides a simple key-value state store:

```rust
use hyperchad::state::{StateStore, sqlite::SqlitePersistence};
use serde_json::json;

let state = StateStore::new(SqlitePersistence::new_in_memory().await?);

// Set values
state.set("user_id", &json!("12345")).await?;
state.set("theme", &json!("dark")).await?;

// Get values
if let Some(user_id) = state.get::<serde_json::Value>("user_id").await? {
    println!("User ID: {}", user_id);
}

// With SQLite file persistence (requires "state-sqlite" feature)
#[cfg(feature = "state-sqlite")]
{
    use hyperchad::state::sqlite::SqlitePersistence;
    let state = StateStore::new(SqlitePersistence::new("app.db").await?);
}
```

### Action System

```rust
// Define actions in templates using the fx DSL
let ui = container! {
    div {
        button fx-click=fx { show("modal") } {
            "Open Modal"
        }
        button fx-click=fx { navigate("/success") } {
            "Submit"
        }
    }
};

// Actions support conditionals and logic (with "actions-logic" feature)
#[cfg(feature = "actions-logic")]
let ui = container! {
    button fx-click=fx {
        if get_visibility("panel") == visible() {
            hide("panel")
        } else {
            show("panel")
        }
    } {
        "Toggle Panel"
    }
};
```

## Programming Interface

### Core Types

```rust
// Container - the fundamental building block
pub struct Container {
    pub element: Element,
    pub children: Vec<Container>,
    // ... styling and layout fields
}

// Element - defines HTML/UI element types
pub enum Element {
    Div,
    Span,
    Button { r#type: Option<String> },
    Input { input: Input, name: Option<String>, autofocus: Option<bool> },
    Heading { size: HeaderSize },
    Image { source: Option<String>, alt: Option<String>, /* ... */ },
    Anchor { target: Option<LinkTarget>, href: Option<String> },
    // ... other variants
}
```

### Renderer Trait

```rust
use async_trait::async_trait;

#[async_trait]
pub trait Renderer: ToRenderRunner + Send + Sync {
    async fn init(&mut self, width: f32, height: f32, /* ... */)
        -> Result<(), Box<dyn std::error::Error>>;

    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error>>;

    async fn emit_event(&self, event_name: String, event_value: Option<String>)
        -> Result<(), Box<dyn std::error::Error>>;

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger);
}
```

### Application Builder

```rust
pub struct AppBuilder {
    // ...
}

impl AppBuilder {
    pub fn new() -> Self;
    pub fn with_title(self, title: String) -> Self;
    pub fn with_router(self, router: Router) -> Self;
    pub fn build<R: Renderer + /* ... */ + 'static>(self, renderer: R) -> Result<App<R>, BuilderError>;
}

pub struct App<R: Renderer + /* ... */> {
    pub renderer: R,
    pub router: Router,
    // ...
}

impl<R: Renderer + /* ... */> App<R> {
    pub fn run(self) -> Result<(), Error>;
    pub async fn serve(&mut self) -> Result<Box<dyn RenderRunner>, Error>;
    pub async fn generate(&self, output: Option<String>) -> Result<(), Error>;
    pub async fn clean(&self, output: Option<String>) -> Result<(), Error>;
}
```

## Configuration

### Feature Flags

```toml
[features]
default = ["all"]

# Renderer features
renderer-egui = ["hyperchad_renderer_egui"]
renderer-fltk = ["hyperchad_renderer_fltk"]
renderer-html = ["hyperchad_renderer_html"]
renderer-vanilla-js = ["hyperchad_renderer_vanilla_js"]

# Platform features
renderer-html-actix = ["hyperchad_renderer_html_actix"]
renderer-html-lambda = ["hyperchad_renderer_html_lambda"]
renderer-html-web-server = ["hyperchad_renderer_html_web_server"]

# State features
state = ["hyperchad_state"]
state-sqlite = ["hyperchad_state/persistence-sqlite"]

# Development features
actions-logic = ["hyperchad_actions/logic"]
renderer-egui-debug = ["hyperchad_renderer_egui/debug"]
debug = ["hyperchad_app/debug", "renderer-egui-debug", "renderer-fltk-debug"]
```

## Renderer-Specific Usage

HyperChad supports multiple renderers through the `AppBuilder`. Use `build_default()` to automatically select the appropriate renderer based on enabled features, or use renderer-specific build methods:

```rust
use hyperchad::app::AppBuilder;

// Build with default renderer (based on features)
let app = AppBuilder::new()
    .with_title("My App".to_string())
    .with_router(router)
    .build_default()?;

// Or use renderer-specific methods when available:
// - build_egui(renderer) for Egui desktop apps
// - build_fltk(renderer) for FLTK desktop apps
// - build_html(renderer) for HTML rendering
// See the renderer module documentation for specific usage
```

## Testing

```bash
# Run all tests
cargo test

# Test specific renderer
cargo test --features "renderer-egui"

# Test with all renderers
cargo test --features "all"
```

## Error Handling

Each module provides its own error types:

```rust
use hyperchad::app::{Error as AppError, BuilderError};
use hyperchad::state::Error as StateError;
use hyperchad::router::ParseError;

// App errors
match app.run() {
    Ok(()) => println!("App completed successfully"),
    Err(AppError::IO(e)) => eprintln!("IO error: {}", e),
    Err(AppError::Builder(e)) => eprintln!("Builder error: {}", e),
    Err(e) => eprintln!("Error: {}", e),
}

// State errors
match state.get::<serde_json::Value>("key").await {
    Ok(Some(value)) => println!("Value: {}", value),
    Ok(None) => println!("Key not found"),
    Err(StateError::Serde(e)) => eprintln!("Serialization error: {}", e),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Architecture

HyperChad is built on these core concepts:

1. **Templates**: Use the `container!` macro to define UI structure
2. **Containers**: Styled elements that form the UI tree
3. **Renderers**: Transform containers into platform-specific output
4. **Router**: Maps paths to content generators (async closures)
5. **Actions**: Handle events and trigger state changes or navigation
6. **State**: Optional key-value persistence layer

## See Also

- [`hyperchad_app`] - Application builder and runtime
- [`hyperchad_renderer`] - Base renderer trait
- [`hyperchad_renderer_egui`] - Egui desktop renderer implementation
- [`hyperchad_renderer_fltk`] - FLTK desktop renderer implementation
- [`hyperchad_renderer_html`] - HTML server-side renderer
- [`hyperchad_renderer_vanilla_js`] - Client-side JavaScript renderer
- [`hyperchad_state`] - State persistence system
- [`hyperchad_router`] - Routing functionality
- [`hyperchad_template`] - Template macro system
- [`hyperchad_actions`] - Action system for events
- [`hyperchad_color`] - Color management
- [`hyperchad_transformer`] - Container and element types
