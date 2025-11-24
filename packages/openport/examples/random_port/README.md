# Random Port Example

Demonstrates random port selection using openport's `rand` feature, which provides better port distribution for production environments.

## Summary

This example shows how to use `pick_random_unused_port()` to find available ports with random selection, reducing port conflicts and improving distribution across the port range.

## What This Example Demonstrates

- Using `pick_random_unused_port()` for random port selection
- Comparing random vs sequential port selection strategies
- Understanding the fallback behavior (random → OS-provided → none)
- Practical use cases for random port selection
- Best practices for allocating ports to multiple services

## Prerequisites

- Basic understanding of network ports
- Familiarity with the `pick_unused_port` function (see `basic_usage` example)
- Understanding of why port conflicts occur

## Running the Example

```bash
cargo run --manifest-path packages/openport/examples/random_port/Cargo.toml
```

Note: This example requires the `rand` feature, which is enabled in the example's `Cargo.toml`.

## Expected Output

```
=== OpenPort Random Port Example ===

1. Finding a random available port (15000-25000 range):
   ✓ Found random port: 18734
   Port is in range: true

2. Comparing random and sequential port selection:
   Finding 5 ports using sequential search (15000..16000):
     Sequential ports: [15000, 15001, 15002, 15003, 15004]
   Finding 5 ports using random search:
     Random ports: [16234, 22891, 19045, 17823, 20156]

3. Use case comparison:
   Sequential search (pick_unused_port):
     + Predictable - always returns lowest available port
     + Fast for small ranges
     - May conflict with other processes using low ports

   Random search (pick_random_unused_port):
     + Better distribution across port range
     + Less likely to conflict with other services
     + Good for production environments
     - Slightly slower (tries 10 random attempts first)

4. Allocating ports for multiple microservices:
   ✓ auth-service: http://localhost:17892
   ✓ api-gateway: http://localhost:23451
   ✓ data-service: http://localhost:19023

5. Understanding the fallback behavior:
   pick_random_unused_port() tries:
   1. Up to 10 random ports in range 15000-25000
   2. If none found, asks OS for free TCP port (10 attempts)
   3. Verifies OS-provided port is also free on UDP
   4. Returns None after 20 total failed attempts

   ✓ Successfully found port 21567 using this algorithm

=== Example completed successfully ===
```

Note: Port numbers will vary on each run due to randomization.

## Code Walkthrough

### Basic Random Port Selection

The simplest usage is straightforward:

```rust
use openport::pick_random_unused_port;

match pick_random_unused_port() {
    Some(port) => println!("Found port: {}", port),
    None => println!("No ports available"),
}
```

This finds a random available port in the range 15000-25000.

### Random vs Sequential Selection

The example demonstrates the difference:

```rust
// Sequential - returns lowest available port
let port1 = pick_unused_port(15000..16000); // Likely returns 15000

// Random - returns random available port
let port2 = pick_random_unused_port(); // Could be anywhere in 15000-25000
```

Sequential selection is predictable but can lead to port conflicts when multiple processes use the same strategy.

### Fallback Algorithm

`pick_random_unused_port()` uses a sophisticated fallback strategy:

1. **Phase 1 - Random attempts (10 tries)**:

    - Generate random port in 15000-25000
    - Check if free on both TCP and UDP
    - Return immediately if found

2. **Phase 2 - OS assistance (10 tries)**:

    - Ask OS for a free TCP port (by binding to port 0)
    - Verify the port is also free on UDP
    - Return if both conditions met

3. **Phase 3 - Give up**:
    - Return `None` after 20 failed attempts

### Practical Usage for Microservices

```rust
let services = ["auth", "api", "data"];
let mut ports = Vec::new();

for service in services {
    if let Some(port) = pick_random_unused_port() {
        println!("{}: http://localhost:{}", service, port);
        ports.push(port);
    }
}
```

This distributes services across the port range, reducing conflicts.

## Key Concepts

### Why Random Selection Matters

**Problem with sequential selection:**

```rust
// Service A starts first
let port_a = pick_unused_port(15000..16000); // Gets 15000

// Service B starts second
let port_b = pick_unused_port(15000..16000); // Gets 15001

// Service A restarts while B is running
let port_a_new = pick_unused_port(15000..16000); // Gets 15002 (not 15000!)
```

Service A can't get its original port back because sequential search is predictable.

**Solution with random selection:**

```rust
let port_a = pick_random_unused_port(); // Gets 18234
// ... A stops, B still running ...
let port_a_new = pick_random_unused_port(); // Likely gets 18234 back
```

Random selection spreads services across the range, making ports more likely to be available on restart.

### Port Range Choice (15000-25000)

This range was chosen because:

- **Below 15000**: Many dynamic/ephemeral ports assigned by OS
- **15000-25000**: Sweet spot with 10,000 available ports
- **Above 25000**: Often used for OS ephemeral ports (varies by system)

The range provides good balance between availability and avoiding conflicts.

### Performance Characteristics

- **Average case**: 1-2 attempts to find port (typical system has many free ports)
- **Worst case**: 20 attempts before giving up
- **Trade-off**: Slightly slower than sequential, but better distribution

### Thread Safety

`pick_random_unused_port()` is thread-safe:

- Each call generates a new random number
- No shared state between calls
- Safe to call from multiple threads simultaneously

However, there's still a race condition between checking and binding (see Troubleshooting).

## Testing the Example

### Compare Multiple Runs

Run the example several times to see port randomization:

```bash
for i in {1..5}; do
  echo "=== Run $i ==="
  cargo run --manifest-path packages/openport/examples/random_port/Cargo.toml | grep "Found random port"
done
```

You should see different port numbers each time.

### Verify Port Distribution

To verify ports are well-distributed:

```bash
cargo run --manifest-path packages/openport/examples/random_port/Cargo.toml | \
  grep "Random ports:" | \
  cut -d: -f2
```

Ports should be spread across the range, not clustered.

## Troubleshooting

### "No available ports found after multiple attempts"

This is extremely rare but can happen if:

- System has very few free ports
- Many services are running
- The system's ephemeral port range overlaps with 15000-25000

Solutions:

- Check active ports: `netstat -an | grep LISTEN`
- Free up ports by stopping unused services
- Use `pick_unused_port()` with a custom range instead

### Port Conflicts Still Occur

Even with random selection, race conditions exist:

1. `pick_random_unused_port()` finds port 18234 as free
2. Another process binds to 18234
3. Your bind attempt fails

This is unavoidable without a reservation system. Solutions:

- Retry on bind failure with a new random port
- Use `PortReservation` for coordinated multi-port allocation (see `port_reservation` example)
- Bind immediately after calling the function

### Different Results Than Expected

Port availability depends on system state:

- Other services may be using ports
- Previous runs may have left ports in `TIME_WAIT` state
- Operating system may reserve certain ports

This is normal behavior - the example adapts to your system's current state.

### Build Fails with "pick_random_unused_port not found"

The function requires the `rand` feature:

```toml
[dependencies]
openport = { workspace = true, features = ["rand"] }
```

This is already configured in the example's `Cargo.toml`.

## Related Examples

- `basic_usage` - Introduction to openport's core functions
- `port_reservation` - Advanced port management for coordinated allocation
