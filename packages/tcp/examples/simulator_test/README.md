# TCP Simulator Test Example

Demonstrates using the switchy_tcp simulator for testing TCP code without real networking.

## Summary

This example shows how to use switchy_tcp's in-memory simulator to test TCP client/server code. The simulator provides deterministic testing without actual network I/O, avoiding port conflicts and network-related test flakiness.

## What This Example Demonstrates

- Binding listeners and connecting clients using the simulator
- Testing echo server behavior in-memory
- Understanding ephemeral port allocation
- Splitting TCP streams into read and write halves
- Resetting simulator state for test isolation
- Writing deterministic TCP tests

## Prerequisites

- Basic understanding of async Rust and tokio
- Familiarity with TCP networking concepts
- Understanding of testing patterns in Rust

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/tcp/examples/simulator_test/Cargo.toml
```

The example will run several tests and display output showing simulator functionality.

## Expected Output

```
=== Switchy TCP Simulator Example ===

1. Testing basic echo server...
   Simulator listener bound to 127.0.0.1:8080
   Client connected to server
   Client sent: "Hello from simulator!"
   Server accepted connection from: 127.0.0.1:40000
   Server received 21 bytes
   Server echoed data back
   Client received echo: "Hello from simulator!"
   ✓ Echo data verified!

2. Demonstrating ephemeral port allocation...
   Ephemeral ports start at: 40000
   Bound listener to specified port: 9000
   Next ephemeral ports: 40000, 40001, 40002
   ✓ Port allocation working!

3. Demonstrating stream splitting...
   Client sent data
   Client received: "Split stream test"
   ✓ Stream splitting works!

=== All tests completed successfully! ===
```

## Code Walkthrough

### Basic Simulator Usage

The simulator uses the same API as real TCP, but runs entirely in-memory:

```rust
// Bind a listener (no actual network port used)
let listener = TcpListener::bind("127.0.0.1:8080").await?;

// Connect a client (in-memory connection)
let client = TcpStream::connect("127.0.0.1:8080").await?;

// Accept the connection
let (stream, addr) = listener.accept().await?;

// Use streams just like real TCP
```

### Server-Client Echo Test

The example sets up a server that echoes data back:

```rust
// Server task
let server_task = tokio::spawn(async move {
    let (mut stream, addr) = listener.accept().await?;

    let mut buffer = [0u8; 1024];
    let n = stream.read(&mut buffer).await?;
    stream.write_all(&buffer[..n]).await?;
});

// Client
let mut client = TcpStream::connect("127.0.0.1:8080").await?;
client.write_all(b"Hello from simulator!").await?;

let mut buffer = [0u8; 1024];
let n = client.read(&mut buffer).await?;
```

### Simulator State Management

The simulator provides functions to manage state for test isolation:

```rust
// Reset ephemeral port counter
simulator::reset_next_port();

// Get next available port
let port = simulator::next_port();

// Reset IP allocation
simulator::reset_next_ip();

// Clear DNS mappings
simulator::reset_dns();
```

## Key Concepts

### In-Memory Simulation

The simulator implements TCP behavior entirely in memory using channels. No actual network sockets are created, making tests:

- **Faster**: No kernel syscalls or network stack overhead
- **Deterministic**: No timing issues or port conflicts
- **Isolated**: Tests don't interfere with each other or the network

### Ephemeral Port Allocation

Just like real TCP, the simulator assigns ephemeral ports (starting at 40000 by default) to client connections. You can query and reset this counter:

```rust
let start = simulator::ephemeral_port_start(); // 40000
let port = simulator::next_port();             // 40000
let port = simulator::next_port();             // 40001
simulator::reset_next_port();                  // Reset to 40000
```

### Stream Splitting

The simulator fully supports splitting streams into read and write halves, just like real TCP:

```rust
use switchy_tcp::GenericTcpStream;

let (read_half, write_half) = stream.into_split();

// Can now use halves independently
tokio::spawn(async move {
    // Use read_half in one task
});

tokio::spawn(async move {
    // Use write_half in another task
});
```

### DNS Simulation

The simulator includes a simple DNS system for hostname resolution (though not shown in this example):

```rust
// Map hostname to IP
simulator::dns_add("example.local", "192.168.1.1".parse()?);

// Connect using hostname
let stream = TcpStream::connect("example.local:8080").await?;

// Clear DNS mappings
simulator::reset_dns();
```

## Testing the Example

This example is self-contained and demonstrates its functionality when run. Each test function validates that the simulator works correctly.

To use the simulator in your own tests:

```rust
#[cfg(test)]
mod tests {
    use switchy_tcp::{TcpListener, TcpStream};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_my_tcp_code() {
        // Use simulator (enabled by default feature)
        let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

        // Your test code here...
    }
}
```

## Troubleshooting

### Type Confusion Between Simulator and Tokio

If you see type errors, ensure you're using the correct feature flags:

```toml
# For simulator (testing)
switchy_tcp = { workspace = true, default-features = false, features = ["simulator"] }

# For real networking
switchy_tcp = { workspace = true, default-features = false, features = ["tokio"] }
```

The default features enable both, with simulator types taking precedence.

### Test Flakiness

If tests behave inconsistently:

1. Reset simulator state at the start of each test
2. Use `tokio::time::sleep` to ensure proper ordering when needed
3. Avoid relying on specific ephemeral port values without resetting

### Connection Refused in Simulator

Unlike real TCP, the simulator requires the listener to be bound before connecting. Ensure your server task has started before the client attempts to connect.

## Related Examples

- See the `echo_server` example for real TCP networking with tokio
