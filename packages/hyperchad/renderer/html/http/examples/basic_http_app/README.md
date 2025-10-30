# Basic HTTP Application Example

A complete example demonstrating how to build a web application using HyperChad's framework-agnostic HTTP adapter.

## Summary

This example shows how to create a multi-page web application with server-side rendering using `hyperchad_renderer_html_http`. It demonstrates routing, dynamic content generation, JSON API endpoints, and how to process HTTP requests to generate responses.

## What This Example Demonstrates

- Creating an `HttpApp` with a custom renderer and router
- Defining multiple routes with different page content
- Server-side HTML generation using the `container!` macro
- Type-safe HTML construction with compile-time checking
- JSON API endpoints for dynamic data
- Processing HTTP requests and generating HTTP responses
- Inline CSS styling for a complete user interface
- Proper error handling and logging
- Testing routes without running a full HTTP server

## Prerequisites

- Rust 1.70 or later
- Basic understanding of HTTP concepts
- Familiarity with async/await in Rust
- Knowledge of HTML structure (helpful but not required)

## Running the Example

From the repository root directory:

```bash
cargo run --manifest-path packages/hyperchad/renderer/html/http/examples/basic_http_app/Cargo.toml
```

The example will test multiple routes and display the results, showing how the HTTP adapter processes requests and generates responses.

## Expected Output

When you run the example, you should see:

```
[2025-01-15T10:30:00Z INFO  basic_http_app_example] Creating HyperChad HTTP application...
[2025-01-15T10:30:00Z INFO  basic_http_app_example] Application created successfully!
[2025-01-15T10:30:00Z INFO  basic_http_app_example] Testing different routes:

[2025-01-15T10:30:00Z INFO  basic_http_app_example] Testing GET /
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Status: 200 OK
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Content-Type: text/html; charset=utf-8
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Body size: 3456 bytes
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Preview: <!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width...

[2025-01-15T10:30:00Z INFO  basic_http_app_example] Testing GET /about
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Status: 200 OK
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Body size: 3234 bytes

[2025-01-15T10:30:00Z INFO  basic_http_app_example] Testing GET /contact
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Status: 200 OK
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Body size: 3567 bytes

[2025-01-15T10:30:00Z INFO  basic_http_app_example] Testing GET /api/status
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Status: 200 OK
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Content-Type: application/json
[2025-01-15T10:30:00Z INFO  basic_http_app_example]   Body: {"message":"Server is running!","status":"ok"}

[2025-01-15T10:30:00Z INFO  basic_http_app_example] ✓ All routes processed successfully!
```

This output demonstrates that the HTTP adapter successfully:

- Processes multiple routes with different content
- Generates proper HTTP responses with correct status codes and headers
- Renders HTML pages with server-side rendering
- Returns JSON responses for API endpoints

## Code Walkthrough

### 1. Creating the HTTP Application

The `create_app()` function sets up the entire application:

```rust
fn create_app() -> HttpApp<DefaultHtmlTagRenderer> {
    let router = Router::new();

    // Add routes...
    router.add_route_result("/", |_req| async move {
        Ok(create_home_page())
    });

    let tag_renderer = DefaultHtmlTagRenderer::default();

    HttpApp::new(tag_renderer, router)
        .with_title("HyperChad HTTP Example")
        .with_description("A framework-agnostic HTTP application")
        .with_viewport("width=device-width, initial-scale=1")
        .with_inline_css(/* CSS styles */)
}
```

This demonstrates the builder pattern for configuring the HTTP application with metadata and styling.

### 2. Defining Routes and Page Content

Each page is created using the `container!` macro for type-safe HTML:

```rust
fn create_home_page() -> Content {
    Content::View(Box::new(hyperchad_router::View {
        primary: Some(
            container! {
                div class="page" {
                    header class="header" {
                        h1 { "HyperChad HTTP Application" }
                        nav class="nav" {
                            a href="/" { "Home" }
                        }
                    }
                    main class="main" {
                        // Page content...
                    }
                }
            }.into(),
        ),
        fragments: vec![],
        delete_selectors: vec![],
    }))
}
```

The `container!` macro provides compile-time validation of HTML structure and prevents common mistakes.

### 3. Creating Test Requests

The `create_route_request()` function creates `RouteRequest` objects for testing:

```rust
fn create_route_request(path: &str, method: &str) -> RouteRequest {
    RouteRequest {
        path: path.to_string(),
        method: method.into(),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        cookies: BTreeMap::new(),
        info: RequestInfo::default(),
        body: None,
    }
}
```

In a real application, you'd convert your HTTP server's request type to `RouteRequest`. This abstraction layer allows the HTTP adapter to work with any HTTP server framework (Hyper, Actix, Axum, etc.).

### 4. JSON API Endpoints

The example includes a JSON endpoint to demonstrate API responses:

