# Basic Tunnel Connection Example

## Summary

This example demonstrates how to establish a WebSocket tunnel connection to a remote MoosicBox server and process incoming messages. It shows the complete setup of a `TunnelSender` instance, connection lifecycle management, and message handling for different message types (text, binary, ping, pong, close, and frame).

## What This Example Demonstrates

- Creating a `TunnelSender` instance with connection parameters
- Configuring host, WebSocket URL, client ID, and access token
- Using `ConfigDatabase` for tunnel configuration storage
- Starting a tunnel connection and receiving messages
- Handling different message types from the tunnel
- Gracefully closing the tunnel connection using `TunnelSenderHandle`
- Automatic connection management (authentication, reconnection, ping/pong)

## Prerequisites

- Basic understanding of async Rust and Tokio
- Familiarity with WebSocket protocol concepts
- Valid MoosicBox server endpoint (or test server)
- Client credentials (client ID and access token)

## Running the Example

```bash
# Using environment variables to configure the connection
TUNNEL_HOST=https://your-server.com \
TUNNEL_WS_URL=wss://your-server.com/tunnel \
TUNNEL_CLIENT_ID=your-client-id \
TUNNEL_ACCESS_TOKEN=your-access-token \
cargo run --manifest-path packages/tunnel_sender/examples/basic_connection/Cargo.toml

# Or use the default demo values (will attempt to connect to example.moosicbox.com)
cargo run --manifest-path packages/tunnel_sender/examples/basic_connection/Cargo.toml
```

To enable detailed logging:

```bash
RUST_LOG=debug cargo run --manifest-path packages/tunnel_sender/examples/basic_connection/Cargo.toml
```

## Expected Output

When the example runs successfully, you should see:

```
=== MoosicBox Tunnel Sender - Basic Connection Example ===

Configuration:
  Host:      https://example.moosicbox.com
  WS URL:    wss://example.moosicbox.com/tunnel
  Client ID: demo-client-123
  Token:     demo-acces...

Creating tunnel sender...
Tunnel sender created successfully
Handle allows connection control (e.g., closing the tunnel)

Starting tunnel connection...
The tunnel will automatically:
  - Fetch authentication signature token
  - Establish WebSocket connection
  - Handle reconnection on failures
  - Send periodic ping messages

Tunnel started! Waiting for messages...
Press Ctrl+C to stop

Received message #1:
  Type: Text
  Content: {"type":"welcome","data":"Connected to tunnel"}

Received message #2:
  Type: Ping
  Payload: 0 bytes

...

Closing tunnel connection...
Tunnel closed successfully

Total messages received: 10
```

## Code Walkthrough

### Step 1: Configuration

```rust
let host = std::env::var("TUNNEL_HOST")
    .unwrap_or_else(|_| "https://example.moosicbox.com".to_string());
let ws_url = std::env::var("TUNNEL_WS_URL")
    .unwrap_or_else(|_| "wss://example.moosicbox.com/tunnel".to_string());
let client_id = std::env::var("TUNNEL_CLIENT_ID")
    .unwrap_or_else(|_| "demo-client-123".to_string());
let access_token = std::env::var("TUNNEL_ACCESS_TOKEN")
    .unwrap_or_else(|_| "demo-access-token-456".to_string());
```

The connection requires four parameters:

- **host**: HTTP(S) endpoint for API calls and authentication
- **ws_url**: WebSocket endpoint for tunnel connection
- **client_id**: Unique identifier for this client instance
- **access_token**: Authentication token for secure connection

### Step 2: Create Configuration Database

```rust
let config_db = ConfigDatabase::new();
```

The `ConfigDatabase` stores tunnel configuration and state. In production, you might initialize this with persistent storage.

### Step 3: Create Tunnel Sender

```rust
let (sender, handle) = TunnelSender::new(
    host,
    ws_url,
    client_id,
    access_token,
    config_db,
);
```

`TunnelSender::new()` returns two components:

- **sender**: The main tunnel client for connection management
- **handle**: A control handle for operations like closing the connection

### Step 4: Start the Connection

