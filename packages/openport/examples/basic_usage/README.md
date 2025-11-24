# Basic Usage Example

A comprehensive example demonstrating the core functionality of the openport library for finding available network ports.

## Summary

This example shows how to use openport's basic functions to find available ports, check port availability, and handle common use cases like allocating ports for web servers.

## What This Example Demonstrates

- Finding available ports in exclusive ranges (`start..end`)
- Finding available ports in inclusive ranges (`start..=end`)
- Checking if specific ports are free on both TCP and UDP
- Checking TCP and UDP availability separately
- Practical usage patterns for allocating ports to services
- Finding multiple available ports

## Prerequisites

- Basic understanding of network ports
- Familiarity with TCP and UDP protocols
- Rust programming knowledge

## Running the Example

```bash
cargo run --manifest-path packages/openport/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will:

1. Find and display an available port in the range 15000-16000 (exclusive)
2. Find and display an available port in the range 8000-9000 (inclusive)
3. Check if specific common ports (8080, 3000, 5000, 9090) are available
4. Demonstrate checking TCP and UDP availability separately for port 8080
5. Show a practical example of finding a port for a web server (3000-9000 range)
6. Find multiple available ports in the range 10000-20000

Example output:

```
=== OpenPort Basic Usage Example ===

1. Finding available port in range 15000..16000 (exclusive):
   ✓ Found available port: 15234

2. Finding available port in range 8000..=9000 (inclusive):
   ✓ Found available port: 8567

3. Checking availability of specific ports:
   ✓ Port 8080 is free on both TCP and UDP
   ✗ Port 3000 is in use
   ✓ Port 5000 is free on both TCP and UDP
   ✓ Port 9090 is free on both TCP and UDP

4. Checking TCP and UDP availability separately:
   Port 8080:
     TCP: free
     UDP: free

5. Practical example - allocating port for a web server:
   ✓ Allocated port 8123 for web server
   Server would start at: http://localhost:8123

6. Finding multiple available ports:
   Found 3 ports: [12345, 12346, 12347]

=== Example completed successfully ===
```

Note: Actual port numbers will vary based on system availability.

## Code Walkthrough

### Finding Ports in Ranges

The `pick_unused_port` function accepts both exclusive and inclusive ranges:

```rust
// Exclusive range (15000 to 15999)
match pick_unused_port(15000..16000) {
    Some(port) => println!("Found port: {}", port),
    None => println!("No ports available"),
}

// Inclusive range (8000 to 9000)
match pick_unused_port(8000..=9000) {
    Some(port) => println!("Found port: {}", port),
    None => println!("No ports available"),
}
```

The function iterates through the range sequentially and returns the first available port.

### Checking Port Availability

Three functions are available for checking port availability:

```rust
// Check both TCP and UDP
if is_free(8080) {
    println!("Port 8080 is completely free");
}

// Check TCP only
if is_free_tcp(8080) {
    println!("Port 8080 is free on TCP");
}

// Check UDP only
if is_free_udp(8080) {
    println!("Port 8080 is free on UDP");
}
```

A port is considered free only if it can be bound on both IPv4 (`0.0.0.0`) and IPv6 (`::`).

### Practical Usage Pattern

For real applications, use error handling to ensure a port is available:

```rust
let port = pick_unused_port(3000..9000)
    .ok_or("No available ports in range")?;
println!("Server starting on http://localhost:{}", port);
```

This pattern is common for development servers, test environments, and microservices.

## Key Concepts

### Port Ranges

- **Exclusive ranges** (`start..end`): Include `start` but exclude `end`
- **Inclusive ranges** (`start..=end`): Include both `start` and `end`
- Both types implement the `PortRange` trait and work with `pick_unused_port`

### Port Availability Checking

- Ports are checked by attempting to bind to both IPv4 and IPv6 addresses
- A port is only considered free if it's available on both address families
- Both TCP and UDP must be free for `is_free()` to return true

### Sequential Search

The `pick_unused_port` function searches sequentially through the range. For large ranges, this means:

- Lower ports in the range are tried first
- The search stops at the first available port
- If you need a random port, use the `rand` feature (see `random_port` example)

### Port Selection Best Practices

- **Privileged ports** (< 1024): Require root/administrator access
- **Common ports** (1024-49151): May be in use by other services
- **Dynamic/private ports** (49152-65535): Usually safe for temporary use
- **Custom ranges** (15000-25000): Good balance for development/testing

## Testing the Example

Run the example multiple times to see how available ports vary based on system state:

```bash
# Run once
cargo run --manifest-path packages/openport/examples/basic_usage/Cargo.toml

# Run multiple times to see different port allocations
for i in {1..3}; do
  echo "=== Run $i ==="
  cargo run --manifest-path packages/openport/examples/basic_usage/Cargo.toml
  echo
done
```

## Troubleshooting

### "No available ports in range"

If you see this message:

- The specified range may be too narrow
- Other services may be using ports in the range
- Try a different or wider range (e.g., 10000..20000)
- Check which ports are in use with `netstat -an` or `ss -tuln`

### Port Shows as Free but Bind Fails

There's a potential race condition where:

1. `pick_unused_port` finds a port
2. Another process binds to it before you can
3. Your bind attempt fails

This is inherent to the design. Solutions:

- Retry with a different port
- Use the `PortReservation` system (see `port_reservation` example)
- Bind immediately after calling `pick_unused_port`

### Permission Denied

If you try to bind to ports < 1024 without proper privileges:

```
Error: Permission denied (os error 13)
```

Solution: Use ports >= 1024 or run with elevated privileges (not recommended).

## Related Examples

- `random_port` - Demonstrates random port selection with the `rand` feature
- `port_reservation` - Shows the port reservation system for managing multiple ports
