# Basic Actix Server Example

## Summary

This example demonstrates how to build a simple web server using the HyperChad Actix renderer, showcasing the `ActixResponseProcessor` trait pattern, basic routing, and HTML content serving.

## What This Example Demonstrates

- **Actix Web Integration**: Setting up a web server with the HyperChad Actix renderer
- **ActixResponseProcessor Pattern**: Implementing the processor trait for custom request/response handling
- **Basic Routing**: Handling multiple routes with a simple match-based routing system
- **HTML Content Generation**: Serving static HTML content with proper content types
- **Error Handling**: Implementing 404 pages for non-existent routes
- **XSS Prevention**: Basic HTML escaping for user input

## Prerequisites

- Basic knowledge of Rust and async programming
- Understanding of web server concepts (HTTP requests, responses, routing)
- Familiarity with Actix Web framework
- A development environment set up for the MoosicBox project

## Running the Example

From the MoosicBox root directory:

```bash
# Using the full manifest path (recommended)
cargo run --manifest-path packages/hyperchad/renderer/html/actix/examples/basic_actix_server/Cargo.toml

# Or navigate to the example directory
cd packages/hyperchad/renderer/html/actix/examples/basic_actix_server
cargo run
```

The server will start on `http://0.0.0.0:8343` by default (configurable via the `PORT` environment variable).

## Expected Output

When you run the example, you should see logging output similar to:

```
[INFO] Starting HyperChad Actix Server Example
[INFO] Server is starting...
[INFO] Visit http://localhost:8343 to view the application
[INFO] Server started on 0.0.0.0:8343
```

You can then:

- Visit `http://localhost:8343/` to see the home page
- Navigate to `http://localhost:8343/about` for information about the example
- Visit `http://localhost:8343/contact` for the contact page
- Try `http://localhost:8343/nonexistent` to see the 404 error page

## Code Walkthrough

### 1. Request Data Structure

The example uses a simple struct to hold request information:

```rust
#[derive(Clone)]
struct SimpleRequest {
    path: String,
    method: String,
}
```

This demonstrates how to extract and store relevant request data for processing.

### 2. ActixResponseProcessor Implementation

The core of the example is the `SimpleProcessor` that implements `ActixResponseProcessor`:

```rust
#[derive(Clone)]
struct SimpleProcessor;

#[async_trait]
impl ActixResponseProcessor<SimpleRequest> for SimpleProcessor {
    fn prepare_request(
        &self,
        req: HttpRequest,
        _body: Option<Arc<Bytes>>,
    ) -> Result<SimpleRequest, actix_web::Error> {
        // Extract request information
        Ok(SimpleRequest {
            path: req.path().to_string(),
            method: req.method().to_string(),
        })
    }

    async fn to_response(&self, data: SimpleRequest) -> Result<HttpResponse, actix_web::Error> {
        // Route to the appropriate handler
        let html_content = match data.path.as_str() {
            "/" => generate_home_page(),
            "/about" => generate_about_page(),
            "/contact" => generate_contact_page(),
            _ => generate_not_found_page(&data.path),
        };

        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html_content))
    }

    async fn to_body(
        &self,
        content: Content,
        _data: SimpleRequest,
    ) -> Result<(Bytes, String), actix_web::Error> {
        // Convert content for streaming updates
        let body = content.to_string();
        Ok((Bytes::from(body), "text/html; charset=utf-8".to_string()))
    }
}
```

### 3. Server Setup and Initialization

The main function sets up the server components:

```rust
fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    // Create renderer event channel (required but unused in this simple example)
    let (_tx, rx) = flume::unbounded::<RendererEvent>();

    // Create processor and application
    let processor = SimpleProcessor;
    let app = ActixApp::new(processor, rx);

    // Get async runtime handle
    let handle = tokio::runtime::Handle::current();

    // Convert to runner and start server
    let mut runner = app.to_runner(hyperchad_renderer::Handle::Tokio(handle))?;
    runner.run()?;

    Ok(())
}
```

### 4. HTML Content Generation

Each route has its own HTML generation function with embedded CSS:

```rust
fn generate_home_page() -> String {
    r#"<!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <title>HyperChad Actix Example - Home</title>
        <style>/* Styles here */</style>
    </head>
    <body>
        <!-- Content here -->
    </body>
    </html>"#.to_string()
}
```

