# CDN Optimization Example

This example demonstrates how to use `hyperchad_renderer_html_cdn` to optimize HyperChad web applications for CDN deployment.

## Summary

This example shows how to apply CDN optimization to a HyperChad web application using the `setup_cdn_optimization` function. The optimization creates a static skeleton HTML that can be cached by CDNs, while dynamic content is loaded via JavaScript at runtime.

## What This Example Demonstrates

- Using `setup_cdn_optimization()` to enable CDN caching for HyperChad apps
- How the skeleton HTML mechanism works with dynamic content fetching
- Configuring page title and viewport meta tags for the CDN skeleton
- Observing the two-stage loading process in browser Network tools
- Maintaining full dynamic functionality with CDN optimization

## Prerequisites

- Basic understanding of HyperChad routing and rendering
- Familiarity with CDN concepts and caching strategies
- Knowledge of web server basics

## Running the Example

From the MoosicBox root directory:

```bash
# Build and run
nix develop .#fltk-hyperchad --command bash -c "cd packages/hyperchad/renderer/html/cdn/examples/basic_cdn_optimization && cargo run"

# Or just build
nix develop .#fltk-hyperchad --command bash -c "cd packages/hyperchad/renderer/html/cdn/examples/basic_cdn_optimization && cargo build"
```

The server will start on `http://localhost:8343` by default.

## Expected Output

When you run the example, you should see:

```
[INFO] Starting CDN Optimization Example
[INFO] CDN optimization configured - skeleton HTML will be served for /
[DEBUG] Auto-registered /__hyperchad_dynamic_root__ for CDN optimization
[DEBUG] CDN optimization configured - skeleton index.html will be generated as static asset
[INFO] Server starting on http://localhost:8343
[INFO] Visit the page and check the Network tab to see CDN optimization in action
```

When you visit `http://localhost:8343` in a browser:

1. The initial skeleton HTML loads immediately (this would come from CDN in production)
2. JavaScript fetches the full content from `/__hyperchad_dynamic_root__`
3. The page is replaced with the complete dynamic content
4. You see a styled page with header, main content, and footer

## Code Walkthrough

### 1. Creating a Standard Router

First, we create a router with dynamic routes as usual:

```rust
fn create_router() -> Router {
    let router = Router::new();

    // Home route - will be automatically optimized for CDN
    router.add_route_result("/", |_req: RouteRequest| async move {
        Ok(create_home_page())
    });

    // Other routes...
    router
}
```

### 2. Applying CDN Optimization

The key step is calling `setup_cdn_optimization()` on the router:

```rust
let router = setup_cdn_optimization(
    router,
    Some("CDN-Optimized HyperChad App"),         // Page title
    Some("width=device-width, initial-scale=1"), // Viewport meta tag
);
```

This function:

- Detects if the root route ("/") is dynamic
- Replaces it with a static skeleton HTML route
- Registers a new endpoint at `/__hyperchad_dynamic_root__` with the original handler
- Configures the skeleton to fetch dynamic content via JavaScript

### 3. The Skeleton HTML Structure

The generated skeleton HTML looks like this:

```html
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>CDN-Optimized HyperChad App</title>
    </head>
    <body>
        <script>
            fetch('/__hyperchad_dynamic_root__')
                .then((response) => response.text())
                .then((html) => {
                    document.open();
                    document.write(html);
                    document.close();
                })
                .catch((error) => {
                    document.write(`<div style="color: red;">Failed to load content: ${error.message}</div>`);
                });
        </script>
    </body>
</html>
```

### 4. Starting the Web Server

Finally, create and run the web server as usual:

```rust
let app = router_to_web_server(DefaultHtmlTagRenderer::default(), router)
    .with_title(Some("CDN-Optimized HyperChad Example".to_string()))
    .with_description(Some(
        "Demonstrates CDN optimization for HyperChad applications".to_string(),
    ));

let runtime = switchy::unsync::runtime::Runtime::new();
let handle = runtime.handle();
let mut runner = app.to_runner(handle)?;
runner.run()?;
```

