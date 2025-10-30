# Basic Simulation Example

A simple example demonstrating the core features of `simvar_harness` for deterministic simulation testing.

## Summary

This example shows how to create a basic deterministic simulation with host and client actors using the `simvar_harness` framework. The simulation includes a message processor host that runs continuously and multiple client actors that send numbered messages at regular intervals.

## What This Example Demonstrates

- Creating a simulation with `SimBootstrap` trait implementation
- Configuring simulation duration and behavior
- Spawning host actors (persistent services that can be restarted)
- Spawning client actors (ephemeral actors that complete tasks)
- Using lifecycle hooks (`init`, `on_start`, `on_step`, `on_end`)
- Checking for simulation cancellation in actors
- Using simulated time with `switchy` for deterministic testing
- Custom simulation properties for result tracking
- Collecting and analyzing simulation results

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with simulation testing concepts
- Knowledge of actor-based systems (helpful but not required)

## Running the Example

From the MoosicBox repository root:

```bash
cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml
```

With debug logging to see detailed simulation progress:

```bash
RUST_LOG=debug cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml
```

Run multiple simulation runs in parallel:

```bash
SIMULATOR_RUNS=5 SIMULATOR_MAX_PARALLEL=2 cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml
```

## Expected Output

The simulation will display:

```
Starting basic simulation example...

=========================== START ============================
Server simulator starting
...
==============================================================

[TUI display showing simulation progress, if enabled]

=== SIMULATION RESULTS ===
Success { ... }

Success rate: 1/1
All simulation runs completed successfully!
```

With `RUST_LOG=debug`, you'll see detailed logs including:

- Client messages being sent with timestamps
- Message processor batches being processed
- Simulation step progress (every 1000 steps)
- Cancellation signals and graceful shutdown

## Code Walkthrough

### 1. Bootstrap Implementation

The `BasicSimulationBootstrap` struct configures the simulation:

```rust
impl SimBootstrap for BasicSimulationBootstrap {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(5);
        config.enable_random_order = true;
        config
    }
}
```

This sets a 5-second simulation duration and enables random actor execution order for more realistic testing.

### 2. Spawning Actors in `on_start`

Host actors are spawned with a factory function that allows them to be restarted:

```rust
sim.host("message-processor", move || {
    Box::pin(async move { run_message_processor().await })
});
```

Client actors are spawned with a single async block:

```rust
sim.client(format!("client-{client_id}"), async move {
    run_message_client(client_id, message_interval).await
});
```

### 3. Host Actor Pattern

The host actor runs continuously until cancelled:

```rust
async fn run_message_processor() -> HostResult {
    let mut processed_count = 0;

    loop {
        if simvar_harness::utils::is_simulator_cancelled() {
            break;
        }

        // Do work...
        simvar_harness::switchy::async_utils::time::sleep(Duration::from_millis(100)).await;
        processed_count += 1;
    }

    Ok(())
}
```

### 4. Client Actor Pattern

Client actors perform a task and complete:

```rust
async fn run_message_client(client_id: usize, interval: Duration) -> ClientResult {
    let mut message_count = 0;

    loop {
        if simvar_harness::utils::is_simulator_cancelled() {
            break;
        }

        message_count += 1;
        // Send message...

        simvar_harness::switchy::async_utils::time::sleep(interval).await;
    }

    Ok(())
}
```

### 5. Result Collection

After simulation runs complete, results are collected and analyzed:

```rust
let results = run_simulation(bootstrap)?;
let success_count = results.iter().filter(|r| r.is_success()).count();
println!("Success rate: {success_count}/{}", results.len());
```

## Key Concepts

### Deterministic Simulation

The `simvar_harness` framework provides deterministic simulation through controlled randomness and time. The same seed produces identical results across runs, making it ideal for:

- Reproducing bugs in concurrent systems
- Testing race conditions systematically
- Validating distributed system behavior

### Host vs Client Actors

- **Host actors**: Persistent services that run continuously and can be restarted (bounced) during simulation. Use for servers, databases, or long-running services.
- **Client actors**: Ephemeral actors that complete a specific task and exit. Use for clients, batch jobs, or one-time operations.

### Simulated Time

The framework uses simulated time through `switchy`, which allows:

- Fast-forwarding through idle periods
- Deterministic scheduling of events
- Reproducible timing behavior

### Lifecycle Hooks

The `SimBootstrap` trait provides hooks for customizing behavior:

- `init()`: One-time initialization before all runs
- `on_start()`: Called at the beginning of each run (spawn actors here)
- `on_step()`: Called every simulation step (use sparingly)
- `on_end()`: Called when a run completes

### Cancellation Handling

Actors should check `is_simulator_cancelled()` regularly to respond to simulation end or Ctrl-C signals. This ensures graceful shutdown and proper cleanup.

## Testing the Example

1. **Run with different configurations**:

    ```bash
    # Longer duration
    SIMULATOR_DURATION=10s cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml

    # Multiple parallel runs
    SIMULATOR_RUNS=10 SIMULATOR_MAX_PARALLEL=4 cargo run --manifest-path packages/simvar/harness/examples/basic_simulation/Cargo.toml
    ```

2. **Test cancellation**: Press Ctrl-C while the simulation is running to verify graceful shutdown.

3. **Modify actor behavior**: Edit the client count or message interval in `BasicSimulationBootstrap` to see how it affects the simulation.

## Troubleshooting

### "All simulation runs failed"

- Check that actors are properly checking `is_simulator_cancelled()` and returning `Ok(())` on graceful shutdown
- Ensure no panics occur in actor code
- Run with `RUST_LOG=debug` to see detailed error messages

### Simulation hangs or doesn't progress

- Verify actors use `switchy::async_utils::time::sleep()` instead of `std::thread::sleep()` or `tokio::time::sleep()`
- Ensure actors eventually check cancellation status
- Check that no actor has an infinite busy loop without yielding

### TUI doesn't display

- Ensure `tui` feature is enabled (it's a default feature)
- The `NO_TUI` environment variable disables the TUI if set
- TUI may not work in some terminal environments

## Related Examples

- `packages/simvar/examples/basic_web_server/` - Web server simulation with HTTP clients
- `packages/simvar/examples/api_testing/` - More complex API testing scenarios

For more advanced usage, see the `simvar_harness` package documentation and the other examples in the `packages/simvar/` directory.