### 5. Security: HTML Escaping

The example includes basic XSS prevention for dynamic content:

```rust
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
```

## Key Concepts

### ActixResponseProcessor Trait

The `ActixResponseProcessor` trait is the core abstraction for handling HTTP requests in HyperChad Actix applications. It provides three key methods:

1. **`prepare_request()`**: Extracts and transforms raw HTTP request data into your custom data structure
2. **`to_response()`**: Converts your data structure into an HTTP response (main routing logic goes here)
3. **`to_body()`**: Converts `Content` to bytes for streaming scenarios (SSE, partial updates)

### ActixApp Configuration

The `ActixApp` struct wraps your processor and renderer event channel:

- Takes a processor implementing `ActixResponseProcessor`
- Requires a `flume::Receiver<RendererEvent>` for rendering events
- Can be extended with actions (via `with_action_tx()`)
- Supports static asset routes (via the `static_asset_routes` field)

### Renderer Events

While this simple example doesn't use renderer events, the channel is required for the framework. In more complex applications, renderer events enable:

- Server-Sent Events (SSE) for real-time updates
- Partial page updates for HTMX integration
- Canvas updates for interactive graphics

### Environment Configuration

The server port can be configured via the `PORT` environment variable:

```bash
PORT=3000 cargo run --manifest-path packages/hyperchad/renderer/html/actix/examples/basic_actix_server/Cargo.toml
```

## Testing the Example

### 1. Basic Navigation

Open your browser and test the routes:

1. Home page: `http://localhost:8343/`
2. About page: `http://localhost:8343/about`
3. Contact page: `http://localhost:8343/contact`
4. Non-existent route: `http://localhost:8343/test` (should show 404)

### 2. Command Line Testing

Use `curl` to test the server:

```bash
# Test home page
curl http://localhost:8343/

# Test about page
curl http://localhost:8343/about

# Test 404 handling
curl http://localhost:8343/nonexistent

# View response headers
curl -I http://localhost:8343/
```

### 3. Verify HTML Output

Check that proper HTML is served:

```bash
curl -s http://localhost:8343/ | head -n 5
```

Should output:

```html
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    </head>
</html>
```

## Troubleshooting

### Port Already in Use

If you see an error like "Address already in use":

```
Error: Os { code: 98, kind: AddrInUse, message: "Address already in use" }
```

**Solution**: Change the port using the `PORT` environment variable:

```bash
PORT=8080 cargo run --manifest-path packages/hyperchad/renderer/html/actix/examples/basic_actix_server/Cargo.toml
```

### Cannot Connect to Server

If your browser shows "Unable to connect":

1. Check that the server is running (look for "Server started" in logs)
2. Verify the port matches what you're accessing in the browser
3. Try `http://localhost:8343` instead of `http://127.0.0.1:8343`

### Compilation Errors

If you encounter compilation errors:

1. Ensure you're in the MoosicBox root directory
2. Try cleaning the build: `cargo clean`
3. Rebuild: `cargo build --manifest-path packages/hyperchad/renderer/html/actix/examples/basic_actix_server/Cargo.toml`

## Related Examples

- **`packages/hyperchad/renderer/html/web_server/examples/basic_web_server/`** - Similar example using the web_server backend instead of Actix
- **`packages/hyperchad/renderer/html/examples/basic_rendering/`** - Basic HTML rendering without a server
- **`packages/hyperchad/examples/`** - Core HyperChad examples for templates and components
- **`packages/web_server/examples/simple_get/`** - Basic web server examples

## Next Steps

To extend this example, you could:

1. **Add the HyperChad template system**: Use `container!` macros for type-safe HTML generation
2. **Enable form handling**: Parse POST request bodies and process form data
3. **Add SSE support**: Implement real-time updates with Server-Sent Events
4. **Implement actions**: Use the `actions` feature for interactive functionality
5. **Add static assets**: Serve CSS, JavaScript, and image files
6. **Integrate a database**: Store and retrieve data from a database
7. **Add authentication**: Implement user login and session management
8. **Use HTMX**: Add partial page updates with HTMX integration

See the package README at `packages/hyperchad/renderer/html/actix/README.md` for more advanced usage patterns.
