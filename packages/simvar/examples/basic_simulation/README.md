# Basic Simulation Example

This example demonstrates the core concepts of simvar without external dependencies like web servers, focusing purely on simulation fundamentals.

## Summary

This beginner-friendly example introduces the fundamental concepts of the simvar simulation framework through a simple scenario: a server that tracks connections from multiple clients. It demonstrates host/client actors, simulation time, shared state, and deterministic execution in the clearest possible way.

## What This Example Demonstrates

- Creating persistent host actors that run for the simulation duration
- Spawning multiple ephemeral client actors that perform tasks
- Using simulation time (`switchy_time`) for deterministic, reproducible execution
- Sharing state between actors using thread-safe primitives (`Arc<AtomicU32>`)
- Implementing the `SimBootstrap` trait to configure simulations
- Lifecycle hooks: `build_sim()`, `on_start()`, and `on_end()`
- Staggering actor start times for realistic behavior
- Logging and observing simulation execution

## Prerequisites

- Rust toolchain (see `rust-toolchain.toml` in project root)
- Basic understanding of async Rust and tokio
- Familiarity with Rust's `Arc` and atomic types
- No external dependencies required (no web servers, databases, etc.)

## Running the Example

From the MoosicBox root directory:

```bash
# Basic run
cargo run --manifest-path packages/simvar/examples/basic_simulation/Cargo.toml

# Or using the package name
cargo run -p simvar_basic_simulation_example

# With info-level logging (recommended)
RUST_LOG=info cargo run -p simvar_basic_simulation_example

# With debug logging for detailed output
RUST_LOG=debug cargo run -p simvar_basic_simulation_example
```

## Expected Output

The simulation runs for 5 seconds of simulated time and produces output like:

```
[INFO] Server starting up...
[INFO] Client 0 starting...
[INFO] Client 1 starting...
[INFO] Client 2 starting...
[INFO] Client 0: Connection #0 (total connections: 1)
[INFO] Client 1: Connection #0 (total connections: 2)
[INFO] Client 2: Connection #0 (total connections: 3)
[DEBUG] Client 0: Disconnected (remaining connections: 2)
[DEBUG] Client 1: Disconnected (remaining connections: 1)
[DEBUG] Client 2: Disconnected (remaining connections: 0)
[INFO] Client 0: Connection #1 (total connections: 1)
...
[INFO] Client 0 completed all connections
[INFO] Client 1 completed all connections
[INFO] Client 2 completed all connections

=== FINAL STATISTICS ===
Total clients: 3
Active connections at end: 0
(All clients should have disconnected)

=== SIMULATION RESULTS ===
Run 1: Success
```

## Code Walkthrough

### 1. Main Entry Point

The `main()` function sets up logging and runs the simulation:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for visibility
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    // Create bootstrap configuration
    let bootstrap = BasicSimulationBootstrap::new();

    // Run the simulation and collect results
    let results = run_simulation(bootstrap)?;

    // Display results
    println!("\n=== SIMULATION RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    Ok(())
}
```

### 2. Bootstrap Configuration

The bootstrap struct holds configuration and shared state:

```rust
struct BasicSimulationBootstrap {
    client_count: usize,
    connection_counter: Arc<AtomicU32>,
}

