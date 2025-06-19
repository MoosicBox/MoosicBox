# MoosicBox Middleware

Basic HTTP middleware collection for the MoosicBox web server ecosystem, providing request logging and service information utilities for Actix Web applications.

## Features

- **API Logger**: Request/response logging middleware with timing and status tracking
- **Service Info**: Middleware for adding service information to responses
- **Tunnel Info**: Optional middleware for tunnel-based requests

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_middleware = "0.1.1"

# Enable tunnel middleware
moosicbox_middleware = { version = "0.1.1", features = ["tunnel"] }
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
- Relevant headers logging (Range, Content-Range, Accept-Ranges, Content-Length)
- Response status and timing information
- Success/failure status tracking
- Error details for failed requests

### Service Info Middleware

```rust
use moosicbox_middleware::service_info::ServiceInfo;
use actix_web::{web, App, HttpServer};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(ServiceInfo::new("My Music Service", "1.0.0"))
            .route("/api/status", web::get().to(status_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn status_handler() -> HttpResponse {
    HttpResponse::Ok().json("Service is running")
}
```

### Tunnel Info Middleware (Optional)

When the `tunnel` feature is enabled:

```rust
use moosicbox_middleware::tunnel_info::TunnelInfo;
use actix_web::{web, App, HttpServer};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(TunnelInfo::new())
            .route("/tunnel/api", web::get().to(tunnel_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn tunnel_handler() -> HttpResponse {
    HttpResponse::Ok().json("Tunnel endpoint")
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

The `ServiceInfo` middleware adds service metadata to responses for identification and monitoring purposes.

### TunnelInfo

The `TunnelInfo` middleware handles tunnel-specific request processing when tunnel features are enabled.

## Core Components

```rust
// API Logger
pub struct ApiLogger;
pub struct ApiLoggerMiddleware<S>;

// Service Info
pub struct ServiceInfo;
pub struct ServiceInfoMiddleware<S>;

// Tunnel Info (feature-gated)
#[cfg(feature = "tunnel")]
pub struct TunnelInfo;
#[cfg(feature = "tunnel")]
pub struct TunnelInfoMiddleware<S>;
```

## Dependencies

- `actix_web`: Web framework for middleware integration
- `futures-util`: For async middleware implementation
- `log`: For logging functionality
- `tracing`: For structured logging support

This middleware collection provides essential request logging and service identification capabilities for MoosicBox web services.
