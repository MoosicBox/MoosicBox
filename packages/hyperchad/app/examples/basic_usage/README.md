# Basic Usage Example

This example demonstrates the fundamental setup and usage of the `hyperchad_app` framework for building web applications with routing, static assets, and server-side rendering.

## Summary

A comprehensive example showing how to create a basic HyperChad application with multiple routes, static asset serving, and a web server backend using Actix.

## What This Example Demonstrates

- Creating a `Router` with multiple page routes
- Building an application using `AppBuilder` with configuration options
- Serving static assets (JavaScript files)
- Setting up window/page properties (title, description, size, background)
- Using the Actix web server backend with vanilla JavaScript
- Creating multiple pages with different layouts and styles
- Navigation between routes using hyperlinks

## Prerequisites

- Basic understanding of Rust and async programming
- Familiarity with web concepts (routing, HTML, HTTP servers)
- Understanding of the HyperChad template syntax (optional but helpful)

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- serve
```

Then open your browser to: **http://localhost:8080**

By default, the server runs on port 8080. You can change this by setting the `PORT` environment variable:

```bash
PORT=3000 cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- serve
```

## Expected Output

When you run the example, you should see console output similar to:

```
Starting HyperChad App Basic Usage Example
Server running on http://localhost:8080
Available routes:
  - http://localhost:8080/
  - http://localhost:8080/about
  - http://localhost:8080/demo
Press Ctrl+C to stop
```

The browser will display:

1. **Home page (/)**: A landing page with introduction, features list, and navigation links
2. **About page (/about)**: Information about the example and architecture
3. **Demo page (/demo)**: Interactive demonstration with styled sections and layout examples

## Code Walkthrough

### 1. Router Setup

The router defines all available routes and their handlers:

```rust
fn create_router() -> Router {
    Router::new()
        .with_route("/", |_req: RouteRequest| async move {
            View::builder().with_primary(create_home_page()).build()
        })
        .with_route("/about", |_req: RouteRequest| async move {
            View::builder().with_primary(create_about_page()).build()
        })
        .with_route("/demo", |_req: RouteRequest| async move {
            View::builder().with_primary(create_demo_page()).build()
        })
}
```

Each route is associated with an async handler that returns a `View`.

### 2. Creating Views

Views are built using the HyperChad template syntax with the `container!` macro:

```rust
fn create_home_page() -> Containers {
    container! {
        div class="page" {
            header padding=24 background=#2563eb color=white {
                h1 { "Welcome to HyperChad App" }
            }
            main padding=24 {
                section { /* content */ }
            }
        }
    }
}
```

The template syntax provides a clean, declarative way to define HTML structure with inline styling.

### 3. AppBuilder Configuration

The `AppBuilder` fluently configures the application:

```rust
let mut app = AppBuilder::new()
    .with_router(router)                    // Set the router
    .with_runtime_handle(runtime.handle())  // Provide async runtime
    .with_title("HyperChad App".to_string()) // Page title
    .with_description("Example app".to_string()) // Meta description
    .with_size(1024.0, 768.0)               // Window size
    .with_background(Color::from_hex("#f9fafb")); // Background color
```

### 4. Static Assets

Static assets like JavaScript files are registered with the app:

```rust
static ASSETS: LazyLock<Vec<StaticAssetRoute>> = LazyLock::new(|| {
    vec![StaticAssetRoute {
        route: format!("js/{}", SCRIPT_NAME_HASHED.as_str()),
        target: AssetPathTarget::FileContents(SCRIPT.as_bytes().into()),
    }]
});

