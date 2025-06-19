# HyperChad

A versatile, multi-renderer UI framework for building cross-platform applications with a unified codebase. HyperChad enables developers to write UI logic once and deploy across desktop (Egui, FLTK), web (HTML, Vanilla JS), and server-side (Actix, Lambda) environments.

## Features

- **Multi-Renderer Architecture**: Support for Egui, FLTK, HTML, Vanilla JS, and server-side rendering
- **Unified Component System**: Write components once, render everywhere
- **State Management**: Reactive state system with persistence options
- **Routing System**: Client-side and server-side routing with navigation
- **Template Engine**: Flexible templating with logic support
- **Action System**: Event handling and data flow management
- **Color Management**: Consistent theming across all renderers
- **JavaScript Bundling**: Automatic bundling for web deployments
- **Hot Reload**: Development-time hot reloading for rapid iteration
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
use hyperchad::{App, Component, Renderer, Element};

#[derive(Debug, Clone)]
struct Counter {
    count: i32,
}

impl Component for Counter {
    type Props = ();
    type State = i32;

    fn render(&self, props: &Self::Props, state: &Self::State) -> Element {
        Element::div()
            .child(Element::text(format!("Count: {}", state)))
            .child(
                Element::button()
                    .text("Increment")
                    .on_click(|_| CounterAction::Increment)
            )
            .child(
                Element::button()
                    .text("Decrement")
                    .on_click(|_| CounterAction::Decrement)
            )
    }
}

#[derive(Debug, Clone)]
enum CounterAction {
    Increment,
    Decrement,
}

// Run with different renderers
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new()
        .with_component(Counter { count: 0 })
        .with_title("Counter App");

    #[cfg(feature = "renderer-egui")]
    app.run_egui()?;

    #[cfg(feature = "renderer-html")]
    app.run_html_server("0.0.0.0:8080")?;

    Ok(())
}
```

### Component System

```rust
use hyperchad::{Component, Element, Props, State};

#[derive(Debug, Clone, Props)]
struct ButtonProps {
    text: String,
    variant: ButtonVariant,
    disabled: bool,
}

#[derive(Debug, Clone)]
enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
}

#[derive(Debug, Clone)]
struct Button;

impl Component for Button {
    type Props = ButtonProps;
    type State = bool; // hover state

    fn render(&self, props: &Self::Props, state: &Self::State) -> Element {
        Element::button()
            .text(&props.text)
            .class(match props.variant {
                ButtonVariant::Primary => "btn-primary",
                ButtonVariant::Secondary => "btn-secondary",
                ButtonVariant::Danger => "btn-danger",
            })
            .disabled(props.disabled)
            .class_if("btn-hover", *state)
            .on_mouse_enter(|_| ButtonAction::Hover(true))
            .on_mouse_leave(|_| ButtonAction::Hover(false))
    }
}
```

### State Management

```rust
use hyperchad::{State, StateManager, Reducer};

#[derive(Debug, Clone, State)]
struct AppState {
    user: Option<User>,
    todos: Vec<Todo>,
    loading: bool,
}

#[derive(Debug, Clone)]
enum AppAction {
    Login(User),
    Logout,
    AddTodo(String),
    ToggleTodo(usize),
    SetLoading(bool),
}

impl Reducer<AppAction> for AppState {
    fn reduce(&mut self, action: AppAction) {
        match action {
            AppAction::Login(user) => {
                self.user = Some(user);
                self.loading = false;
            }
            AppAction::Logout => {
                self.user = None;
                self.todos.clear();
            }
            AppAction::AddTodo(text) => {
                self.todos.push(Todo::new(text));
            }
            AppAction::ToggleTodo(index) => {
                if let Some(todo) = self.todos.get_mut(index) {
                    todo.completed = !todo.completed;
                }
            }
            AppAction::SetLoading(loading) => {
                self.loading = loading;
            }
        }
    }
}

// Use state in components
let state_manager = StateManager::new(AppState::default());
let app = App::new().with_state(state_manager);
```

### Routing

```rust
use hyperchad::{Router, Route, Navigate};

#[derive(Debug, Clone)]
enum AppRoute {
    Home,
    About,
    User(u32),
    NotFound,
}

