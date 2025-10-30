# Simulator Test Example

Demonstrates using the in-memory TCP simulator for testing network code without actual network I/O.

## Summary

This example shows how to use the `switchy_tcp` simulator feature to test TCP networking code in-memory. The simulator provides the same API as real TCP but operates entirely in memory, making tests fast, deterministic, and independent of network availability.

## What This Example Demonstrates

- Using `SimulatorTcpListener` and `SimulatorTcpStream` for testing
- Testing basic client-server communication without network I/O
- Testing multiple concurrent connections in memory
- Testing split stream operations with the simulator
- Writing deterministic network tests
- Avoiding port conflicts in test suites

## Prerequisites

- Understanding of async Rust and Tokio
- Familiarity with TCP networking concepts
- Basic knowledge of testing practices
- Understanding of why mocking/simulation is useful in tests

## Running the Example

```bash
cargo run --manifest-path packages/tcp/examples/simulator_test/Cargo.toml
```

## Expected Output

```
Switchy TCP Simulator Testing Example
======================================
This example demonstrates testing TCP code with the in-memory simulator

=== Test 1: Basic Communication ===
Simulator listener bound to 127.0.0.1:8080
Client connected to 127.0.0.1:8080
Server accepted connection from 127.0.0.1:XXXXX
Server received command: PING
Client sent: PING
Server sending response: PONG
Client received: PONG
✓ Test passed: Basic communication works

=== Test 2: Multiple Concurrent Connections ===
Listener bound to 127.0.0.1:8081
Client #1 connected
Server accepted connection #1 from 127.0.0.1:XXXXX
Client #2 connected
Server accepted connection #2 from 127.0.0.1:XXXXX
Client #3 connected
Server accepted connection #3 from 127.0.0.1:XXXXX
Server received command: PING
Client #1 sent: PING
Server sending response: PONG
Client #1 received: PONG
Server received command: HELLO
Client #2 sent: HELLO
Server sending response: WORLD
Client #2 received: WORLD
Server received command: STATUS
Client #3 sent: STATUS
Server sending response: OK
Client #3 received: OK
✓ Test passed: Multiple concurrent connections work

=== Test 3: Split Stream Communication ===
Listener bound to 127.0.0.1:8082
✓ Test passed: Split stream communication works

✓ All tests passed successfully!

Key benefits of the simulator:
  - No actual network I/O (fast and deterministic)
  - No port conflicts
  - Works without network access
  - Perfect for unit testing
```

## Code Walkthrough

### Creating a Simulator Listener

The simulator uses the same API as real TCP:

```rust
let listener = SimulatorTcpListener::bind(addr).await?;
```

### Connecting with Simulator

Client connections work identically:

```rust
let mut client = SimulatorTcpStream::connect(addr).await?;
```

### Test 1: Basic Communication

Demonstrates a simple request-response pattern:

```rust
// Server accepts and handles connection
let (stream, _) = listener.accept().await?;
handle_client(stream).await?;

// Client sends command and reads response
client.write_all(b"PING").await?;
let n = client.read(&mut buffer).await?;
```

### Test 2: Multiple Connections

Shows concurrent client handling:

```rust
for i in 1..=3 {
    let (stream, _) = listener.accept().await?;
    tokio::spawn(async move {
        handle_client(stream).await?;
    });
}
```

### Test 3: Split Streams

Demonstrates that split operations work with the simulator:

```rust
let (mut read_half, mut write_half) = stream.into_split();

// Use halves independently in different tasks
```

## Key Concepts

### In-Memory Simulation

The simulator operates entirely in memory using channels. No actual network sockets are created, making tests fast and deterministic.

### Same API as Real TCP

The simulator implements the same generic traits as the Tokio TCP implementation, so you can write code once and test with the simulator but run in production with real TCP.

### Deterministic Testing

Because there's no actual network I/O, tests are deterministic and repeatable. No flaky tests due to network conditions or timing issues.

### No Port Conflicts

Multiple tests can use the same port numbers without conflicts because each test runs in its own isolated in-memory space.

### Generic Programming Benefits

By using the generic `GenericTcpListener` and `GenericTcpStream` traits, you can write code that works with both the simulator and real TCP:

```rust
async fn my_protocol<S, R, W, L>(listener: L)
where
    S: GenericTcpStream<R, W>,
    R: GenericTcpStreamReadHalf,
    W: GenericTcpStreamWriteHalf,
    L: GenericTcpListener<S>,
{
    // Works with both simulator and real TCP
}
```

## Testing the Example

1. Run the example and verify all tests pass
2. Notice how fast the tests execute (no network overhead)
3. Try running the tests multiple times in quick succession (no port conflicts)
4. Modify the commands and responses to experiment with different scenarios
5. Add additional test cases to practice testing with the simulator

## Troubleshooting

**"Connection refused" errors:**

- Ensure the listener is bound before the client connects
- The simulator may require a small delay for setup in complex scenarios

**Tests hang indefinitely:**

- Check that all tasks are properly awaited
- Verify that streams are being read/written on both sides
- Ensure no deadlocks between reader and writer tasks

**Unexpected behavior compared to real TCP:**

- The simulator may have slight timing differences
- Some edge cases might behave differently than real network I/O
- Report any significant differences as potential bugs

## Related Examples

- `echo_server` - Basic TCP server/client with real Tokio TCP
- `split_stream` - Stream splitting (works with both simulator and real TCP)
