# Nested GET Example

This example demonstrates how to organize routes using nested scopes in the MoosicBox web server. It shows how scope prefixes combine with route paths to create hierarchical URL structures, which is essential for organizing larger APIs.

## What This Example Demonstrates

- **Nested Scopes**: Creating routes under scope prefixes
- **Path Composition**: How scope paths combine with route paths
- **Route Organization**: Hierarchical structure for API endpoints
- **CORS with Nested Routes**: CORS configuration applies to all nested routes
- **URL Structure**: Creating clean, organized endpoint hierarchies

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic HTTP routing concepts

## Running the Example

```bash
# From repository root
cargo run -p switchy_web_server_example_nested_get

# From example directory
cd packages/switchy/web_server/examples/nested_get
cargo run

# With NixOS
nix develop .#server --command cargo run -p switchy_web_server_example_nested_get
```

Note: This example uses the actix backend, which is enabled via the features in the switchy_web_server dependency.

## Expected Output

The server creates a single endpoint at `/nested/example` by combining:

- Scope prefix: `/nested`
- Route path: `/example`
- Final endpoint: `/nested/example`

## Testing the Server

### Manual Testing with curl

**Access the Nested Route**

```bash
curl http://localhost:8080/nested/example
# Expected: hello, world! path=/nested/example query=
```

**With Query Parameters**

```bash
curl "http://localhost:8080/nested/example?category=test&id=456"
# Expected: hello, world! path=/nested/example query=category=test&id=456
```

**Test Non-Existent Routes**

```bash
# This should return 404 - route doesn't exist at root level
curl http://localhost:8080/example

# This should return 404 - wrong nested path
curl http://localhost:8080/other/example
```

## Code Walkthrough

### Scope Creation and Nesting

```rust
let server = switchy_web_server::WebServerBuilder::new()
    .with_cors(cors)
    .with_scope(
        Scope::new("/nested")           // Create scope with prefix
            .get("/example", handler)   // Add route to scope
    )
    .build();
```

### Path Resolution

- **Scope prefix**: `/nested`
- **Route path**: `/example`
- **Final URL**: `http://localhost:8080/nested/example`

### Handler Implementation

```rust
.get("/example", |req| {
    let path = req.path().to_string();      // Returns "/nested/example"
    let query = req.query_string().to_string();
    Box::pin(async move {
        Ok(HttpResponse::ok()
            .with_body(format!("hello, world! path={path} query={query}")))
    })
})
```

## Advanced Nesting Patterns

### Multiple Levels of Nesting

```rust
// This example shows single-level nesting, but you can nest deeper:
Scope::new("/api")
    .with_scope(
        Scope::new("/v1")
            .with_scope(
                Scope::new("/users")
                    .get("/{id}", get_user_handler)
                    .post("", create_user_handler)
            )
    )
// Results in: /api/v1/users/{id} and /api/v1/users
```

### Multiple Routes in Same Scope

```rust
Scope::new("/nested")
    .get("/example", handler1)
    .get("/other", handler2)
    .post("/create", handler3)
// Results in: /nested/example, /nested/other, /nested/create
```

## Key Concepts

### Scope Benefits

- **Organization**: Group related routes logically
- **Maintainability**: Easier to manage large APIs
- **Middleware**: Apply middleware to entire scopes
- **Versioning**: Easy API versioning with scope prefixes

### Path Composition Rules

- Scope paths and route paths are concatenated
- Leading/trailing slashes are handled automatically
- Empty scope prefix (`""`) creates routes at root level
- Multiple scopes can be nested for complex hierarchies

### CORS Inheritance

- CORS configuration applies to all routes in all scopes
- Nested routes inherit the server's CORS settings
- No additional CORS configuration needed for nested routes

## Comparison with simple_get

| Aspect           | nested_get             | simple_get            |
| ---------------- | ---------------------- | --------------------- |
| **Endpoint URL** | `/nested/example`      | `/example`            |
| **Scope Usage**  | Uses `/nested` prefix  | Uses empty `""` scope |
| **Organization** | Demonstrates hierarchy | Shows flat structure  |
| **Use Case**     | API organization       | Simple endpoints      |

## Real-World Applications

### API Versioning

```rust
Scope::new("/api/v1")
    .get("/users", list_users_v1)
    .get("/posts", list_posts_v1)

Scope::new("/api/v2")
    .get("/users", list_users_v2)
    .get("/posts", list_posts_v2)
```

### Feature Grouping

```rust
Scope::new("/admin")
    .get("/dashboard", admin_dashboard)
    .get("/users", admin_users)

Scope::new("/public")
    .get("/health", health_check)
    .get("/status", status_check)
```

## Troubleshooting

### Route Not Found (404)

**Problem**: Requests to `/example` return 404
**Solution**: The route is at `/nested/example`, not `/example`

### Build Issues

**Problem**: Compilation errors about missing traits
**Solution**: The actix feature is enabled by default via workspace dependencies

### Path Confusion

**Problem**: Unsure what the final URL will be
**Solution**: Final URL = scope prefix + route path

## Related Examples

- **simple_get**: Shows basic routing without nesting
- **openapi**: Demonstrates nested routes with API documentation
- **basic_handler**: Shows different handler registration approach
- **combined_extractors_standalone**: Complex handlers in nested structures

This example is fundamental for understanding how to organize larger web applications with the MoosicBox web server abstraction, providing the foundation for building well-structured APIs.
