# MoosicBox Web Server Simulator

Simulation backend for moosicbox_web_server that provides in-memory HTTP request/response handling for deterministic testing.

## Features

* **In-Memory HTTP**: No actual network operations
* **Deterministic Behavior**: Predictable responses for testing
* **Route Matching**: Full route and scope support
* **Request/Response Mocking**: Complete HTTP simulation
* **Integration Ready**: Drop-in replacement for actix backend

## Usage

```rust
use moosicbox_web_server::{WebServerBuilder, Scope, HttpResponse};

// Create server with dynamic routes
let server = WebServerBuilder::new()
    .with_scope(Scope::new("/api")
        .get("/health", |_req| {
            Box::pin(async move {
                Ok(HttpResponse::ok().with_body("OK"))
            })
        }))
    .build();

// Start server
server.start().await;
```