# Basic Usage Example

A comprehensive example demonstrating the core functionality of the `openport` crate for finding available network ports.

## Summary

This example shows how to find available ports within specified ranges, check if specific ports are free, and actually bind to discovered ports using both TCP and UDP protocols.

## What This Example Demonstrates

- Finding available ports in exclusive ranges (`start..end`)
- Finding available ports in inclusive ranges (`start..=end`)
- Checking if specific ports are available on TCP, UDP, or both
- Verifying that bound ports are correctly detected as unavailable
- Creating actual TCP and UDP servers on discovered ports
- Finding multiple available ports from a range

## Prerequisites

- Basic understanding of networking concepts (TCP/UDP, ports)
- Familiarity with Rust's standard networking types (`TcpListener`, `UdpSocket`)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/openport/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/openport/examples/basic_usage
cargo run
```

## Expected Output

```
=== OpenPort Basic Usage Example ===

1. Finding a port in range 8000..9000:
   Found available port: 8000
   Port is in expected range: true

2. Finding a port in inclusive range 15000..=16000:
   Found available port: 15000
   Port is in expected range: true

3. Checking specific port availability:
   Checking port 8080
   - TCP available: true
   - UDP available: true
   - Both available: true

4. Demonstrating port detection after binding:
   Found free port: 20000
   Port is free before binding: true
   Bound TCP listener to port 20000
   Port is free after TCP binding: false

5. Finding a port and creating a simple server:
   Starting server on port 30000
   ✓ TCP server successfully bound to 127.0.0.1:30000
   Server address: 127.0.0.1:30000
   ✓ UDP socket successfully bound to 127.0.0.1:30000
   Socket address: 127.0.0.1:30000

6. Finding multiple available ports:
   Found 5 available ports in range 40000..41000:
     1. Port 40000
     2. Port 40001
     3. Port 40002
     4. Port 40003
     5. Port 40004

=== Example Complete ===
```

Note: Actual port numbers will vary depending on what ports are available on your system.

## Code Walkthrough

### 1. Finding Ports in Ranges

The most basic operation is finding an available port within a range:

```rust
// Exclusive range (does not include 9000)
match openport::pick_unused_port(8000..9000) {
    Some(port) => println!("Found port: {}", port),
    None => println!("No ports available"),
}

// Inclusive range (includes 16000)
match openport::pick_unused_port(15000..=16000) {
    Some(port) => println!("Found port: {}", port),
    None => println!("No ports available"),
}
```

The function returns the first available port in the range, or `None` if all ports are occupied.

### 2. Checking Port Availability

You can check if specific ports are available before attempting to bind:

```rust
let port = 8080;

// Check TCP only
if openport::is_free_tcp(port) {
    println!("Port {} is available on TCP", port);
}

// Check UDP only
if openport::is_free_udp(port) {
    println!("Port {} is available on UDP", port);
}

// Check both protocols
if openport::is_free(port) {
    println!("Port {} is available on both TCP and UDP", port);
}
```

These functions check both IPv4 and IPv6 address families, returning `true` only if the port is free on both.

### 3. Binding to Discovered Ports

Once you find an available port, you can bind servers to it:

```rust
if let Some(port) = openport::pick_unused_port(30000..31000) {
    // Create TCP listener
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .expect("Failed to bind");

    // Create UDP socket
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", port))
        .expect("Failed to bind");

    println!("Servers running on port {}", port);
}
```

### 4. Finding Multiple Ports

To find multiple available ports, combine `is_free()` with iteration:

```rust
let mut ports = Vec::new();
for port in 40000..41000 {
    if openport::is_free(port) {
        ports.push(port);
        if ports.len() >= 5 {
            break;
        }
    }
}
```

## Key Concepts

### Port Availability

A port is considered "free" when:

- It can be bound on both IPv4 (`0.0.0.0`) and IPv6 (`[::]`) unspecified addresses
- It's available on the requested protocol (TCP, UDP, or both)

### Sequential Port Checking

`pick_unused_port()` checks ports sequentially from the start of the range until it finds a free port. This is simple and predictable, but not randomized.

### Race Conditions

**Important**: There's a small window between checking if a port is free and actually binding to it. Another process could grab the port in between. Always handle bind errors gracefully:

```rust
if let Some(port) = openport::pick_unused_port(8000..9000) {
    match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(listener) => { /* use listener */ },
        Err(e) => eprintln!("Port was taken: {}", e),
    }
}
```

### IPv4 and IPv6

All availability checks test both IPv4 and IPv6. A port is only considered "free" if it's available on both address families. This prevents subtle bugs in dual-stack network environments.

## Testing the Example

The example is self-contained and requires no additional setup. Simply run it to see:

1. How ports are discovered in different ranges
2. How port availability changes after binding
3. How to create actual network servers on discovered ports

You can verify the output by checking that:

- All discovered ports are within their specified ranges
- Port availability correctly reflects the binding state
- Both TCP and UDP servers can bind to the same discovered port

## Troubleshooting

### "No available ports found in range"

This can happen if:

- All ports in the specified range are already in use
- The range is too small (e.g., `8000..8001` only has one port)
- System restrictions prevent binding to those ports

**Solution**: Try a larger range or a different range (e.g., `15000..25000`).

### "Failed to bind" errors

This usually means another process grabbed the port between discovery and binding.

**Solution**: Use the port reservation system (see the `port_reservation` example) or implement retry logic.

### Permission denied on low ports

Ports below 1024 require root/administrator privileges on most systems.

**Solution**: Use ports above 1024, or run with elevated privileges if necessary.

## Related Examples

- **port_reservation**: Demonstrates the thread-safe port reservation system for managing multiple ports
