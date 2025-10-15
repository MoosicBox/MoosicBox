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
use hyperchad::app::AppBuilder;
use hyperchad::router::{RouteRequest, Router};
use hyperchad::template::container;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let router = Router::new()
        .with_route_result("/", |_req: RouteRequest| async move {
            let content = container! {
                div {
                    h1 { "Welcome to HyperChad" }
                    button { "Click Me" }
                }
            };
            Ok(content.into())
        });

    let app = AppBuilder::new()
        .with_title("My App".to_string())
        .with_router(router)
        .build_default()?;

    app.run().await?;
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
use hyperchad::router::{Router, RouteRequest};
use hyperchad::template::container;

let router = Router::new()
    .with_route_result("/", |_req: RouteRequest| async move {
        let content = container! {
            div { "Home Page" }
        };
        Ok(content.into())
    })
    .with_route_result("/about", |_req: RouteRequest| async move {
        let content = container! {
            div { "About Page" }
        };
        Ok(content.into())
    })
    .with_route_result("/user/:id", |req: RouteRequest| async move {
        let user_id = req.path.strip_prefix("/user/").unwrap_or("");
        let content = container! {
            div {
                h1 { "User Profile" }
                p { format!("User ID: {}", user_id) }
            }
        };
        Ok(content.into())
    });
```

### State Management

HyperChad provides a simple key-value state store:

```rust
use hyperchad::state::{StateStore, InMemoryStatePersistence};
use serde_json::json;

let state = StateStore::new(InMemoryStatePersistence::new());

// Set values
state.set("user_id", &json!("12345")).await?;
state.set("theme", &json!("dark")).await?;

// Get values
if let Some(user_id) = state.get::<serde_json::Value>("user_id").await? {
    println!("User ID: {}", user_id);
}

// With SQLite persistence (requires "state-sqlite" feature)
#[cfg(feature = "state-sqlite")]
{
    use hyperchad::state::SqliteStatePersistence;
    let state = StateStore::new(SqliteStatePersistence::new("app.db").await?);
}
```

### Action System

```rust
use hyperchad::actions::{Action, ActionContext};

// Define actions in templates
let ui = container! {
    div {
        button fx-click=fx {
            set_value("counter", "5")
            navigate("/success")
        } {
            "Submit"
        }
    }
};

// Actions support conditionals and logic (with "actions-logic" feature)
#[cfg(feature = "actions-logic")]
{
    let ui = container! {
        button fx-click=fx {
            if eq(get_value("status"), "active") {
                navigate("/dashboard")
            } else {
                navigate("/login")
            }
        } {
            "Continue"
        }
    };
}
```

## Programming Interface

### Core Types

```rust
// Container - the fundamental building block
pub struct Container {
    pub id: usize,
    pub str_id: Option<String>,
    pub classes: Vec<String>,
    pub element: Element,
    pub children: Vec<Container>,
    pub direction: LayoutDirection,
    pub overflow_x: LayoutOverflow,
    pub overflow_y: LayoutOverflow,
    pub width: Option<Number>,
    pub height: Option<Number>,
    pub background: Option<Color>,
    pub padding_left: Option<Number>,
    pub padding_right: Option<Number>,
    pub padding_top: Option<Number>,
    pub padding_bottom: Option<Number>,
    pub margin_left: Option<Number>,
    pub margin_right: Option<Number>,
    pub margin_top: Option<Number>,
    pub margin_bottom: Option<Number>,
    pub font_size: Option<Number>,
    pub color: Option<Color>,
    pub actions: Vec<Action>,
    // ... many other styling and layout fields
}

// Element - defines HTML/UI element types
pub enum Element {
    Div,
    Span,
    Button { r#type: Option<String> },
    Input { input: Input, name: Option<String>, autofocus: Option<bool> },
    Heading { size: HeaderSize },
    Image {
        source: Option<String>,
        alt: Option<String>,
        fit: Option<ImageFit>,
        source_set: Option<String>,
        sizes: Option<Number>,
        loading: Option<ImageLoading>,
    },
    Anchor { target: Option<LinkTarget>, href: Option<String> },
    Raw { value: String },
    Aside,
    Main,
    Header,
    Footer,
    Section,
    Form,
    UnorderedList,
    OrderedList,
    ListItem,
    Table,
    THead,
    TH,
    TBody,
    TR,
    TD,
    #[cfg(feature = "canvas")]
    Canvas,
}
```

### Renderer Trait

```rust
use async_trait::async_trait;

#[async_trait]
pub trait Renderer: ToRenderRunner + Send + Sync {
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
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger);

    // Note: render, render_partial, and emit_event methods are implemented
    // via the ToRenderRunner trait and RenderRunner
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
    pub fn with_width(self, width: f32) -> Self;
    pub fn with_height(self, height: f32) -> Self;
    pub fn with_background(self, color: Color) -> Self;

    // Build methods for different renderers
    pub fn build<R>(self, renderer: R) -> Result<App<R>, BuilderError>;
    pub fn build_default(self) -> Result<App<DefaultRenderer>, BuilderError>;
    pub fn build_default_egui(self) -> Result<App<EguiRenderer>, BuilderError>;
    pub fn build_default_html(self) -> Result<App<HtmlStubRenderer>, BuilderError>;
    // ... other renderer-specific build methods
}

pub struct App<R: Renderer + /* ... */> {
    pub renderer: R,
    pub router: Router,
    // ...
}

impl<R: Renderer + /* ... */> App<R> {
    pub async fn run(&self) -> Result<(), Error>;
    pub async fn serve(&self) -> Result<(), Error>;
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
debug = ["renderer-egui-debug", "renderer-fltk-debug"]
```

## Renderer-Specific Usage

### Egui Desktop Application

```rust
use hyperchad::app::AppBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = AppBuilder::new()
        .with_title("Desktop App".to_string())
        .with_router(router)
        .build_default_egui()?;

    app.run().await?;
    Ok(())
}
```

### Web Application with Actix

```rust
use hyperchad::app::AppBuilder;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = AppBuilder::new()
        .with_router(router)
        .build_default_html_actix()
        .unwrap();

    app.serve().await.unwrap();
    Ok(())
}
```

### Server-Side Rendering with HTML

```rust
use hyperchad::renderer_html::{HtmlRenderer, DefaultHtmlTagRenderer, router_to_web_server};
use hyperchad::router::Router;

let router = Router::new();
// Add routes...

let app = router_to_web_server(DefaultHtmlTagRenderer::default(), router)
    .with_title(Some("My App".to_string()));

// Create runner and serve
let runtime = switchy::unsync::runtime::Runtime::new();
let handle = runtime.handle();
let mut runner = app.to_runner(handle)?;
runner.run()?;
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
#[cfg(feature = "serde")]
use hyperchad::router::ParseError;

// App errors
match app.run().await {
    Ok(()) => println!("App completed successfully"),
    Err(e) => eprintln!("Error: {}", e),
}

// State errors
match state.get::<String>("key").await {
    Ok(Some(value)) => println!("Value: {}", value),
    Ok(None) => println!("Key not found"),
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
