# Simvar Examples

This directory contains examples demonstrating how to use simvar for deterministic simulation testing of concurrent systems.

## Overview

These examples showcase different aspects of simulation testing with simvar:

- **Core simulation concepts** - Host/client actors, simulation time, deterministic execution
- **Web server testing** - Basic server operations and client interactions
- **API validation** - REST API endpoint testing and contract validation

## Examples

### 1. Basic Simulation (`basic_simulation/`)

**Difficulty**: Beginner
**Focus**: Core simvar concepts without external dependencies

A minimal simulation demonstrating fundamental concepts:

- Creating persistent host actors and ephemeral client actors
- Using simulation time for deterministic execution
- Sharing state between actors using thread-safe primitives
- Understanding the SimBootstrap lifecycle

**Key Concepts**: SimBootstrap trait, Host/Client actors, Simulation time, Deterministic execution

**Recommended Starting Point**: Start here if you're new to simvar!

### 2. Basic Web Server (`basic_web_server/`)

**Difficulty**: Beginner
**Focus**: Fundamentals of simvar + web server integration

A simple HTTP server simulation that demonstrates:

- Setting up a web server with multiple endpoints
- Creating client actors that make HTTP requests
- Basic request/response handling
- Simulation configuration and metrics

**Key Concepts**: SimBootstrap, Host/Client actors, HTTP routes, basic metrics

### 3. API Testing (`api_testing/`)

**Difficulty**: Intermediate
**Focus**: Comprehensive REST API validation

Complete API testing framework with:

- CRUD operations testing (Create, Read, Update, Delete)
- Multiple test scenarios (happy path, error handling, edge cases, concurrency)
- Detailed test result tracking and reporting
- HTTP status code and response validation

**Key Concepts**: REST API testing, test scenarios, validation patterns, comprehensive reporting

## Getting Started

### Prerequisites

- Rust toolchain (see `rust-toolchain.toml` in project root)
- Basic understanding of async Rust programming
- Familiarity with HTTP concepts (for web server examples)

### Running Examples

Each example can be run independently:

```bash
# From the MoosicBox root directory

# Basic simulation example (recommended starting point)
cargo run -p simvar_basic_simulation_example

# Basic web server example
cargo run -p simvar_basic_web_server_example

# API testing example
cargo run -p simvar_api_testing_example
```

### Logging

Enable detailed logging for better insight:

```bash
# Info level logging
RUST_LOG=info cargo run -p <example_name>

# Debug level logging
RUST_LOG=debug cargo run -p <example_name>

# Specific module logging
RUST_LOG=simvar=debug,moosicbox_web_server=info cargo run -p <example_name>
```

## Learning Path

### Beginner Path

1. **Start with `basic_simulation`** - Learn core concepts without complexity
2. **Understand the fundamentals** - Host/client actors, simulation time, deterministic execution
3. **Experiment with the code** - Modify client counts, durations, and behaviors
4. **Try `basic_web_server`** - Apply concepts to HTTP server testing

### Intermediate Path

1. **Explore `api_testing`** - Understand comprehensive API validation
2. **Create custom test scenarios** - Add your own test cases
3. **Experiment with different features** - Try database, filesystem, or TCP simulations
4. **Build your own simulations** - Apply concepts to your testing needs

## Key Concepts

### SimBootstrap

The bootstrap pattern configures and initializes simulations:

- **`props()`**: Simulation metadata and configuration
- **`build_sim()`**: Simulation parameters (duration, randomization)
- **`on_start()`**: Initialize hosts and clients
- **`on_step()`**: Per-step logic (optional)
- **`on_end()`**: Cleanup and final reporting

### Host Actors

Long-running services that handle requests:

- Web servers that serve HTTP endpoints
- Database servers or external services
- Background processing services
- Monitoring and metrics collection services

### Client Actors

Request-generating entities that interact with hosts:

- HTTP clients making requests
- Load generators creating traffic
- Test clients validating behavior
- Monitoring clients collecting metrics

### Simulation Time

Simvar provides deterministic time simulation:

- **`simvar::switchy::time::now()`**: Current simulation time
- **`simvar::switchy::time::sleep()`**: Async sleep in simulation time
- **`simvar::switchy::time::timeout()`**: Timeout operations
- **Time acceleration**: Simulations run faster than real-time

