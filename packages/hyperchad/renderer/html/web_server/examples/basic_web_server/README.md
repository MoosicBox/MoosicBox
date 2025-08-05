# HyperChad Basic Web Server Example

This example demonstrates a complete web application built with the HyperChad framework, showcasing server-side rendering with the web server backend.

## Features

* **Server-side rendering** with type-safe HTML generation using the `container!` macro
* **Component-based architecture** with reusable page functions
* **Built-in routing** for multiple pages and API endpoints
* **Static asset serving** for CSS and JavaScript files
* **JSON API endpoints** for dynamic functionality
* **Modern responsive design** with proper HTML structure

## Project Structure

```
basic_web_server/
├── src/
│   └── main.rs          # Main application with routes and components
├── Cargo.toml           # Dependencies and metadata
└── README.md            # This file
```

## Routes

* **GET /** - Home page with welcome message and features
* **GET /about** - About page with framework information
* **GET /contact** - Contact page with a form
* **GET /api/status** - API endpoint returning server status as JSON

## Running the Example

From the MoosicBox root directory:

```bash
# Build and run
nix-shell --run "cd packages/hyperchad/renderer/html/web_server/examples/basic_web_server && cargo run"

# Or just build
nix-shell --run "cd packages/hyperchad/renderer/html/web_server/examples/basic_web_server && cargo build"
```

The server will start on `http://localhost:8343` by default.

## Key Concepts Demonstrated

### 1. Component Architecture

The application uses a component-based approach with reusable functions:

```rust
fn create_home_page() -> Container {
    container! {
        div class="page" {
            header class="header" {
                // Navigation and content
            }
            main class="main" {
                // Page content
            }
            footer class="footer" {
                // Footer content
            }
        }
    }.into()
}
```

### 2. Type-Safe HTML Generation

HTML is generated using the `container!` macro with compile-time safety:

```rust
container! {
    div class="container" {
        h1 { "Welcome!" }
        anchor href="/about" { "Learn More" }
    }
}.into()
```

### 3. Routing and Handlers

Routes are defined with async handlers that return Containers:

```rust
router.add_route_result("/", |_req: RouteRequest| async move {
    Ok(create_home_page())
});
```

### 4. JSON API Endpoints

API endpoints return JSON data using the `Content::Raw` type:

```rust
router.add_route_result("/api/status", |_req: RouteRequest| async move {
    let response = json!({
        "status": "ok",
        "message": "Server is running!"
    });
    Ok(Content::Raw {
        data: response.to_string().into(),
        content_type: "application/json".to_string(),
    })
});
```

### 5. Web Server Integration

The application uses the HyperChad web server backend:

```rust
let app = router_to_web_server(DefaultHtmlTagRenderer::default(), router)
    .with_title(Some("HyperChad Web Server Example".to_string()))
    .with_description(Some("A modern web application built with HyperChad".to_string()));
```

## Technology Stack

* **HyperChad** - Web framework with type-safe HTML generation
* **Tokio** - Async runtime for high-performance I/O
* **Serde JSON** - JSON serialization for API responses
* **moosicbox_web_server** - Underlying HTTP server integration

## Supported Elements

The `container!` macro supports these HTML elements:
- `div`, `section`, `aside`, `main`, `header`, `footer`
- `form`, `span`, `button`, `anchor`, `image`, `input`
- `h1`, `h2`, `h3`, `h4`, `h5`, `h6`
- `ul`, `ol`, `li`
- `table`, `thead`, `th`, `tbody`, `tr`, `td`
- `canvas`

## Next Steps

This example provides a solid foundation for building larger applications. Consider exploring:

* Database integration for persistent data
* User authentication and sessions
* WebSocket support for real-time features
* More complex form handling and validation
* Middleware for logging, CORS, etc.
* Static asset optimization and caching
