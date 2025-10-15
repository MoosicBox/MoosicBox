# HyperChad Basic Web Server Example

This example demonstrates a complete web application built with the HyperChad framework, showcasing server-side rendering with the web server backend.

## Features

- **Server-side rendering** with type-safe HTML generation using the `container!` macro
- **Component-based architecture** with reusable page functions
- **Built-in routing** for multiple pages and API endpoints
- **JSON API endpoints** for dynamic functionality
- **Modern HTML structure** with semantic elements

## Project Structure

```
basic_web_server/
├── src/
│   └── main.rs          # Main application with routes and components
├── Cargo.toml           # Dependencies and metadata
└── README.md            # This file
```

## Routes

- **GET /** - Home page with welcome message and features
- **GET /about** - About page with framework information
- **GET /contact** - Contact page with a form
- **GET /api/status** - API endpoint returning server status as JSON

## Running the Example

From the MoosicBox root directory:

```bash
# Build and run
nix develop .#fltk-hyperchad --command bash -c "cd packages/hyperchad/renderer/html/web_server/examples/basic_web_server && cargo run"

# Or just build
nix develop .#fltk-hyperchad --command bash -c "cd packages/hyperchad/renderer/html/web_server/examples/basic_web_server && cargo build"
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

- **HyperChad** - Web framework with type-safe HTML generation
- **Switchy** - Async runtime abstraction for high-performance I/O
- **Serde JSON** - JSON serialization for API responses
- **Env Logger** - Logging infrastructure

## Elements Used in This Example

This example demonstrates the following HTML elements with the `container!` macro:

- **Layout**: `div`, `section`, `main`, `header`, `footer`
- **Text**: `h1`, `h2`, `h3`, `span`
- **Forms**: `form`, `input`, `button`
- **Navigation**: `anchor`
- **Lists**: `ul`, `li`

Note: The HyperChad framework supports additional elements beyond those demonstrated here.

## Potential Enhancements

This example provides a solid foundation for building larger applications. Consider exploring:

- **Planned/Future**: Database integration for persistent data
- **Planned/Future**: User authentication and sessions
- **Planned/Future**: WebSocket support for real-time features
- **Planned/Future**: More complex form handling and validation
- **Planned/Future**: Middleware for logging, CORS, etc.
- **Planned/Future**: Static asset optimization and caching
