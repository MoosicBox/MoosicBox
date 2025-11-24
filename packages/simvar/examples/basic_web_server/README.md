# Basic Web Server Simulation Example

This example demonstrates how to use simvar with moosicbox_web_server to create a basic web server simulation.

## Summary

A foundational example showing how to set up a simulated web server with multiple concurrent clients, demonstrating the core concepts of host/client actors in simvar's deterministic testing framework.

## What This Example Demonstrates

- Setting up a web server using `moosicbox_web_server`
- Creating simulation hosts and clients with `simvar`
- Handling HTTP requests and responses
- Time-based simulation with configurable duration
- Error handling and logging
- CORS configuration
- JSON serialization/deserialization
- Deterministic concurrent testing of web services

## Prerequisites

Before running this example, you should be familiar with:

- Basic Rust async/await programming
- HTTP request/response concepts
- Understanding of client-server architecture
- Familiarity with the simvar simulation framework (see package README)

## Endpoints

The web server provides these endpoints:

- `GET /api/v1/health` - Health check endpoint
- `GET /api/v1/status` - Server status with uptime information
- `POST /api/v1/echo` - Echo endpoint that accepts JSON and returns a response

## Running the Example

```bash
# From the MoosicBox root directory
cargo run --manifest-path packages/simvar/examples/basic_web_server/Cargo.toml

# Or using the package name
cargo run -p simvar_basic_web_server_example

# With informational logging
RUST_LOG=info cargo run -p simvar_basic_web_server_example

# With detailed debug logging
RUST_LOG=debug cargo run -p simvar_basic_web_server_example
```

## Expected Output

The simulation runs for 10 seconds and produces output showing:

```
=== SIMULATION RESULTS ===
SimResult { ... }

Success rate: 4/4
```

Key indicators in the output:

- **Success/failure statistics**: Shows how many actors (hosts + clients) completed successfully
- **Simulation results**: Details about each actor's execution
- **Success rate**: Ratio of successful completions (should be 4/4: 1 server + 3 clients)

With `RUST_LOG=info` or `RUST_LOG=debug`, you'll also see:

- Server startup messages
- Client request logs showing status codes
- Request counts and timing information
- Graceful shutdown messages

## Code Walkthrough

### 1. Bootstrap Configuration

The `BasicWebServerBootstrap` struct configures the simulation parameters:

```rust
struct BasicWebServerBootstrap {
    server_port: u16,        // Port for the web server (default: 8080)
    client_count: usize,     // Number of client actors (default: 3)
    request_interval: Duration, // Time between requests (default: 500ms)
}
```

### 2. Implementing SimBootstrap

The `SimBootstrap` trait provides lifecycle hooks:

```rust
impl SimBootstrap for BasicWebServerBootstrap {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(10); // 10-second simulation
        config.enable_random_order = true;         // Enable actor randomization
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        // Create the web server host
        sim.host("web-server", move || {
            Box::pin(async move { start_web_server(server_port).await })
        });

        // Create multiple client actors
        for i in 0..self.client_count {
            sim.client(format!("client-{}", i + 1), async move {
                run_client(client_id, server_port, request_interval).await
            });
        }
    }
}
```

### 3. Web Server Host

The server uses `moosicbox_web_server` to define HTTP endpoints:

```rust
let server = WebServerBuilder::new()
    .with_port(port)
    .with_cors(cors)
    .with_scope(
        Scope::new("/api/v1")
            .get("/health", |_req| { /* Health check logic */ })
            .get("/status", |_req| { /* Status with uptime */ })
            .post("/echo", |_req| { /* Echo with timestamp */ })
    )
    .build();
```

### 4. Client Actors

Each client makes rotating requests using the simulated HTTP client:

