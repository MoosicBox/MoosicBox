# Basic Web Server Simulation Example

This example demonstrates the fundamentals of using simvar with moosicbox_web_server to create a deterministic web server simulation.

## Summary

This is a beginner-friendly introduction to simvar that creates a simple HTTP server simulation with multiple client actors making periodic requests. It showcases the core concepts of host/client actors, simulation time, and basic HTTP request/response handling in a controlled, reproducible environment.

## What This Example Demonstrates

- Setting up a web server using `moosicbox_web_server` within a simvar simulation
- Creating persistent host actors (the web server) and ephemeral client actors
- Handling HTTP requests and responses across multiple endpoints
- Using simulation time for deterministic, time-based testing
- Configuring CORS and server settings
- JSON serialization/deserialization for API responses
- Error handling and logging in simulated environments
- Basic metrics collection (request counts, success/failure rates)

## Prerequisites

- Rust toolchain (see `rust-toolchain.toml` in project root)
- Basic understanding of HTTP concepts (GET/POST requests, status codes)
- Familiarity with async Rust and the tokio runtime
- Understanding of web server fundamentals

## Endpoints

The web server provides these endpoints:

- `GET /api/v1/health` - Health check endpoint
- `GET /api/v1/status` - Server status with uptime information
- `POST /api/v1/echo` - Echo endpoint that accepts JSON and returns a response

## Running the Example

From the MoosicBox root directory:

```bash
# Basic run
cargo run --manifest-path packages/simvar/examples/basic_web_server/Cargo.toml

# Or using the package name
cargo run -p simvar_basic_web_server_example

# With info-level logging
RUST_LOG=info cargo run -p simvar_basic_web_server_example

# With debug logging for detailed output
RUST_LOG=debug cargo run -p simvar_basic_web_server_example
```

## Configuration

The simulation can be configured by modifying the `BasicWebServerBootstrap`:

- `server_port`: Port for the web server (default: 8080)
- `client_count`: Number of client actors (default: 3)
- `request_interval`: Time between client requests (default: 500ms)
- `duration`: Total simulation time (default: 10 seconds)

## Expected Output

The simulation runs for 10 seconds of simulated time and produces output including:

```
=== BASIC WEB SERVER SIMULATION RESULTS ===
Run 1: Success
  Duration: 10.0s
  Server: Running on port 8080
  Clients: 3 actors
  Total Requests: ~60 (3 clients × 2 requests/sec × 10 seconds)
  Success Rate: 100%
```

With `RUST_LOG=info` or `RUST_LOG=debug`, you'll see:

- Server startup messages
- Client connection attempts
- Individual HTTP requests and responses
- Request timing information
- Final statistics and cleanup

## Code Walkthrough

### 1. Main Entry Point

The `main()` function creates the bootstrap and runs the simulation:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bootstrap = BasicWebServerBootstrap::new();
    let results = run_simulation(bootstrap)?;

    println!("\n=== SIMULATION RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    Ok(())
}
```

### 2. Bootstrap Configuration

`BasicWebServerBootstrap` implements the `SimBootstrap` trait:

```rust
struct BasicWebServerBootstrap {
    server_port: u16,
    client_count: usize,
    request_interval: Duration,
}

impl SimBootstrap for BasicWebServerBootstrap {
    fn build_sim(&self, config: SimConfig) -> SimConfig {
        config.with_duration(Duration::from_secs(10))
    }