impl BasicSimulationBootstrap {
    fn new() -> Self {
        Self {
            client_count: 3,
            connection_counter: Arc::new(AtomicU32::new(0)),
        }
    }
}
```

### 3. Simulation Configuration

The `build_sim()` method configures simulation parameters:

```rust
fn build_sim(&self, config: SimConfig) -> SimConfig {
    config.with_duration(Duration::from_secs(5))
}
```

This sets the simulation to run for exactly 5 seconds of simulated time.

### 4. Creating the Host Actor

The `on_start()` method spawns a persistent server host:

```rust
sim.host("server", move || {
    let counter = Arc::clone(&counter);

    Box::pin(async move {
        log::info!("Server starting up...");

        loop {
            time::sleep(Duration::from_millis(500)).await;

            let count = counter.load(Ordering::Relaxed);
            log::debug!("Server: Current connection count = {count}");
        }

        Ok::<(), Box<dyn std::error::Error + Send>>(())
    })
});
```

**Key points:**

- Host actors run for the entire simulation duration
- They use infinite loops; the simulation duration terminates them
- They can maintain state and monitor system conditions

### 5. Creating Client Actors

Multiple client actors are spawned to interact with the server:

```rust
for i in 0..self.client_count {
    let counter = Arc::clone(&self.connection_counter);

    sim.client(format!("client-{i}"), async move {
        log::info!("Client {i} starting...");

        // Stagger start times
        let initial_delay = Duration::from_millis(100 * u64::try_from(i).unwrap_or(0));
        time::sleep(initial_delay).await;

        // Perform multiple connection rounds
        for round in 0..3 {
            // "Connect" to server
            let prev = counter.fetch_add(1, Ordering::Relaxed);
            log::info!("Client {i}: Connection #{round} (total: {})", prev + 1);

            // Do work
            time::sleep(Duration::from_millis(800)).await;

            // "Disconnect" from server
            let prev = counter.fetch_sub(1, Ordering::Relaxed);
            log::debug!("Client {i}: Disconnected (remaining: {})", prev - 1);

            // Wait before next round
            time::sleep(Duration::from_millis(400)).await;
        }

        log::info!("Client {i} completed all connections");
        Ok::<(), Box<dyn std::error::Error + Send>>(())
    });
}
```

**Key points:**

- Client actors can complete their work and terminate
- Start times are staggered for realistic behavior
- They communicate through shared state (atomic counter)
- Each client performs the same pattern of work

### 6. Cleanup and Reporting

The `on_end()` method runs after simulation completion:

```rust
fn on_end(&self, _sim: &impl Sim) {
    let final_count = self.connection_counter.load(Ordering::Relaxed);
    println!("\n=== FINAL STATISTICS ===");
    println!("Total clients: {}", self.client_count);
    println!("Active connections at end: {final_count}");
}
```

## Key Concepts

### SimBootstrap Trait

The `SimBootstrap` trait defines the simulation lifecycle:

- **`build_sim(config)`**: Configure simulation parameters (duration, seed, etc.)
- **`on_start(sim)`**: Initialize hosts and clients when simulation begins
- **`on_end(sim)`**: Cleanup and reporting when simulation completes (optional)

### Host vs. Client Actors

**Host Actors:**

- Persistent services that run for the simulation duration
- Use infinite loops; terminated by simulation timeout
- Examples: servers, databases, monitoring services
- Created with `sim.host(name, factory)`

**Client Actors:**

- Ephemeral entities that perform specific tasks
- Can complete and terminate naturally
- Examples: request generators, test clients, batch jobs
- Created with `sim.client(name, future)`

### Simulation Time

Simvar uses **deterministic time simulation** via `switchy_time`:

- **`time::now()`**: Get current simulation time
- **`time::sleep(duration)`**: Sleep in simulation time (not real time)
- **Deterministic**: Same seed produces identical execution order
- **Fast**: Simulations run faster than real-time
- **Controllable**: Time advances in predictable increments

**Critical:** Always use `switchy_time::sleep()` instead of `tokio::time::sleep()` in simulations!

### Shared State Between Actors

Actors share state using thread-safe primitives:

```rust
// Create shared state
let counter = Arc::new(AtomicU32::new(0));

// Clone for each actor
let counter_clone = Arc::clone(&counter);

// Access atomically
counter.fetch_add(1, Ordering::Relaxed);
let value = counter.load(Ordering::Relaxed);
```

### Deterministic Execution

Simvar provides reproducible execution:

- **Same seed → identical results**: Perfect for regression testing
- **Controlled randomness**: Use simvar's random feature for deterministic RNG
- **No race conditions**: Execution order is deterministic
- **Reproducible bugs**: Bugs occur consistently, making debugging easier

## Testing the Example

### Basic Verification

Run the example and verify:

1. Simulation completes successfully
2. All 3 clients start and complete
3. Connection count reaches 0 at the end
4. Server runs for the full 5 seconds
5. Total execution time is much less than 5 real seconds (time acceleration)

### Experimenting with Configuration

Try modifying the bootstrap:

```rust
BasicSimulationBootstrap {
    client_count: 5,  // More clients
    ..Default::default()
}
```

Or change the simulation duration:

```rust
fn build_sim(&self, config: SimConfig) -> SimConfig {
    config.with_duration(Duration::from_secs(10))
}
```

### Observing Determinism

Run the example multiple times:

```bash
cargo run -p simvar_basic_simulation_example > run1.txt
cargo run -p simvar_basic_simulation_example > run2.txt
diff run1.txt run2.txt
```

The output should be **identical** each time (assuming no logging timestamps).

### Adding Custom Behavior

Extend the example by:

1. Adding more connection logic to clients
2. Implementing server-side processing
3. Adding different client behaviors (fast/slow clients)
4. Tracking additional metrics (latency, throughput)

## Troubleshooting

### Compilation Errors

**Problem**: Example doesn't compile

**Solutions:**

- Update Rust toolchain: `rustup update`
- Clean and rebuild: `cargo clean && cargo build -p simvar_basic_simulation_example`
- Check workspace dependencies: `cargo check`

### Simulation Hangs

**Problem**: Simulation doesn't complete

**Solutions:**

- Verify `build_sim()` sets a duration: `config.with_duration(Duration::from_secs(5))`
- Check for deadlocks in shared state access
- Enable debug logging: `RUST_LOG=debug cargo run -p simvar_basic_simulation_example`

### Unexpected Behavior

**Problem**: Clients don't complete or counts are wrong

**Solutions:**

- Verify you're using `switchy_time::sleep()` not `tokio::time::sleep()`
- Check atomic operations use correct `Ordering`
- Review client logic for infinite loops
- Add more logging to trace execution

### No Output

**Problem**: Simulation runs but produces no output

**Solutions:**

- Enable logging: `RUST_LOG=info cargo run -p simvar_basic_simulation_example`
- Check that `env_logger` is initialized in `main()`
- Verify `log::info!` statements are present

## Related Examples

- **basic_web_server**: Web server simulation demonstrating HTTP request/response
- **api_testing**: Advanced API testing with comprehensive test scenarios
- **Other simvar examples**: Explore `packages/simvar/examples/` for more patterns
