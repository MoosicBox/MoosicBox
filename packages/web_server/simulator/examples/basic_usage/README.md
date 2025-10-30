# Basic Usage Example

This example demonstrates the fundamental usage patterns of the `web_server_simulator` package for testing HTTP interactions in a simulated environment without starting a real server.

## Summary

Demonstrates how to create a simulated web server, register route handlers, add mock responses, handle requests, and inspect request logs for testing purposes.

## What This Example Demonstrates

- Creating a `SimulationWebServer` instance
- Adding route handlers using the `handlers` module helper functions
- Registering mock responses for specific request patterns
- Starting and stopping the simulation server
- Making simulated HTTP requests with various methods, headers, and query strings
- Processing different response types (text, HTML, JSON)
- Logging and inspecting all handled requests
- Error handling when routes are not found
- Using the server for unit testing workflows

## Prerequisites

- Basic understanding of HTTP concepts (methods, status codes, headers)
- Familiarity with async Rust and the Tokio runtime
- Understanding of JSON serialization with Serde

## Running the Example

Execute the example using Cargo from the repository root:

```bash
cargo run --manifest-path packages/web_server/simulator/examples/basic_usage/Cargo.toml
```

Or with verbose logging:

```bash
RUST_LOG=debug cargo run --manifest-path packages/web_server/simulator/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will output a step-by-step walkthrough showing:

```
=== Web Server Simulator - Basic Usage Example ===

1. Creating simulation web server...
   Server created (not yet started)

2. Adding route handlers...
   Added: GET /hello (text response)
   Added: GET / (HTML response)
   Added: GET /api/user (JSON response)
   Added: GET /health (health check)

3. Adding mock responses...
   Added mock: GET /status

4. Starting simulation server...
   Server is now running: true

5. Making simulated requests...

   Request: GET /hello
   Status: 200
   Body: Hello, World!

   Request: GET /
   Status: 200
   Content-Type: text/html

   Request: GET /api/user
   Status: 200
   User: User { id: 1, name: "Alice", email: "alice@example.com" }

   Request: GET /health
   Status: 200
   Health: {"status":"ok","timestamp":"simulation"}

   Request: GET /status
   Status: 200
   Body: Service is running

6. Making request with headers and query string...

   Status: 200

7. Checking request log...
   Total requests logged: 6
   #1: GET /hello
   #2: GET /
   #3: GET /api/user
   #4: GET /health
   #5: GET /status
   #6: GET /hello

8. Testing error handling...

   Expected error: Route not found: GET /nonexistent

9. Clearing request log...
   Requests in log: 0

10. Stopping simulation server...
    Server is now running: false

=== Example completed successfully! ===
```

## Code Walkthrough

### 1. Creating the Server

```rust
let server = SimulationWebServer::new();
```

Creates a new simulation web server instance. The server starts in a stopped state and must be explicitly started before handling requests.

### 2. Adding Route Handlers

The `handlers` module provides convenient helper functions for common response types:

```rust
// Text response
let hello_handler = handlers::text_response(HttpMethod::Get, "/hello", "Hello, World!");
server.add_route(hello_handler).await;

// HTML response
let home_handler = handlers::html_response(
    HttpMethod::Get,
    "/",
    "<html><body><h1>Welcome</h1></body></html>",
);
server.add_route(home_handler).await;

// JSON response with structured data
let user = User { id: 1, name: "Alice".to_string(), email: "alice@example.com".to_string() };
let user_handler = handlers::json_response(HttpMethod::Get, "/api/user", user);
server.add_route(user_handler).await;

// Health check endpoint
let health_handler = handlers::health_check("/health");
server.add_route(health_handler).await;
```

### 3. Adding Mock Responses

For simpler testing scenarios, you can add mock responses that don't require handler logic:

```rust
server.add_mock_response(
    "GET /status",
    SimulatedResponse::ok().with_text_body("Service is running"),
).await;
```

Mock responses are checked before route handlers and use a simple `"METHOD path"` key format.

### 4. Starting the Server

```rust
server.start().await?;
```

The server must be started before it can handle requests. Attempting to handle requests on a stopped server will return a `ServerNotStarted` error.

### 5. Making Requests

Create requests with the `SimulatedRequest` builder API:

```rust
// Simple request
let request = SimulatedRequest::new(HttpMethod::Get, "/hello");
let response = server.handle_request(request).await?;

// Request with headers and query string
let request = SimulatedRequest::new(HttpMethod::Get, "/hello")
    .with_query_string("lang=en")
    .with_header("User-Agent", "ExampleClient/1.0")
    .with_header("Accept", "text/plain");
let response = server.handle_request(request).await?;
```

### 6. Processing Responses

Responses contain status codes, headers, and optional bodies:

```rust
println!("Status: {}", response.status_code);
if let Some(body) = &response.body {
    let user: User = serde_json::from_slice(body)?;
    println!("User: {:?}", user);
}
```

### 7. Request Logging

All requests are automatically logged and can be inspected:

```rust
let log = server.get_request_log();
println!("Total requests logged: {}", log.len());
for req in log.iter() {
    println!("{} {}", req.method, req.path);
}

// Clear the log when needed
server.clear_request_log();
```

## Key Concepts

### In-Memory Simulation

The web server simulator operates entirely in memory with no network operations. This makes tests:

- **Fast**: No network overhead or port binding
- **Deterministic**: Same inputs always produce same outputs
- **Isolated**: No interference between tests or external dependencies

### Route Priority

The simulator checks mock responses before route handlers. This allows you to:

- Override specific routes with mocks during testing
- Use handlers for complex logic and mocks for simple responses

### Request Logging

Every request is logged automatically, enabling test assertions about:

- Number of requests made
- Request methods and paths
- Headers and bodies sent
- Order of operations

### Error Handling

The simulator provides specific errors for different failure scenarios:

- `RouteNotFound`: No matching route or mock response
- `ServerNotStarted`: Attempted to handle request before starting
- `HandlerFailed`: Handler execution error

## Testing the Example

The example is designed to demonstrate typical testing workflows:

1. **Setup**: Create server and register routes
2. **Act**: Make simulated requests
3. **Assert**: Verify responses and request log

You can modify the example to experiment with:

- Different HTTP methods (POST, PUT, DELETE)
- Request bodies and JSON payloads
- Custom headers
- Error responses (4xx, 5xx status codes)
- Multiple concurrent requests

## Troubleshooting

### "Server not started" errors

Ensure you call `server.start().await?` before handling requests:

```rust
server.start().await?;
let response = server.handle_request(request).await?;
```

### "Route not found" errors

Verify that:

1. The route was added before starting the server
2. The HTTP method matches exactly
3. The path matches exactly (no pattern matching yet)

### JSON deserialization errors

Ensure the response has a JSON body and the struct matches the data structure:

```rust
if let Some(body) = &response.body {
    match serde_json::from_slice::<User>(body) {
        Ok(user) => println!("User: {:?}", user),
        Err(e) => eprintln!("JSON error: {}", e),
    }
}
```

## Related Examples

This is currently the only example for `web_server_simulator`. For more advanced usage patterns, refer to the unit tests in `packages/web_server/simulator/src/lib.rs`.
