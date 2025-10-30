# Basic OpenTelemetry Tracing Example

A comprehensive example demonstrating how to initialize and use OpenTelemetry tracing with the switchy_telemetry package.

## Summary

This example shows how to set up OpenTelemetry tracing, create instrumented functions, and manually create spans for distributed tracing in your Rust applications.

## What This Example Demonstrates

- Initializing an OpenTelemetry tracer with a service name
- Configuring a tracing subscriber with the OpenTelemetry layer
- Using the `#[instrument]` attribute for automatic span creation
- Creating nested spans to represent call hierarchies
- Manual span creation for fine-grained control
- Recording events and logs within spans
- Simulator mode for testing without external dependencies

## Prerequisites

- Basic understanding of Rust async programming
- Familiarity with the tracing crate
- Optional: OpenTelemetry collector (Jaeger, Tempo, etc.) for viewing traces

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/telemetry/examples/basic_tracing/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/telemetry/examples/basic_tracing
cargo run
```

## Expected Output

You should see output similar to:

```
=== Basic OpenTelemetry Tracing Example ===

1. Initializing OpenTelemetry tracer...
   ✓ Tracer initialized successfully

2. Setting up tracing subscriber...
   ✓ Subscriber configured

3. Calling instrumented functions...
INFO instrumented_function{name="Alice" value=42}: Processing data for user: Alice, value: 42
INFO instrumented_function{name="Alice" value=42}: Completed processing for Alice

4. Demonstrating nested spans...
INFO process_request{request_id=123}: Starting request processing
INFO process_request{request_id=123}:validate_request{request_id=123}: Validating request
INFO process_request{request_id=123}:validate_request{request_id=123}: Request 123 is valid
INFO process_request{request_id=123}:execute_operation{request_id=123}: Executing operation
INFO process_request{request_id=123}:execute_operation{request_id=123}: Operation completed for request 123
INFO process_request{request_id=123}: Request processed successfully

5. Creating manual spans...
INFO manual_operation: Inside manual parent span
INFO manual_operation:manual_child_task: Inside manual child span
INFO manual_operation: Back in parent span

6. Simulating concurrent operations...
INFO simulate_concurrent_work: Starting concurrent work simulation
INFO simulate_concurrent_work:process_item{item_id=1}: Processing item 1
INFO simulate_concurrent_work:process_item{item_id=1}: Item 1 processed
INFO simulate_concurrent_work:process_item{item_id=2}: Processing item 2
INFO simulate_concurrent_work:process_item{item_id=2}: Item 2 processed
INFO simulate_concurrent_work:process_item{item_id=3}: Processing item 3
INFO simulate_concurrent_work:process_item{item_id=3}: Item 3 processed
INFO simulate_concurrent_work: Concurrent work completed

=== Example completed successfully ===

Note: Spans are exported to the OTLP endpoint configured via OTEL_ENDPOINT
      Default endpoint: http://127.0.0.1:4317
      Use a tool like Jaeger or Grafana Tempo to visualize traces
```

## Code Walkthrough

### Initializing the Tracer

```rust
let tracer_layer = init_tracer("basic-tracing-example")?;
```

The `init_tracer()` function creates an OpenTelemetry tracer layer. In simulator mode (enabled by default in this example), it returns a no-op layer for testing. In production, it would export spans to an OTLP endpoint.

### Setting Up the Subscriber

```rust
tracing_subscriber::registry()
    .with(tracer_layer)
    .with(tracing_subscriber::fmt::layer())
    .init();
```

We configure the tracing subscriber with both the OpenTelemetry layer (for trace export) and a fmt layer (for console output). This allows us to see traces in real-time while also exporting them.

### Automatic Instrumentation

```rust
#[instrument]
fn instrumented_function(name: &str, value: i32) {
    info!("Processing data for user: {}, value: {}", name, value);
    // Function body
}
```

The `#[instrument]` attribute automatically creates a span for the function. Function arguments are captured as span attributes, which helps with filtering and analyzing traces.

### Nested Spans

```rust
#[instrument]
fn process_request(request_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    validate_request(request_id)?;  // Creates child span
    execute_operation(request_id)?; // Creates another child span
    Ok(())
}
```

When instrumented functions call other instrumented functions, a parent-child span relationship is automatically created, representing the call hierarchy.

### Manual Span Creation

```rust
let span = span!(Level::INFO, "manual_operation", operation_type = "demo");
let _enter = span.enter();
```

For cases where `#[instrument]` isn't suitable, you can manually create spans with custom names and attributes. The `_enter` guard ensures the span is active for the current scope.

## Key Concepts

### OpenTelemetry Tracing

OpenTelemetry is an observability framework for distributed tracing, metrics, and logs. It provides a vendor-neutral way to collect telemetry data from your applications.

### Spans

A span represents a unit of work in a distributed system. Spans have:

- A name describing the operation
- Start and end timestamps
- Attributes (key-value pairs)
- Links to parent and child spans
- Events (timestamped logs)

### Trace Context Propagation

In distributed systems, trace context (trace ID, span ID) is propagated across service boundaries, allowing you to follow a request through multiple services.

### OTLP (OpenTelemetry Protocol)

OTLP is the protocol used to export telemetry data to collectors. This example uses gRPC-based OTLP export (when not in simulator mode).

### Simulator Mode

The `simulator` feature flag enables testing without requiring an external OpenTelemetry collector. Perfect for development and testing environments.

## Testing the Example

### With Simulator Mode (Default)

The example runs in simulator mode by default, which means spans are created but not exported. This is perfect for learning and testing.

### With Real OpenTelemetry Collector

To test with a real collector:

1. Start an OpenTelemetry collector (e.g., Jaeger):

```bash
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

2. Modify `Cargo.toml` to disable simulator mode:

```toml
[dependencies]
switchy_telemetry = { workspace = true }  # Remove "simulator" feature
```

3. Run the example with the OTLP endpoint:

```bash
OTEL_ENDPOINT=http://127.0.0.1:4317 cargo run
```

4. View traces in Jaeger UI: http://localhost:16686

## Troubleshooting

### "Connection refused" error

If you see connection errors when not using simulator mode, ensure:

- The OTLP endpoint is running and accessible
- The `OTEL_ENDPOINT` environment variable is set correctly
- Firewall rules allow connections to the endpoint

### Spans not appearing in collector

Check that:

- Simulator mode is disabled in production
- The service name matches what you're searching for
- The collector is configured to receive OTLP over gRPC on port 4317
- Network connectivity between your app and the collector is working

### Build errors

Ensure all dependencies are available:

```bash
cargo clean
cargo update
cargo build
```

## Related Examples

- **actix_metrics** - Demonstrates Actix web integration with HTTP metrics (if available)
- See the switchy_telemetry README.md for additional inline examples