// Add assets to the app
for asset in ASSETS.iter().cloned() {
    app.static_asset_route_result(asset)?;
}
```

### 5. Building and Running

Finally, build the app with the default renderer and run it:

```rust
app.build_default()?.run()?;
```

The `build_default()` method automatically selects the appropriate renderer based on enabled features. With the `actix` and `vanilla-js` features enabled, it uses the Actix web server with vanilla JavaScript interactivity.

## Key Concepts

### Router

The `Router` maps URL paths to async handler functions. Each handler returns a `View` containing the page content. Routes are registered using `.with_route(path, handler)`.

### View and Containers

A `View` represents the complete page structure. The `primary` field contains the main content as `Containers`, which are built using the `container!` macro with HyperChad's template syntax.

### AppBuilder Pattern

The builder pattern provides a fluent API for configuring the application. Methods prefixed with `with_` return `self`, allowing method chaining. Configuration includes:

- **Router**: Required - defines application routes
- **Runtime**: Async runtime for handling requests
- **Metadata**: Title, description, viewport settings
- **Window**: Size, position, background color
- **Assets**: Static files to serve

### Renderer Selection

The `build_default()` method selects the renderer based on feature flags:

- `egui` → Native desktop UI with egui
- `fltk` → Native desktop UI with fltk
- `actix` + `vanilla-js` → Web server with Actix and vanilla JavaScript
- `lambda` + `vanilla-js` → Serverless deployment with AWS Lambda
- Other combinations → Various HTML-based renderers

### HyperChad Template Syntax

The template syntax provides an ergonomic way to define UI:

```rust
container! {
    div class="example" padding=16 background=#fff {
        h1 { "Title" }
        p { "Paragraph text" }
        a href="/link" { "Link text" }
    }
}
```

Features include:

- Unquoted attribute values for colors, numbers, keywords
- Inline styling with CSS-like properties
- Type-safe HTML structure
- Compile-time validation

## Testing the Example

### Navigate Between Routes

Click the navigation links on each page to move between routes. The URL will update, and the page content will change accordingly.

### View Page Source

Right-click and select "View Page Source" to see the generated HTML. Notice:

- The `<title>` tag contains the configured title
- The `<meta>` description tag is present
- The vanilla JavaScript file is included
- The HTML structure matches the template definitions

### Check Static Assets

Visit `http://localhost:8080/js/{hash}.js` (the actual hash will be logged in console) to see the served JavaScript file.

### Test Responsive Layout

Resize your browser window to see how the layout adapts. The `max-width` and responsive attributes ensure proper display on different screen sizes.

## Troubleshooting

### Port Already in Use

If port 8080 is already in use, set a different port:

```bash
PORT=3001 cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- serve
```

### Build Errors

Ensure all workspace dependencies are available. From the repository root:

```bash
cargo build --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml
```

### Runtime Errors

Check the console output for error messages. Common issues:

- Missing router configuration
- Invalid asset paths
- Runtime not properly initialized

### Page Not Loading

Verify the server started successfully by checking for the "Server running" message in the console. If the page doesn't load:

1. Check that the URL matches the logged server address
2. Verify no firewall is blocking the connection
3. Check for errors in the browser console

## Related Examples

- **packages/hyperchad/examples/details_summary/** - Demonstrates collapsible UI elements with details/summary tags
- **packages/web_server/examples/simple_get/** - Basic web server example with Actix
- **packages/hyperchad/router/examples/** - Router-specific examples (if available)

## Additional Features

This example can be extended to demonstrate:

- **Action Handlers**: Add `.with_action_handler()` to process user interactions
- **Resize Listeners**: Use `.with_on_resize()` to respond to window size changes
- **Custom CSS**: Add `.with_css_url()` or `.with_inline_css()` for styling
- **Form Handling**: Add routes that process POST requests
- **Dynamic Content**: Use route parameters and query strings
- **Static Site Generation**: Use the `gen` CLI command to pre-render pages

## CLI Commands

The hyperchad_app framework provides several CLI commands:

### Serve (Development)

Start the development server:

```bash
cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- serve
```

### Generate Static Site

Pre-render all routes to static HTML files:

```bash
cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- gen --output ./dist
```

### Clean Generated Files

Remove generated static files:

```bash
cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- clean --output ./dist
```

### List Dynamic Routes

Display all registered routes:

```bash
cargo run --manifest-path packages/hyperchad/app/examples/basic_usage/Cargo.toml -- dynamic-routes
```

## Performance Notes

- The Actix web server is highly performant and production-ready
- Static assets are served efficiently from memory
- Routes are resolved using a fast lookup mechanism
- The vanilla JavaScript client provides responsive UI updates
- Pre-rendering with the `gen` command produces optimized static HTML

## Security Considerations

This is a basic example intended for learning. For production applications:

- Enable HTTPS/TLS encryption
- Implement proper authentication and authorization
- Validate and sanitize user input
- Set appropriate security headers
- Follow security best practices for web applications