## Common Patterns

### Request/Response Handling

```rust
// Define request/response types
#[derive(Serialize, Deserialize)]
struct MyRequest { /* fields */ }

#[derive(Serialize, Deserialize)]
struct MyResponse { /* fields */ }

// Create route handler
moosicbox_web_server::route!(POST, my_endpoint, "/api/endpoint", |req| {
    Box::pin(async move {
        // Process request
        let response = MyResponse { /* data */ };
        let body = serde_json::to_string(&response).unwrap();
        Ok(HttpResponse::ok().with_body(body))
    })
});
```

### Metrics Collection

```rust
// Define metrics structure
#[derive(Debug)]
struct Metrics {
    total_requests: u64,
    successful_requests: u64,
    response_times: Vec<u64>,
}

// Record metrics in clients
let start_time = simvar::switchy::time::now();
let result = client.request(Method::Get, &url).send().await;
let response_time = simvar::switchy::time::now()
    .duration_since(start_time)
    .unwrap()
    .as_millis() as u64;

metrics.lock().unwrap().record_request(response_time, result.is_ok());
```

### Error Handling

```rust
// Robust error handling with retries
let mut retry_count = 0;
let max_retries = 3;

while retry_count <= max_retries {
    match client.request(Method::Get, &url).send().await {
        Ok(response) if response.status().is_success() => {
            // Success - break retry loop
            break;
        }
        Ok(_) | Err(_) => {
            retry_count += 1;
            if retry_count <= max_retries {
                // Exponential backoff
                let backoff = Duration::from_millis(100 * (1 << retry_count));
                simvar::switchy::time::sleep(backoff).await;
            }
        }
    }
}
```

## Best Practices

### Simulation Design

- **Start simple** and gradually add complexity
- **Use realistic parameters** based on production data
- **Include both success and failure scenarios**
- **Make simulations deterministic** for reproducible results

### Performance Testing

- **Test multiple load patterns** to understand different scenarios
- **Monitor both client and server metrics**
- **Include error injection** to test resilience
- **Validate SLA compliance** under various conditions

### Code Organization

- **Separate concerns** (bootstrap, metrics, business logic)
- **Use proper error handling** with detailed error messages
- **Follow MoosicBox conventions** (BTreeMap, #[must_use], etc.)
- **Document simulation parameters** and expected outcomes

### Metrics and Monitoring

- **Collect meaningful metrics** that relate to user experience
- **Use appropriate data structures** (BTreeMap for deterministic ordering)
- **Implement proper aggregation** (percentiles, averages, rates)
- **Provide clear reporting** with actionable insights

## Troubleshooting

### Common Issues

**Simulation doesn't start**

- Check port conflicts (ensure ports are available)
- Verify dependencies in Cargo.toml
- Check for compilation errors

**Clients can't connect to server**

- Ensure server starts before clients (use delays)
- Verify correct ports and URLs
- Check for network simulation interference

**Poor performance or timeouts**

- Reduce client count or request frequency
- Increase simulation duration
- Check for resource constraints

**Inconsistent results**

- Ensure deterministic simulation settings
- Use fixed seeds for random number generation
- Avoid real-time dependencies

### Debugging Tips

**Enable detailed logging**

```bash
RUST_LOG=debug cargo run -p <example_name>
```

**Add custom logging**

```rust
log::debug!("Client {} making request to {}", client_id, url);
log::info!("Server processed {} requests", request_count);
```

**Use simulation time consistently**

```rust
// Good - uses simulation time
let now = simvar::switchy::time::now();

// Bad - uses real time
let now = std::time::SystemTime::now();
```

## Contributing

When adding new examples:

1. Follow the established directory structure
2. Include comprehensive README documentation
3. Add proper error handling and logging
4. Follow MoosicBox coding conventions
5. Include both simple and advanced scenarios
6. Provide clear configuration options

## Further Reading

- [Simvar Documentation](../README.md) - Core simulation concepts
- [MoosicBox Web Server Documentation](../../web_server/README.md) - Web server features
- [Switchy Documentation](https://docs.rs/switchy) - Underlying simulation framework
- [Actix Web Documentation](https://actix.rs/) - HTTP server implementation
