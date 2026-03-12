# MoosicBox Web Server Simulator

In-memory HTTP request/response simulation for deterministic testing. This package provides the core simulation types and server implementation used by the switchy_web_server testing framework.

## Features

- **In-Memory HTTP**: No actual network operations
- **Deterministic Behavior**: Predictable responses for testing
- **Request/Response Types**: Complete HTTP request/response simulation with headers, bodies, and status codes
- **Route Handlers**: Dynamic route registration with async handler support
- **Mock Responses**: Pre-configured responses for specific request patterns
- **Request Logging**: Track all handled requests for test verification

## Core Types

- `SimulationWebServer`: Main server implementation with route and mock management (`add_route`, `add_mock_response`, `start`, `stop`, `handle_request`, `get_request_log`, `clear_request_log`)
- `SimulatedRequest`: Represents an HTTP request with method, path, headers, and body (`new`, `with_query_string`, `with_header`, `with_headers`, `with_body`, `with_json_body`)
- `SimulatedResponse`: Represents an HTTP response with status code, headers, and body (`new`, `ok`, `not_found`, `internal_server_error`, `with_header`, `with_body`, `with_text_body`, `with_html_body`, `with_json_body`)
- `RouteHandler`: Async handler function for processing requests (`RouteHandler::new` for custom handlers)
- `Error`: Error types for route matching and handler execution

## Usage

```rust
use switchy_web_server_simulator::{
    SimulationWebServer, SimulatedRequest, SimulatedResponse, RouteHandler,
    handlers,
};
use switchy_http_models::Method as HttpMethod;

// Create server
let server = SimulationWebServer::new();

// Add a route handler
let handler = handlers::text_response(
    HttpMethod::Get,
    "/health",
    "OK"
);
server.add_route(handler).await;

// Or add a mock response
server.add_mock_response(
    "GET /status",
    SimulatedResponse::ok().with_text_body("running")
).await;

// Start server
server.start().await.unwrap();

// Handle requests
let request = SimulatedRequest::new(HttpMethod::Get, "/health");
let response = server.handle_request(request).await.unwrap();
```

## Helper Functions

The `handlers` module provides convenience functions for common response patterns:

- `json_response()`: Create JSON response handlers
- `text_response()`: Create plain text response handlers
- `html_response()`: Create HTML response handlers
- `health_check()`: Create health check endpoints

## Integration

This package is typically used through the `switchy_web_server` crate's simulator backend feature, which provides higher-level integration with `WebServerBuilder`, `Scope`, and routing APIs.