```rust
loop {
    if simvar::utils::is_simulator_cancelled() {
        break; // Graceful shutdown on simulation end
    }

    match request_count % 3 {
        0 => { /* Health check request */ },
        1 => { /* Status request */ },
        _ => { /* Echo request with JSON payload */ },
    }

    switchy_async::time::sleep(request_interval).await;
}
```

### 5. Data Models

The example uses typed request/response structures:

```rust
#[derive(Serialize, Deserialize)]
struct EchoRequest {
    message: String,
    timestamp: u64,
}

#[derive(Serialize, Deserialize)]
struct StatusResponse {
    status: String,
    uptime_seconds: u64,
    requests_served: u64,
}
```

## Key Concepts

### SimBootstrap

The `SimBootstrap` trait provides lifecycle hooks for configuring and managing the simulation:

- **`build_sim`**: Configures simulation parameters (duration, randomization, etc.)
- **`on_start`**: Creates initial hosts and clients
- **`on_step`**: Optional per-step logic (not used in this example)
- **`on_end`**: Cleanup logic when simulation completes
- **`props`**: Metadata about the simulation configuration

### Host Actor

A **host** is a long-running actor that persists for the simulation duration. In this example, the web server is a host that:

- Starts up and binds to a port
- Continuously handles incoming HTTP requests
- Runs until the simulation ends

Hosts are created with `sim.host(name, factory_fn)`.

### Client Actors

**Clients** are ephemeral actors that perform specific tasks and complete. In this example, each client:

- Makes periodic HTTP requests to the server
- Runs until the simulation ends or cancellation is detected
- Reports completion status

Clients are created with `sim.client(name, async_task)`.

### Deterministic Simulation

Simvar provides deterministic execution by controlling:

- **Time**: `switchy_async::time::sleep()` advances simulated time, not real time
- **Randomness**: Controlled random order of actor execution when enabled
- **Concurrency**: Predictable scheduling of async tasks

This allows reproducible test results across multiple runs.

### Graceful Cancellation

Clients check `simvar::utils::is_simulator_cancelled()` to detect when the simulation duration expires, allowing graceful shutdown instead of abrupt termination.

## Testing the Example

To verify the example works correctly:

1. **Run the simulation** and check that the success rate is 4/4
2. **Enable logging** with `RUST_LOG=debug` to see request/response details
3. **Modify parameters** in `BasicWebServerBootstrap::new()` to experiment:
    - Change `server_port` to use a different port
    - Adjust `client_count` to add more clients
    - Modify `request_interval` to change request frequency
4. **Check the duration** by modifying `config.duration` in `build_sim()`

Example modifications to try:

```rust
// In BasicWebServerBootstrap::new()
server_port: 9090,      // Different port
client_count: 5,        // More clients
request_interval: Duration::from_millis(250), // Faster requests

// In build_sim()
config.duration = Duration::from_secs(20); // Longer simulation
```

## Troubleshooting

### Simulation Fails with Port Already in Use

- The default port 8080 may be in use by another service
- Solution: Change `server_port` in `BasicWebServerBootstrap::new()` to an available port (e.g., 8081, 9090)

### Success Rate Less Than 4/4

- Check logs with `RUST_LOG=debug` to see which actor failed
- Review error messages for connection issues or request failures
- Verify the simulation duration is sufficient for all clients to start

### No Request Logs Visible

- Logging requires `RUST_LOG` environment variable
- Use `RUST_LOG=info` for basic logs or `RUST_LOG=debug` for detailed output
- Ensure you're running the correct example package

### Compilation Errors

- Ensure you're in the MoosicBox root directory
- Run `cargo clean` and try again
- Check that all dependencies are available in the workspace

## Related Examples

- **api_testing**: Demonstrates comprehensive API testing with validation (see `packages/simvar/examples/api_testing/`)

For more advanced scenarios, explore extending this example with:

- More complex request patterns (burst traffic, gradual ramp-up)
- Load testing scenarios with configurable client counts
- Database interactions with simulated persistence
- Error injection and recovery testing
- Performance metrics collection