```rust
let mut receiver = sender.start();
```

Calling `start()` initiates the tunnel connection process:

1. Fetches authentication signature token from the host
2. Establishes WebSocket connection with credentials
3. Begins automatic ping/pong heartbeat
4. Returns a channel receiver for incoming messages

### Step 5: Process Messages

```rust
while let Some(message) = receiver.recv().await {
    match message {
        TunnelMessage::Text(text) => { /* Handle text */ },
        TunnelMessage::Binary(bytes) => { /* Handle binary */ },
        TunnelMessage::Ping(data) => { /* Handle ping */ },
        TunnelMessage::Pong(data) => { /* Handle pong */ },
        TunnelMessage::Close => { /* Connection closed */ },
        TunnelMessage::Frame(frame) => { /* Raw frame */ },
    }
}
```

The tunnel sender provides six message types:

- **Text**: UTF-8 string messages (JSON payloads, etc.)
- **Binary**: Raw byte data (file transfers, encoded data)
- **Ping**: Keep-alive signals from server
- **Pong**: Responses to ping messages
- **Close**: Connection closure indication
- **Frame**: Raw WebSocket frame for low-level operations

### Step 6: Cleanup

```rust
handle.close();
```

The handle's `close()` method gracefully shuts down the tunnel connection, cancelling the internal cancellation token and stopping all background tasks.

## Key Concepts

### Automatic Connection Management

The `TunnelSender` handles several operations automatically:

- **Authentication**: Fetches and uses signature tokens transparently
- **Reconnection**: Automatically retries failed connections with exponential backoff
- **Heartbeat**: Sends periodic ping messages to keep connection alive
- **Error Recovery**: Handles network interruptions and reconnects

### Message Priority

The tunnel sender uses a priority queue for outgoing messages. Smaller messages are prioritized to reduce latency for control messages and quick responses.

### Thread Safety

Both `TunnelSender` and `TunnelSenderHandle` are `Clone` and can be safely shared across async tasks. The handle can be used to control the connection from any task.

### Cancellation

The tunnel connection can be cancelled at any time using `handle.close()`. This cancels all pending operations and closes the WebSocket connection cleanly.

## Testing the Example

### Without a Real Server

If you don't have access to a MoosicBox server, the example will still run but fail to connect. You can observe the connection retry logic and error handling:

```bash
RUST_LOG=info cargo run --manifest-path packages/tunnel_sender/examples/basic_connection/Cargo.toml
```

You should see authentication attempts and retry behavior in the logs.

### With a Test Server

For full testing, you can:

1. Set up a local MoosicBox server instance
2. Configure the example with your local server endpoints
3. Use a WebSocket testing tool to send messages to the tunnel
4. Observe the example receiving and processing those messages

## Troubleshooting

### Connection Fails Immediately

**Issue**: Example exits without connecting

**Solutions**:

- Verify the `TUNNEL_HOST` and `TUNNEL_WS_URL` are correct
- Check network connectivity to the server
- Ensure the access token is valid and not expired
- Review logs with `RUST_LOG=debug` for detailed error messages

### Authentication Errors

**Issue**: "Unauthorized response from fetch_signature_token"

**Solutions**:

- Confirm your `TUNNEL_CLIENT_ID` and `TUNNEL_ACCESS_TOKEN` are correct
- Check if the access token has expired
- Verify the client ID is registered with the server

### Connection Drops Frequently

**Issue**: Connection establishes but drops repeatedly

**Solutions**:

- Check network stability
- Verify the server is running and accessible
- Look for firewall or proxy issues blocking WebSocket connections
- Increase logging to see specific error messages

### No Messages Received

**Issue**: Connection succeeds but no messages arrive

**Solutions**:

- This is expected if no data is being sent through the tunnel
- The tunnel waits for server-initiated messages or forwarded requests
- You can test by having another client send messages through the tunnel
- Ping/pong messages should still arrive periodically

## Related Examples

- No other examples exist yet for this package
- See `packages/web_server/examples/simple_get/` for HTTP server patterns
- See `packages/async/examples/cancel/` for cancellation token usage patterns