    fn on_start(&self, sim: &mut impl Sim) {
        // Spawn host and client actors
    }
}
```

### 3. Creating the Server Host

The server is spawned as a persistent host actor:

```rust
sim.host("web-server", || {
    Box::pin(async move {
        let builder = WebServerBuilder::default()
            .with_port(8080)
            .with_scope(create_routes());

        builder.serve().await?;
        Ok(())
    })
});
```

### 4. Creating Client Actors

Multiple client actors make periodic requests:

```rust
for i in 0..client_count {
    sim.client(format!("client-{}", i), async move {
        loop {
            // Make HTTP request
            let response = client
                .request(Method::Get, "http://127.0.0.1:8080/api/v1/health")
                .send()
                .await?;

            // Process response
            if response.status().is_success() {
                log::info!("Request succeeded");
            }

            // Wait before next request
            switchy_time::sleep(request_interval).await;
        }
    });
}
```

### 5. Defining Routes

Routes are created using the `moosicbox_web_server::route!` macro:

```rust
fn create_routes() -> Scope {
    Scope::new("/api/v1")
        .route(health_endpoint())
        .route(status_endpoint())
        .route(echo_endpoint())
}

moosicbox_web_server::route!(GET, health_endpoint, "/health", |_req| {
    Box::pin(async move {
        let body = json!({"status": "healthy"}).to_string();
        Ok(HttpResponse::ok().with_body(body))
    })
});
```

## Key Concepts

### SimBootstrap Trait

The `SimBootstrap` trait configures simulation lifecycle:

- **`build_sim()`**: Sets simulation parameters (duration, seed, etc.)
- **`on_start()`**: Initializes hosts and clients when simulation begins
- **`on_end()`**: Cleanup logic when simulation completes (optional)

### Host vs. Client Actors

- **Host actors**: Persistent services that run for the simulation duration (e.g., web servers, databases)
- **Client actors**: Ephemeral entities that make requests and can complete or run indefinitely

### Simulation Time

Simvar uses **deterministic time simulation**:

- `switchy_time::now()` - Current simulation time
- `switchy_time::sleep()` - Sleep in simulation time (not real time)
- Time advances in a controlled, reproducible manner
- Tests run faster than real-time while maintaining timing relationships

### Deterministic Execution

Key benefits of simvar's deterministic execution:

- **Reproducible**: Same seed produces identical results
- **Fast**: Simulations run faster than real-time
- **Controllable**: Adjust time, randomness, and execution order
- **Debuggable**: Consistent behavior makes debugging easier

## Testing the Example

### Basic Verification

Run the example and verify:

1. Simulation completes without errors
2. All client requests succeed (100% success rate)
3. Server responds to all three endpoints
4. Simulation runs for exactly 10 seconds (simulation time)

### Experimenting with Configuration

Try modifying the bootstrap parameters:

```rust
BasicWebServerBootstrap {
    server_port: 8081,      // Change port
    client_count: 5,        // More clients
    request_interval: Duration::from_millis(250), // Faster requests
}
```

### Adding Debug Output

Enable detailed logging to see request flow:

```bash
RUST_LOG=simvar=debug,moosicbox_web_server=info cargo run -p simvar_basic_web_server_example
```

## Troubleshooting

### Port Already in Use

**Problem**: Error binding to port 8080

**Solutions**:

- Change `server_port` in `BasicWebServerBootstrap::new()`
- Check for other services: `lsof -i :8080` (Linux/macOS)
- Use a different port like 8081 or 8082

### Compilation Errors

**Problem**: Example doesn't compile

**Solutions**:

- Update Rust toolchain: `rustup update`
- Clean and rebuild: `cargo clean && cargo build -p simvar_basic_web_server_example`
- Check workspace dependencies: `cargo check`

### Simulation Hangs

**Problem**: Simulation doesn't complete

**Solutions**:

- Check for infinite loops in client code
- Verify server starts successfully (enable `RUST_LOG=debug`)
- Ensure simulation duration is set in `build_sim()`

### No Output

**Problem**: Simulation runs but produces no output

**Solutions**:

- Enable logging: `RUST_LOG=info cargo run -p simvar_basic_web_server_example`
- Check if clients are successfully making requests
- Verify server endpoints are correctly configured

## Related Examples

- **api_testing**: More advanced example with comprehensive API testing
- **Other simvar examples**: Explore `packages/simvar/examples/` for additional patterns