```rust
router.add_route_result("/api/status", |_req| async move {
    let status = serde_json::json!({
        "status": "ok",
        "message": "Server is running!",
    });

    Ok(Content::Raw {
        data: serde_json::to_vec(&status)?.into(),
        content_type: "application/json".to_string(),
    })
});
```

This shows how to return non-HTML content types.

### 5. Processing Requests

The main function demonstrates how to process requests:

```rust
let home_request = create_route_request("/", "GET");
let home_response = app.process(&home_request).await?;

println!("Status: {}", home_response.status());
println!("Content-Type: {:?}", home_response.headers().get("content-type"));
println!("Body size: {} bytes", home_response.body().len());
```

The `process()` method is the core of the HTTP adapter - it takes a `RouteRequest` and returns a standard `http::Response<Vec<u8>>`. You can integrate this with any HTTP server by:

1. Converting your server's request type to `RouteRequest`
2. Calling `app.process(&request).await`
3. Converting the response back to your server's response type

## Key Concepts

### Framework-Agnostic Design

The `hyperchad_renderer_html_http` package doesn't depend on any specific HTTP server framework. Instead, it:

- Accepts a generic `RouteRequest` type
- Returns standard `http::Response<Vec<u8>>` responses
- Allows you to integrate with any HTTP server (Hyper, Actix, Axum, etc.)

### Type-Safe HTML Generation

The `container!` macro provides compile-time safety:

```rust
container! {
    div class="container" {
        h1 { "Title" }
        p { "Paragraph" }
    }
}
```

- Validates HTML structure at compile time
- Prevents unclosed tags and malformed HTML
- Provides IDE autocomplete for HTML elements

### Content Types

HyperChad supports multiple content types through the `Content` enum:

- `Content::View` - HTML pages with server-side rendering
- `Content::Raw` - Custom content with specified content type (JSON, XML, etc.)
- `Content::Json` - JSON responses (requires `json` feature)

### Router and Navigation

The router handles URL patterns and dispatches to appropriate handlers:

- Routes are matched against incoming requests
- Handlers are async functions returning `Content`
- Dynamic path parameters can be extracted from URLs

## Testing the Example

### 1. Run the Example

Execute the example and observe the output:

```bash
cargo run --manifest-path packages/hyperchad/renderer/html/http/examples/basic_http_app/Cargo.toml
```

Verify that all routes are tested successfully and produce the expected output.

### 2. Modify Routes

Try adding a new route in the code:

```rust
router.add_route_result("/test", |_req| async move {
    Ok(Content::View(Box::new(hyperchad_router::View {
        primary: Some(container! {
            div { h1 { "Test Page" } }
        }.into()),
        fragments: vec![],
        delete_selectors: vec![],
    })))
});
```

Then add a test for it in `main()` and re-run the example.

### 3. Experiment with Different Content Types

Modify the JSON endpoint to return different data structures:

```rust
let status = serde_json::json!({
    "custom": "data",
    "array": [1, 2, 3],
});
```

### 4. Increase Logging Verbosity

See more detailed logs by setting the log level:

```bash
RUST_LOG=debug cargo run --manifest-path packages/hyperchad/renderer/html/http/examples/basic_http_app/Cargo.toml
```

## Troubleshooting

### Compilation Errors

If you encounter missing dependencies:

```bash
# Update dependencies
cargo update

# Clean and rebuild
cargo clean
cargo build --manifest-path packages/hyperchad/renderer/html/http/examples/basic_http_app/Cargo.toml
```

### Example Doesn't Run

If the example fails to execute:

1. Verify you're running from the repository root
2. Check that all workspace dependencies are available
3. Try `cargo check` first to identify any compilation issues

### Understanding the Output

The example output shows:

- **Status codes**: HTTP status codes (200 OK, 404 Not Found, etc.)
- **Content-Type headers**: The MIME type of the response
- **Body size**: Size of the response body in bytes
- **Preview**: First 100 characters of HTML responses
- **Full JSON**: Complete JSON responses for API endpoints

## Related Examples

- **hyperchad/examples/details_summary** - Demonstrates interactive web components with HyperChad
- **web_server/examples/simple_get** - Shows basic HTTP GET request handling
- **hyperchad/renderer/html/web_server/examples/basic_web_server** - Example using the web server-specific integration (Actix-based)

## Next Steps

After understanding this example, you can:

1. **Add more routes** - Create additional pages and endpoints
2. **Implement actions** - Use the `actions` feature to handle form submissions and user interactions
3. **Serve static assets** - Use the `assets` feature to serve CSS, JavaScript, and image files
4. **Add middleware** - Implement logging, authentication, or CORS middleware
5. **Integrate with a database** - Add persistence for dynamic content
6. **Deploy to production** - Configure for production environments with proper error handling and monitoring