impl Route for AppRoute {
    fn from_path(path: &str) -> Self {
        match path {
            "/" => AppRoute::Home,
            "/about" => AppRoute::About,
            path if path.starts_with("/user/") => {
                let id = path.strip_prefix("/user/")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                AppRoute::User(id)
            }
            _ => AppRoute::NotFound,
        }
    }

    fn to_path(&self) -> String {
        match self {
            AppRoute::Home => "/".to_string(),
            AppRoute::About => "/about".to_string(),
            AppRoute::User(id) => format!("/user/{}", id),
            AppRoute::NotFound => "/404".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct App {
    router: Router<AppRoute>,
}

impl Component for App {
    type Props = ();
    type State = AppRoute;

    fn render(&self, _props: &Self::Props, route: &Self::State) -> Element {
        Element::div()
            .child(self.render_navbar())
            .child(match route {
                AppRoute::Home => self.render_home(),
                AppRoute::About => self.render_about(),
                AppRoute::User(id) => self.render_user(*id),
                AppRoute::NotFound => self.render_404(),
            })
    }
}
```

### Templates

```rust
use hyperchad::{Template, TemplateEngine, Context};

// Define template
let template = r#"
<div class="user-card">
    <h2>{{ user.name }}</h2>
    <p>{{ user.email }}</p>
    {% if user.is_admin %}
        <span class="badge">Admin</span>
    {% endif %}
    <ul>
    {% for skill in user.skills %}
        <li>{{ skill }}</li>
    {% endfor %}
    </ul>
</div>
"#;

// Create context
let context = Context::new()
    .insert("user", User {
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
        is_admin: true,
        skills: vec!["Rust".to_string(), "JavaScript".to_string()],
    });

// Render template
let engine = TemplateEngine::new();
let rendered = engine.render(template, &context)?;
```

## Programming Interface

### Core Types

```rust
pub trait Component: Clone + Send + Sync {
    type Props: Clone + Send + Sync;
    type State: Clone + Send + Sync;

    fn render(&self, props: &Self::Props, state: &Self::State) -> Element;
    fn update(&mut self, action: Self::Action) -> bool { false }
}

pub struct Element {
    tag: String,
    attributes: HashMap<String, String>,
    children: Vec<Element>,
    text: Option<String>,
    event_handlers: HashMap<String, EventHandler>,
}

impl Element {
    pub fn div() -> Self;
    pub fn span() -> Self;
    pub fn button() -> Self;
    pub fn input() -> Self;
    pub fn text<S: Into<String>>(content: S) -> Self;

    pub fn attr<K, V>(mut self, key: K, value: V) -> Self
    where K: Into<String>, V: Into<String>;

    pub fn class<S: Into<String>>(mut self, class: S) -> Self;
    pub fn class_if<S: Into<String>>(mut self, class: S, condition: bool) -> Self;
    pub fn child(mut self, child: Element) -> Self;
    pub fn children<I>(mut self, children: I) -> Self
    where I: IntoIterator<Item = Element>;
}
```

### Renderer Traits

```rust
pub trait Renderer: Send + Sync {
    type Error;
    type Output;

    fn render(&self, element: &Element) -> Result<Self::Output, Self::Error>;
    fn handle_event(&mut self, event: Event) -> Result<(), Self::Error>;
}

// Specific renderer implementations
pub struct EguiRenderer {
    ctx: egui::Context,
}

pub struct HtmlRenderer {
    output: String,
}

pub struct VanillaJsRenderer {
    dom: VirtualDom,
}
```

### State Management

```rust
pub trait State: Clone + Send + Sync + 'static {}

pub trait Reducer<A>: State {
    fn reduce(&mut self, action: A);
}

pub struct StateManager<S: State> {
    state: Arc<RwLock<S>>,
    subscribers: Vec<Box<dyn Fn(&S) + Send + Sync>>,
}

impl<S: State> StateManager<S> {
    pub fn new(initial_state: S) -> Self;
    pub fn get_state(&self) -> S;
    pub fn dispatch<A>(&self, action: A) where S: Reducer<A>;
    pub fn subscribe<F>(&mut self, callback: F) where F: Fn(&S) + Send + Sync + 'static;
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

# Development features
renderer-egui-debug = ["hyperchad_renderer_egui/debug"]
actions-logic = ["hyperchad_actions/logic"]
```

### Environment Variables

- `HYPERCHAD_RENDERER`: Default renderer to use (egui, html, fltk)
- `HYPERCHAD_DEV_MODE`: Enable development features (default: false)
- `HYPERCHAD_HOT_RELOAD`: Enable hot reloading (default: true in dev)
- `HYPERCHAD_BUNDLE_JS`: Enable JavaScript bundling (default: true)

## Renderer-Specific Usage

### Egui Desktop Application

```rust
use hyperchad::{App, EguiRenderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new()
        .with_title("My Desktop App")
        .with_size(800, 600);

    let renderer = EguiRenderer::new()
        .with_theme(Theme::Dark)
        .with_font("Inter", 14.0);

    app.run_with_renderer(renderer)?;
    Ok(())
}
```

### Web Application

```rust
use hyperchad::{App, HtmlRenderer, VanillaJsRenderer};

// Server-side rendering
fn render_ssr() -> String {
    let app = App::new();
    let renderer = HtmlRenderer::new();
    app.render_to_string(&renderer)
}

// Client-side hydration
#[wasm_bindgen]
pub fn hydrate() {
    let app = App::new();
    let renderer = VanillaJsRenderer::new();
    app.hydrate_dom(&renderer);
}
```

### Server Deployment

```rust
use hyperchad::{App, ActixRenderer};
use actix_web::{web, App as ActixApp, HttpServer};

async fn render_page(app: web::Data<App>) -> impl Responder {
    let renderer = ActixRenderer::new();
    app.render(&renderer).await
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app = App::new().build();

    HttpServer::new(move || {
        ActixApp::new()
            .app_data(web::Data::new(app.clone()))
            .route("/", web::get().to(render_page))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

## Development Tools

### Hot Reloading

```rust
use hyperchad::{DevServer, WatchOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev_server = DevServer::new()
        .with_watch_paths(vec!["src/", "templates/"])
        .with_reload_on_change(true)
        .with_port(3000);

    dev_server.start()?;
    Ok(())
}
```

### Component Inspector

```rust
use hyperchad::{Inspector, DebugMode};

let app = App::new()
    .with_debug_mode(DebugMode::Development)
    .with_inspector(Inspector::new()
        .show_component_tree(true)
        .show_state_changes(true)
        .show_performance_metrics(true)
    );
```

## Testing

```bash
# Run all tests
cargo test

# Test specific renderer
cargo test --features "renderer-egui"

# Run integration tests
cargo test --test integration

# Test with all renderers
cargo test --features "all"
```

## Performance Optimization

### Virtual DOM Optimization

```rust
use hyperchad::{VirtualDom, DiffOptions};

let vdom = VirtualDom::new()
    .with_diff_options(DiffOptions {
        skip_text_nodes: false,
        batch_updates: true,
        debounce_ms: 16, // 60fps
    });
```

### State Updates

```rust
// Batch state updates for better performance
state_manager.batch_dispatch(vec![
    AppAction::SetLoading(true),
    AppAction::ClearData,
    AppAction::LoadData(data),
    AppAction::SetLoading(false),
]);

// Use memoization for expensive computations
let memoized_component = Component::memo(ExpensiveComponent, |prev, next| {
    prev.data_id == next.data_id
});
```

## Error Handling

```rust
use hyperchad::{HyperChadError, RenderError};

match app.render(&renderer) {
    Ok(output) => println!("Rendered successfully"),
    Err(HyperChadError::RenderError(e)) => {
        eprintln!("Render error: {}", e);
    }
    Err(HyperChadError::StateError(e)) => {
        eprintln!("State management error: {}", e);
    }
    Err(HyperChadError::ComponentError(e)) => {
        eprintln!("Component error: {}", e);
    }
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

## See Also

- [`hyperchad_renderer_egui`] - Egui desktop renderer implementation
- [`hyperchad_renderer_html`] - HTML server-side renderer
- [`hyperchad_renderer_vanilla_js`] - Client-side JavaScript renderer
- [`hyperchad_state`] - State management system
- [`hyperchad_router`] - Routing functionality
- [`hyperchad_template`] - Template engine
