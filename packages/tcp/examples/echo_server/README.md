# Echo Server Example

A basic TCP echo server and client demonstrating fundamental usage of the `switchy_tcp` package with Tokio.

## Summary

This example shows how to create a simple TCP echo server that accepts connections and echoes back any data it receives, along with a client that connects and sends test messages.

## What This Example Demonstrates

- Creating and binding a TCP listener with `TokioTcpListener`
- Accepting incoming TCP connections
- Creating a TCP client connection with `TokioTcpStream`
- Reading and writing data over TCP streams
- Handling multiple concurrent connections with `tokio::spawn`
- Accessing connection information (local and peer addresses)
- Proper error handling for network operations

## Prerequisites

- Basic understanding of async Rust and Tokio
- Familiarity with TCP networking concepts
- Knowledge of how to run Rust examples with Cargo

## Running the Example

First, start the server in one terminal:

```bash
cargo run --manifest-path packages/tcp/examples/echo_server/Cargo.toml -- server 127.0.0.1:8080
```

Then, in another terminal, run the client:

```bash
cargo run --manifest-path packages/tcp/examples/echo_server/Cargo.toml -- client 127.0.0.1:8080
```

## Expected Output

**Server terminal:**

```
Starting echo server on 127.0.0.1:8080
Echo server listening on 127.0.0.1:8080
New connection from: 127.0.0.1:XXXXX
Received 14 bytes from 127.0.0.1:XXXXX: "Hello, server!"
Echoed 14 bytes back to 127.0.0.1:XXXXX
Received 12 bytes from 127.0.0.1:XXXXX: "How are you?"
Echoed 12 bytes back to 127.0.0.1:XXXXX
Received 8 bytes from 127.0.0.1:XXXXX: "Goodbye!"
Echoed 8 bytes back to 127.0.0.1:XXXXX
Client 127.0.0.1:XXXXX disconnected
```

**Client terminal:**

```
Connecting to server at 127.0.0.1:8080
Connected to server at 127.0.0.1:8080
Local address: 127.0.0.1:XXXXX
Sending: Hello, server!
Received: Hello, server!
Sending: How are you?
Received: How are you?
Sending: Goodbye!
Received: Goodbye!
Client finished
```

## Code Walkthrough

### Server Setup

The server binds to an address and listens for connections:

```rust
let listener = TokioTcpListener::bind(addr).await?;
```

### Accepting Connections

The server accepts connections in a loop and spawns a task for each:

```rust
loop {
    let (mut stream, client_addr) = listener.accept().await?;
    tokio::spawn(async move {
        // Handle connection
    });
}
```

### Echo Logic

For each connection, the server reads data and echoes it back:

```rust
let mut buffer = [0u8; 1024];
match stream.read(&mut buffer).await {
    Ok(0) => break, // Connection closed
    Ok(n) => {
        stream.write_all(&buffer[..n]).await?;
    }
    Err(e) => eprintln!("Error: {e}"),
}
```

### Client Connection

The client connects to the server:

```rust
let mut stream = TokioTcpStream::connect(addr).await?;
```

### Sending and Receiving

The client sends messages and reads responses:

```rust
stream.write_all(msg.as_bytes()).await?;
let n = stream.read(&mut buffer).await?;
```

## Key Concepts

### Tokio TCP Types

The example uses `TokioTcpListener` and `TokioTcpStream` which are the Tokio-backed implementations of the generic TCP traits. These provide real network I/O.

### Concurrent Connection Handling

Each client connection is handled in its own task using `tokio::spawn`, allowing the server to handle multiple clients simultaneously without blocking.

### Graceful Disconnection

The server detects when a client disconnects by receiving a read of 0 bytes, which indicates the connection has been closed.

### Error Handling

Network operations return `Result` types, and errors are handled appropriately using the `?` operator and `match` expressions.

## Testing the Example

1. Start the server in one terminal
2. Run the client in another terminal
3. Observe the messages being sent and echoed back
4. Try running multiple clients simultaneously to see concurrent connection handling
5. Try sending Ctrl+C to the client to see graceful disconnection

## Troubleshooting

**"Address already in use" error:**

- The port is already bound by another process
- Wait a few seconds for the OS to release the port
- Use a different port number

**"Connection refused" error:**

- Make sure the server is running before starting the client
- Verify the address and port match between server and client

**Client hangs or doesn't receive data:**

- Check that the server is echoing data correctly
- Verify network connectivity
- Check for firewall issues

## Related Examples

- `split_stream` - Demonstrates concurrent reading and writing with stream splitting
- `simulator_test` - Shows testing with the in-memory simulator
