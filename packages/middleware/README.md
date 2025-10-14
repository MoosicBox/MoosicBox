# MoosicBox Middleware

Basic HTTP middleware collection for the MoosicBox web server ecosystem, providing request logging and service information utilities for Actix Web applications.

## Features

- **API Logger**: Request/response logging middleware with timing and status tracking
- **Service Info**: Request extractor for service metadata (port information)
- **Tunnel Info**: Request extractor for tunnel host configuration (enabled by default)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_middleware = "0.1.4"

# Disable tunnel middleware (enabled by default)
moosicbox_middleware = { version = "0.1.4", default-features = false }
```

## Usage

### API Logger Middleware

```rust
use moosicbox_middleware::api_logger::ApiLogger;
use actix_web::{web, App, HttpServer, HttpResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(ApiLogger::new())
            .route("/hello", web::get().to(hello_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn hello_handler() -> HttpResponse {
    HttpResponse::Ok().json("Hello, World!")
}
```

The API logger middleware provides:
- Request method, path, and query string logging
- Request headers logging (Range)
- Response headers logging (Content-Range, Accept-Ranges, Content-Length)
- Response status and timing information
- Success/failure status tracking
- Error details for failed requests

### Service Info Extractor

```rust
use moosicbox_middleware::service_info::{ServiceInfo, init};
use actix_web::{web, App, HttpServer, HttpResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize service info globally
    init(ServiceInfo { port: 8080 }).expect("Failed to initialize service info");

    HttpServer::new(|| {
        App::new()
            .route("/api/status", web::get().to(status_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn status_handler(service_info: ServiceInfo) -> HttpResponse {
    HttpResponse::Ok().json(format!("Service running on port {}", service_info.port))
}
```

### Tunnel Info (Default Feature)

The `tunnel` feature is enabled by default:

```rust
use moosicbox_middleware::tunnel_info::{TunnelInfo, init};
use actix_web::{web, App, HttpServer, HttpResponse};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tunnel info globally
    init(TunnelInfo {
        host: Arc::new(Some("tunnel.example.com".to_string()))
    }).expect("Failed to initialize tunnel info");

    HttpServer::new(|| {
        App::new()
            .route("/tunnel/api", web::get().to(tunnel_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn tunnel_handler(tunnel_info: TunnelInfo) -> HttpResponse {
    match tunnel_info.host.as_ref() {
        Some(host) => HttpResponse::Ok().json(format!("Tunnel host: {}", host)),
        None => HttpResponse::Ok().json("No tunnel host configured"),
    }
}
```

## Middleware Details

### ApiLogger

The `ApiLogger` middleware logs:
- **Request Start**: Method, path, query string, and relevant headers
- **Request End**: Response status, timing, and relevant response headers
- **Success/Failure**: Different log levels for success vs. error responses
- **Error Details**: Full error information for debugging

Log output example:
```
TRACE GET /api/tracks?limit=10 headers=[("range", "bytes=0-1023")] STARTED
TRACE GET /api/tracks?limit=10 headers=[("range", "bytes=0-1023")] resp_headers=[("content-length", "2048"), ("accept-ranges", "bytes")] FINISHED SUCCESS "200 OK" (25 ms)
```

### ServiceInfo

The `ServiceInfo` extractor provides access to service metadata (e.g., port number) via Actix Web's request extraction mechanism. Initialize once at startup, then extract in handlers.

### TunnelInfo

The `TunnelInfo` extractor provides access to tunnel host configuration via Actix Web's request extraction mechanism. Enabled by default via the `tunnel` feature. Initialize once at startup, then extract in handlers.

## Core Components

```rust
// API Logger (middleware)
pub struct ApiLogger;
pub struct ApiLoggerMiddleware<S>;

// Service Info (request extractor)
pub struct ServiceInfo {
    pub port: u16,
}

// Tunnel Info (request extractor, feature-gated)
#[cfg(feature = "tunnel")]
pub struct TunnelInfo {
    pub host: Arc<Option<String>>,
}
```

## Dependencies

- `actix-web`: Web framework for middleware integration
- `futures`: Future combinators
- `futures-util`: Async middleware implementation utilities
- `log`: Logging facade
- `tracing`: Structured logging support
- `moosicbox_assert`: Assertion utilities
- `switchy_time`: Time measurement utilities

This package provides request logging middleware and service information extractors for MoosicBox web services.
