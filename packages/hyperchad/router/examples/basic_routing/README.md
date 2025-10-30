# Basic Routing Example

## Summary

This example demonstrates the core routing capabilities of the `hyperchad_router` package, including route matching patterns, HTTP method handling, JSON parsing, query parameters, and header access.

## What This Example Demonstrates

- Creating a router with multiple route patterns
- Exact path matching for simple routes
- Multiple literal routes (matching multiple paths with a single handler)
- Prefix matching for static file serving
- Handling different HTTP methods (GET, POST)
- Parsing JSON request bodies with `parse_body()`
- Accessing query parameters from requests
- Reading request headers
- Error handling for invalid routes
- Returning different content types (HTML strings, JSON, raw content)

## Prerequisites

- Basic understanding of async Rust programming
- Familiarity with HTTP concepts (methods, headers, query parameters)
- Knowledge of JSON and serialization with `serde`

## Running the Example

```bash
cargo run --manifest-path packages/hyperchad/router/examples/basic_routing/Cargo.toml
```

## Expected Output

The example will output a series of navigation results showing how the router handles different types of requests:

```
=== HyperChad Router - Basic Routing Example ===

Router created with the following routes:
  - GET /             (home page)
  - GET /about        (about page)
  - GET,POST /api/users (API endpoint with JSON)
  - GET /api/v1 or /api/v2 (multiple literal routes)
  - GET /static/*     (prefix route for static files)
  - GET /query        (query parameter demo)
  - GET /headers      (header access demo)

1. Navigating to home page (/)...
   Result: Got content successfully

2. Navigating to about page (/about)...
   Result: Got content successfully

3. Making GET request to /api/users...
   Result: Got user list

4. Making POST request to /api/users with JSON body...
   Creating user: Alice (alice@example.com)
   Result: User created successfully

5. Accessing API versions (multiple literal routes)...
   Navigating to /api/v1...
   Result: Success
   Navigating to /api/v2...
   Result: Success

6. Accessing static files (prefix route)...
   Navigating to /static/css/style.css...
   Serving static file: css/style.css
   Result: Got file content
   Navigating to /static/js/app.js...
   Serving static file: js/app.js
   Result: Got file content

7. Accessing route with query parameters...
   Query param: name = Bob
   Query param: age = 30
   Result: Processed query parameters

8. Accessing route that reads headers...
   Header: user-agent: HyperChad-Example/1.0
   Header: accept: application/json
   Result: Read headers successfully

9. Attempting to navigate to non-existent route...
   Error (expected): InvalidPath

=== Example completed successfully! ===
```

## Code Walkthrough

### Main Function

The example creates a Tokio runtime and runs the async demonstration:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::new()?;
    runtime.block_on(async {
        let router = create_router();
        // ... perform various navigation examples
    })?;
    Ok(())
}
```

### Creating the Router

The `create_router()` function demonstrates different route patterns:

**Basic exact path matching:**

```rust
.with_route("/", |_req| async {
    "<h1>Welcome to HyperChad Router!</h1>".to_string()
})
```

**Method-aware API endpoint with JSON parsing:**

```rust
.with_route_result("/api/users", |req| async move {
    match req.method {
        Method::Get => {
            // Return user list
        }
        Method::Post => {
            let user: User = req.parse_body()?;
            // Create user
        }
        _ => {
            // Method not allowed
        }
    }
})
```

**Multiple literal routes (alternatives):**

```rust
.with_route(&["/api/v1", "/api/v2"][..], |req| async move {
    format!("You accessed: {}", req.path)
})
```

**Prefix matching for static files:**

```rust
.with_route(
    RoutePath::LiteralPrefix("/static/".to_string()),
    |req| async move {
        let file_path = req.path.strip_prefix("/static/").unwrap_or("");
        Some(Content::Raw {
            data: Bytes::from(format!("/* Static file: {file_path} */")),
            content_type: "text/css".to_string(),
        })
    },
)
```

### Navigation Examples

The example demonstrates several navigation patterns:

**Simple string navigation:**

```rust
router.navigate("/").await
```

**Manual request construction for POST with JSON body:**

```rust
let json_body = serde_json::to_vec(&user)?;
let post_request = RouteRequest {
    path: "/api/users".to_string(),
    method: Method::Post,
    body: Some(Arc::new(Bytes::from(json_body))),
    headers: [("content-type".to_string(), "application/json".to_string())]
        .into_iter()
        .collect(),
    // ... other fields
};
router.navigate(post_request).await
```

**Query parameters:**

```rust
let query_request = RouteRequest {
    path: "/query".to_string(),
    query: [
        ("name".to_string(), "Bob".to_string()),
        ("age".to_string(), "30".to_string()),
    ]
    .into_iter()
    .collect(),
    // ... other fields
};
```

## Key Concepts

### Route Matching Patterns

The `hyperchad_router` supports three main route matching patterns:

1. **`RoutePath::Literal`** - Exact path match (e.g., `"/about"`)
2. **`RoutePath::Literals`** - Match any of multiple paths (e.g., `&["/api/v1", "/api/v2"]`)
3. **`RoutePath::LiteralPrefix`** - Match paths starting with a prefix (e.g., `"/static/"`)

### Route Handler Types

Two handler registration methods are available:

- **`with_route`** - For handlers that cannot fail (return `String` or other content directly)
- **`with_route_result`** - For handlers that return `Result` (for error handling)

### Request Parsing

The `RouteRequest` provides convenient parsing methods:

- **`parse_body::<T>()`** - Parse JSON from request body
- **`parse_form::<T>()`** - Parse multipart form data (requires `form` feature)
- **`query`** - Access query parameters as a `BTreeMap<String, String>`
- **`headers`** - Access headers as a `BTreeMap<String, String>`

### Content Types

Route handlers can return different content types:

- **String** - HTML or text content
- **`Content::Raw`** - Raw bytes with custom MIME type
- **JSON** - Via serialization to string

## Testing the Example

### Verify Router Creation

The example creates the router and prints all registered routes. Ensure all routes are listed correctly.

### Test Each Route Type

The example automatically tests:

- Basic navigation to exact paths
- GET and POST requests to the same endpoint
- Multiple literal route alternatives
- Prefix matching with different file paths
- Query parameter extraction
- Header reading
- Error handling for invalid routes

### Inspect Output

Check that:

- All successful navigations return content
- JSON parsing works for POST requests
- Query parameters are correctly extracted
- Headers are accessible
- Invalid routes return `InvalidPath` error

## Troubleshooting

### Compilation Errors

If you encounter compilation errors, ensure:

- The workspace dependencies are properly configured
- You're using a compatible Rust version (1.70+)
- All required features are enabled in the workspace

### Runtime Errors

Common runtime issues:

- **"MissingBody" error**: Ensure the request has a body when calling `parse_body()`
- **"SerdeJson" error**: Check that the JSON in the request body is valid
- **"InvalidPath" error**: The route doesn't exist in the router (this is expected for the final test case)

### Navigation Failures

If navigation fails unexpectedly:

- Check that the route is registered in the router
- Verify the path exactly matches the registered route
- For prefix routes, ensure the path starts with the prefix
- For multiple literals, ensure the path matches one of the alternatives

## Related Examples

- See `packages/hyperchad/examples/details_summary/` for a full web application example using the router
- See `packages/hyperchad/examples/http_events/` for HTTP event handling with the router
- See `packages/web_server/examples/simple_get/` for integrating the router with a web server
