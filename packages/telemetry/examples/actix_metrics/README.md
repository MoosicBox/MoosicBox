# Actix Web with OpenTelemetry Metrics Example

A comprehensive example demonstrating how to integrate OpenTelemetry tracing and metrics into an Actix web application using switchy_telemetry.

## Summary

This example shows how to set up a complete Actix web server with OpenTelemetry integration, including request tracing middleware, metrics collection, and a metrics endpoint.

## What This Example Demonstrates

- Initializing OpenTelemetry tracing for a web service
- Adding `RequestTracing` middleware to automatically trace HTTP requests
- Adding request metrics middleware for collecting HTTP metrics
- Serving telemetry metrics via a `/metrics` endpoint
- Instrumenting request handlers with the `#[instrument]` attribute
- Creating nested spans for operations within request handlers
- Logging structured events within spans
- Returning both plain text and JSON responses with tracing

## Prerequisites

- Basic understanding of Actix web framework
- Familiarity with async Rust and tokio
- Understanding of HTTP request/response cycles
- Optional: OpenTelemetry collector (Jaeger, Tempo) for viewing traces

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/telemetry/examples/actix_metrics/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/telemetry/examples/actix_metrics
cargo run
```

## Expected Output

When you start the server, you should see:

```
=== Actix Web with OpenTelemetry Example ===

1. Initializing OpenTelemetry tracing...
   ✓ Tracer initialized successfully

2. Configuring tracing subscriber...
   ✓ Subscriber configured

3. Creating HTTP metrics handler...
   ✓ Metrics handler created

4. Starting HTTP server on 127.0.0.1:8080...
   ✓ Server starting

=== Server Ready ===
Try these endpoints:
  • http://127.0.0.1:8080/          - Simple greeting
  • http://127.0.0.1:8080/hello/Bob - Personalized greeting
  • http://127.0.0.1:8080/data      - JSON response example
  • http://127.0.0.1:8080/metrics   - Telemetry metrics endpoint

Press Ctrl+C to stop the server
```

## Code Walkthrough

### Server Setup

```rust
let tracer_layer = switchy_telemetry::init_tracer("actix-metrics-example")
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

tracing_subscriber::registry()
    .with(tracer_layer)
    .with(tracing_subscriber::fmt::layer())
    .init();
```

First, we initialize the OpenTelemetry tracer and set up the tracing subscriber. The `fmt::layer()` allows us to see logs in the console while also exporting spans to OpenTelemetry.

### Metrics Handler

```rust
let metrics_handler = Arc::new(get_http_metrics_handler());
```

The `get_http_metrics_handler()` function returns a handler that provides both the `/metrics` endpoint implementation and the request metrics middleware.

### Middleware Configuration

```rust
App::new()
    .wrap(middleware::Logger::default())
    .wrap(RequestTracing::new())
    .wrap(metrics_handler.request_middleware())
    .app_data(web::Data::new(metrics_handler.clone()))
    .service(metrics)
```

We add three key middleware components:

1. **Logger** - Logs each request/response (standard Actix middleware)
2. **RequestTracing** - Creates OpenTelemetry spans for each request
3. **Request Metrics** - Collects metrics about requests (duration, status codes, etc.)

The metrics handler is added to app data so the `/metrics` endpoint can access it.

### Instrumented Handlers

```rust
#[get("/hello/{name}")]
#[instrument(skip(req))]
async fn hello(req: HttpRequest, name: web::Path<String>) -> HttpResponse {
    info!(name = %name, "Handling personalized greeting");
    simulate_work(&name).await;
    // ...
}
```

The `#[instrument]` attribute creates a span for each request handler. We use `skip(req)` to avoid including the entire request object in the span (which would be verbose). Function parameters like `name` are automatically captured as span attributes.

### Nested Spans

```rust
#[instrument]
async fn simulate_work(name: &str) {
    info!("Simulating work for {}", name);
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}
```

When an instrumented function calls another instrumented function, a parent-child span relationship is automatically created, showing the operation hierarchy.

## Key Concepts

### Request Tracing Middleware

The `RequestTracing` middleware automatically creates a span for each incoming HTTP request. This span includes:

- Request method (GET, POST, etc.)
- Request path
- Request duration
- Response status code

All operations performed during the request become child spans of this root request span.

### Request Metrics

The request metrics middleware collects quantitative data about HTTP requests:

- Request count by endpoint
- Request duration histograms
- Status code distribution
- Active request count

### The /metrics Endpoint

The `/metrics` endpoint is provided by switchy_telemetry and serves telemetry data. In simulator mode, it returns a stub response. In production mode (without the simulator feature), it can export metrics in formats compatible with Prometheus and other monitoring tools.

### Span Attributes

When using `#[instrument]`, function parameters become span attributes. These attributes allow you to:

- Filter traces by specific values (e.g., find all requests for user "Bob")
- Analyze patterns in your data
- Debug issues with specific inputs

### Structured Logging

```rust
info!(
    method = %req.method(),
    path = %req.path(),
    name = %name,
    "Handling personalized greeting"
);
```

Using structured logging with tracing allows you to attach rich context to log events. These events are part of the span and exported to your OpenTelemetry backend.

## Testing the Example

### Basic Testing

1. Start the server:

```bash
cargo run
```

2. In another terminal, test the endpoints:

```bash
# Test the index endpoint
curl http://127.0.0.1:8080/

# Test personalized greeting
curl http://127.0.0.1:8080/hello/Alice

# Test JSON data endpoint
curl http://127.0.0.1:8080/data

# View metrics endpoint
curl http://127.0.0.1:8080/metrics
```

### Load Testing

Test with multiple concurrent requests:

```bash
# Using Apache Bench
ab -n 100 -c 10 http://127.0.0.1:8080/hello/LoadTest

# Using wrk
wrk -t4 -c100 -d30s http://127.0.0.1:8080/data
```

Watch the console output to see the traces being generated for each request.

### With Real OpenTelemetry Collector

To visualize traces in a real OpenTelemetry backend:

1. Start Jaeger with OTLP support:

```bash
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

2. Modify `Cargo.toml` to disable simulator mode:

```toml
[dependencies]
switchy_telemetry = { workspace = true, features = ["actix"] }
```

3. Run the server:

```bash
OTEL_ENDPOINT=http://127.0.0.1:4317 cargo run
```

4. Make some requests to generate traces

5. View traces in Jaeger: http://localhost:16686

You'll see:

- Service map showing request flow
- Individual traces with timing information
- Span hierarchies showing which functions were called
- Span attributes showing request parameters

## Troubleshooting

### Port already in use

If port 8080 is already in use, you can change it in the code:

```rust
let bind_address = "127.0.0.1:3000";  // Use port 3000 instead
```

### Middleware order matters

The order of middleware in Actix web matters. They are executed in reverse order of registration:

```rust
.wrap(A)  // Executed third
.wrap(B)  // Executed second
.wrap(C)  // Executed first
```

For tracing, ensure `RequestTracing` is registered before other middleware that you want to trace.

### "Connection refused" with real collector

Ensure:

- The OpenTelemetry collector is running and accessible
- The `OTEL_ENDPOINT` environment variable points to the correct address
- Network connectivity allows connections to port 4317

### Spans not appearing

Check that:

- The `#[instrument]` attribute is properly imported from the `tracing` crate
- The tracing subscriber is initialized before the server starts
- Simulator mode is disabled when using a real collector

## Related Examples

- **basic_tracing** - Demonstrates basic OpenTelemetry tracing without web framework
- See the switchy_telemetry README.md for additional inline examples and use cases
