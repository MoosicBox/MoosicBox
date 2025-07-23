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
use web_server_simulator::SimulationWebServer;
use moosicbox_web_server::{WebServerBuilder, Scope, route};

// Create simulation server
let server = SimulationWebServer::new()
    .with_scope(Scope::new("/api")
        .with_route(route!(GET, health, "/health", health_handler)));

// Start simulation
server.start().await?;

// Send simulated requests
let response = server.handle_request(
    HttpMethod::Get,
    "/api/health",
    None,
    BTreeMap::new()
).await?;
```