## Key Concepts

### CDN-Friendly Architecture

The optimization splits your application into two parts:

1. **Static skeleton** - Minimal HTML that can be cached indefinitely by CDNs
2. **Dynamic content endpoint** - Server-rendered content fetched at runtime

This provides:

- **Fast initial load**: Skeleton served from CDN edge locations
- **Dynamic functionality**: Full application features preserved
- **Cost efficiency**: Static assets don't consume compute resources
- **Scalability**: CDN handles traffic spikes automatically

### When CDN Optimization Activates

The `setup_cdn_optimization()` function only activates when:

- The root route ("/") exists
- The root route is dynamic (not static)

If these conditions aren't met, the function returns the router unchanged.

### Document Replacement Technique

The skeleton uses `document.open()`, `document.write()`, and `document.close()` to replace the entire document with the fetched content. This provides a seamless user experience where the full page appears to load as a single unit.

### Configuring the Skeleton

The skeleton can be customized with optional parameters:

- `title: Option<&str>` - Sets the `<title>` tag
- `viewport: Option<&str>` - Sets the viewport meta tag content

If `None` is provided, the corresponding elements are omitted.

## Testing the Example

### 1. Observe Network Requests

Open your browser's Developer Tools (F12) and go to the Network tab:

1. Visit `http://localhost:8343`
2. You'll see two requests:
    - `GET /` - The skeleton HTML (cacheable by CDN)
    - `GET /__hyperchad_dynamic_root__` - The dynamic content

### 2. Check the Dynamic Endpoint

You can also directly access the dynamic endpoint:

```bash
curl http://localhost:8343/__hyperchad_dynamic_root__
```

This returns the full rendered HTML that would normally be served at "/".

### 3. Test the API Endpoint

The example includes a JSON API endpoint to demonstrate that other routes work normally:

```bash
curl http://localhost:8343/api/info
```

Expected response:

```json
{
    "cdn_enabled": true,
    "message": "CDN optimization is active!",
    "timestamp": 1234567890
}
```

### 4. Navigate to Other Routes

Visit `http://localhost:8343/about` to see a regular dynamic route without CDN optimization. Only the root route ("/") gets optimized.

## Troubleshooting

### Issue: Skeleton loads but no content appears

**Cause**: The dynamic endpoint might be failing or the fetch request is blocked.

**Solution**:

- Check the browser console for JavaScript errors
- Ensure the server is running on the expected port
- Verify CORS settings if accessing from a different domain

### Issue: CDN optimization not activating

**Cause**: The root route might be defined as a static route or doesn't exist.

**Solution**:

- Ensure you're using `.add_route()` or `.add_route_result()` for "/"
- Don't use `.with_static_route()` for the root route if you want CDN optimization
- Check logs for "CDN optimization configured - root route is static" message

### Issue: Page flashes during load

**Cause**: This is normal behavior as the skeleton is replaced with dynamic content.

**Solution**:

- In production, add a loading indicator to the skeleton HTML
- The flash is minimal and only occurs on initial page load
- CDN edge caching makes subsequent loads very fast

## Related Examples

- [HyperChad Basic Web Server](../../web_server/examples/basic_web_server/) - Foundation for web applications without CDN optimization
- [HyperChad Details/Summary](../../../examples/details_summary/) - Component examples that work with CDN optimization

## Production Deployment

When deploying to production with a CDN:

1. Configure your CDN to cache the root route ("/") with a long TTL
2. Set the dynamic endpoint (`/__hyperchad_dynamic_root__`) to bypass the CDN
3. Enable compression (gzip/brotli) for both routes
4. Consider adding a loading indicator to the skeleton HTML
5. Monitor cache hit rates and adjust TTL as needed

Example CDN configuration (conceptual):

```
Route: /
  - Cache: Yes
  - TTL: 1 year (immutable)
  - Origin: Your web server

Route: /__hyperchad_dynamic_root__
  - Cache: No
  - Origin: Your compute backend
```
