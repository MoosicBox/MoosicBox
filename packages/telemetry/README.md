# Switchy Telemetry

OpenTelemetry integration for distributed tracing and metrics collection.

## Overview

The Telemetry package provides:

- **OpenTelemetry Integration**: Complete OTLP (OpenTelemetry Protocol) support
- **Distributed Tracing**: Trace propagation and span collection
- **Metrics Collection**: HTTP metrics and custom metrics support
- **Actix Web Integration**: Middleware for web application monitoring
- **Simulator Mode**: Testing support with simulated telemetry
- **Resource Attribution**: Service identification and metadata

## Features

### Tracing System

- **OTLP Export**: Send traces to OpenTelemetry collectors
- **Trace Context Propagation**: W3C Trace Context standard support
- **Batch Export**: Efficient batched span transmission
- **Resource Tagging**: Service name and version attribution
- **Instrumentation Scope**: Proper scope management for spans

### HTTP Metrics

- **Request Metrics**: HTTP request/response monitoring
- **Actix Web Middleware**: Drop-in middleware for request tracing
- **Custom Handlers**: Pluggable metrics collection handlers

### Simulator Support

- **Testing Mode**: Simulate telemetry without external dependencies
- **Mock Handlers**: Test HTTP metrics collection
- **Development Friendly**: Easy local development setup

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_telemetry = { path = "../telemetry" }

# With specific features
switchy_telemetry = {
    path = "../telemetry",
    features = ["actix", "simulator"]
}
```

## Usage

### Initialize Tracing

```rust
use switchy_telemetry::init_tracer;
use moosicbox_logging::free_log_client::DynLayer;

// Initialize OpenTelemetry tracing
let tracer_layer: DynLayer = init_tracer("my-service")?;

// Use with tracing subscriber
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracer_layer)
    .init();
```

### Environment Configuration

```bash
# Set OTLP endpoint (defaults to http://127.0.0.1:4317)
export OTEL_ENDPOINT=http://jaeger:4317

# Or use default local endpoint
# OTEL_ENDPOINT=http://127.0.0.1:4317
```

### Actix Web Integration

```rust
use actix_web::{web, App, HttpServer};
use switchy_telemetry::{RequestTracing, get_http_metrics_handler, metrics};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize telemetry
    let tracer_layer = switchy_telemetry::init_tracer("web-service")
        .map_err(std::io::Error::other)?;

    // Create metrics handler
    let metrics_handler = std::sync::Arc::new(get_http_metrics_handler());

    HttpServer::new(move || {
        App::new()
            // Add request tracing middleware
            .wrap(RequestTracing::new())
            .wrap(metrics_handler.request_middleware())
            // Add metrics endpoint
            .app_data(web::Data::new(metrics_handler.clone()))
            .service(metrics)
            .service(my_handler)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

#[tracing::instrument]
async fn my_handler() -> &'static str {
    "Hello, World!"
}
```

### Custom HTTP Metrics Handler

```rust
use switchy_telemetry::HttpMetricsHandler;
use actix_web::{HttpRequest, HttpResponse};
use actix_web_opentelemetry::RequestMetrics;
use futures_util::future::LocalBoxFuture;

#[derive(Debug)]
struct CustomMetricsHandler;

impl HttpMetricsHandler for CustomMetricsHandler {
    fn call(
        &self,
        _request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<HttpResponse<String>, actix_web::error::Error>> {
        Box::pin(async {
            // Custom metrics collection logic
            let metrics_data = collect_custom_metrics().await;
            Ok(HttpResponse::with_body(actix_web::http::StatusCode::OK, metrics_data))
        })
    }

    fn request_middleware(&self) -> RequestMetrics {
        RequestMetrics::builder().build()
    }
}

async fn collect_custom_metrics() -> String {
    // Implement custom metrics collection
    "# Custom metrics\n".to_string()
}
```

### Resource Attribution

```rust
use switchy_telemetry::get_resource_attr;
use opentelemetry::KeyValue;

// Create resource attributes for service identification
let resource = get_resource_attr("my-service");

// Resource includes:
// - service.name: "my-service"
// - Additional service metadata
```

### Manual Tracing

```rust
use tracing::{info, instrument, span, Level};

#[instrument]
async fn process_request(user_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Processing request for user {}", user_id);

    let span = span!(Level::INFO, "database_query", user_id = user_id);
    let _enter = span.enter();

    // Database operation
    query_user_data(user_id).await?;

    info!("Request processed successfully");
    Ok(())
}

#[instrument]
async fn query_user_data(user_id: u64) -> Result<UserData, DatabaseError> {
    // Database query with automatic span creation
    Ok(UserData::default())
}
```

### Simulator Mode

```rust
// Enable simulator mode for testing
#[cfg(feature = "simulator")]
use switchy_telemetry::init_tracer;

// In tests or development
let tracer_layer = init_tracer("test-service")?;
// Uses simulator instead of real OTLP export
```

## Feature Flags

- **`actix`**: Enable Actix Web middleware and HTTP metrics
- **`simulator`**: Enable simulator mode for testing

## Configuration

### Environment Variables

- **`OTEL_ENDPOINT`**: OpenTelemetry collector endpoint (default: `http://127.0.0.1:4317`)

### Default Configuration

- **Protocol**: OTLP over gRPC (using Tonic)
- **Propagator**: W3C Trace Context
- **Export**: Batch exporter for efficiency
- **Schema**: OpenTelemetry schema v1.2.0

## Metrics Endpoint

The package provides a `/metrics` endpoint when using Actix Web:

```rust
use switchy_telemetry::metrics;

// Add to your Actix Web app
.service(metrics)
```

Access metrics at: `http://localhost:8080/metrics`

## Error Handling

```rust
use switchy_telemetry::init_tracer;

match init_tracer("my-service") {
    Ok(layer) => {
        // Use tracer layer
    }
    Err(e) => {
        eprintln!("Tracer initialization failed: {}", e);
    }
}
```

## Dependencies

- **OpenTelemetry**: Core tracing functionality
- **OpenTelemetry OTLP**: OTLP exporter implementation
- **Tracing OpenTelemetry**: Bridge between tracing and OpenTelemetry
- **Actix Web OpenTelemetry**: Actix Web middleware (optional)
- **MoosicBox Logging**: Logging integration

## Integration Points

- **Jaeger**: Compatible with Jaeger OTLP receiver
- **Zipkin**: Compatible with Zipkin OTLP receiver
- **OpenTelemetry Collector**: Direct OTLP export
- **Grafana**: Visualization of traces (via Tempo or other OTLP-compatible backends)

## Use Cases

- **Microservices Monitoring**: Distributed tracing across services
- **Performance Analysis**: Request latency and throughput monitoring
- **Error Tracking**: Exception and error rate monitoring
- **Service Dependencies**: Understand service interaction patterns
- **Load Testing**: Monitor system behavior under load
