# Basic WebSocket Client Usage Example

A comprehensive example demonstrating how to use the `moosicbox_app_ws` WebSocket client for real-time communication with a WebSocket server.

## Summary

This example shows how to create a WebSocket client, establish a connection, send and receive messages, and handle graceful shutdown with proper error handling and automatic reconnection.

## What This Example Demonstrates

- Creating a `WsClient` instance and obtaining a `WsHandle`
- Configuring a client with a custom cancellation token
- Starting a WebSocket connection with automatic reconnection
- Receiving messages asynchronously via channels
- Sending text messages using the `WebsocketSender` trait
- Sending ping messages to keep connections alive
- Handling different message types (text, binary, ping)
- Graceful shutdown and connection cleanup
- Proper error handling for connection and send failures

## Prerequisites

- A WebSocket server running on `ws://localhost:8080`
- To quickly start a test WebSocket server, you can use:
    - `websocat -s 8080` (if you have [websocat](https://github.com/vi/websocat) installed)
    - Or use any WebSocket echo server
    - Or modify the `websocket_url` in the example to connect to a different server

## Running the Example

```bash
# From the repository root
cargo run --manifest-path packages/app/tauri/ws/examples/basic_usage/Cargo.toml

# Or from the example directory
cd packages/app/tauri/ws/examples/basic_usage
cargo run
```

## Expected Output

When running against a WebSocket server, you should see output similar to:

```
=== MoosicBox WebSocket Client Example ===

Message receiver started

✅ WebSocket connected!

Starting to send messages...

📤 Sending: Hello from client, message #1
📥 Received text: Echo: Hello from client, message #1
🏓 Received ping from server
📤 Sending: Hello from client, message #2
📥 Received text: Echo: Hello from client, message #2
...

Finished sending messages

⏱️  Timeout reached, shutting down...

🛑 Closing WebSocket connection...

Message receiver stopped

✨ Example completed successfully!
```

## Code Walkthrough

### 1. Creating the Client and Handle

```rust
let (client, handle) = WsClient::new(websocket_url);
```

`WsClient::new()` returns a tuple containing:

- `client`: The WebSocket client that manages the connection lifecycle
- `handle`: A handle that allows you to send messages and close the connection

### 2. Setting Up Cancellation

```rust
let cancellation_token = CancellationToken::new();
let client = client.with_cancellation_token(client_token);
```

Cancellation tokens enable graceful shutdown. When cancelled, the client will cleanly close the WebSocket connection and stop all tasks.

### 3. Creating a Message Channel

```rust
let (tx, mut rx) = mpsc::channel::<WsMessage>(100);
```

The channel is used to receive messages from the WebSocket server. The sender (`tx`) is passed to the client's `start()` method, and messages can be received from the receiver (`rx`).

### 4. Starting the Connection

```rust
client.start(
    None,                  // client_id (optional)
    None,                  // signature_token (optional)
    "default".to_string(), // profile name
    || { println!("✅ WebSocket connected!"); },
    tx,
).await
```

The `start()` method:

- Establishes the WebSocket connection
- Automatically reconnects on connection failures with exponential backoff
- Calls the provided callback when connection is established
- Sends received messages through the `tx` channel
- Runs until cancelled or encounters an unrecoverable error

### 5. Receiving Messages

```rust
while let Some(msg) = rx.recv().await {
    match msg {
        WsMessage::TextMessage(text) => { /* handle text */ },
        WsMessage::Message(bytes) => { /* handle binary */ },
        WsMessage::Ping => { /* handle ping */ },
    }
}
```

Messages are received asynchronously from the channel. The example handles three message types: text messages, binary messages, and ping messages.

### 6. Sending Messages

```rust
sender_handle.send(&message).await?;
sender_handle.ping().await?;
```

The `WsHandle` implements the `WebsocketSender` trait, which provides:

- `send(&self, data: &str)`: Sends text messages
- `ping(&self)`: Sends ping messages to keep the connection alive

### 7. Closing the Connection

```rust
handle.close();
```

Calling `close()` on the handle triggers the cancellation token, which causes the client to cleanly shut down the connection and stop all tasks.

## Key Concepts

### Automatic Reconnection

The `WsClient` automatically attempts to reconnect if the connection drops. It uses exponential backoff to avoid overwhelming the server with reconnection attempts.

### Separation of Concerns

- **`WsClient`**: Manages the connection lifecycle and message routing
- **`WsHandle`**: Provides an interface for sending messages and closing the connection
- **`WebsocketSender` trait**: Defines the contract for sending different message types

### Async Message Handling

Messages are received and processed asynchronously using Tokio channels, allowing your application to handle messages without blocking the WebSocket connection.

### Graceful Shutdown

Using cancellation tokens ensures that all tasks are properly cleaned up when the connection is closed, preventing resource leaks.

## Testing the Example

1. **Start a WebSocket server** on port 8080:

    ```bash
    # Using websocat
    websocat -s 8080

    # Or use any WebSocket echo server
    ```

2. **Run the example** in another terminal:

    ```bash
    cargo run --manifest-path packages/app/tauri/ws/examples/basic_usage/Cargo.toml
    ```

3. **Observe the output** showing connection establishment, message exchange, and shutdown

4. **Test reconnection** by stopping and restarting the server while the example is running - you should see the client automatically reconnect

## Troubleshooting

### Connection Refused

**Problem**: `Failed to connect to websocket server: Os { code: 111, kind: ConnectionRefused }`

**Solution**: Ensure a WebSocket server is running on `ws://localhost:8080`. Start one using `websocat -s 8080` or modify the URL in the example.

### Authentication Errors

**Problem**: `Unauthorized ws connection`

**Solution**: If your WebSocket server requires authentication, provide `client_id` and `signature_token` to the `start()` method:

```rust
client.start(
    Some("your-client-id".to_string()),
    Some("your-signature-token".to_string()),
    "default".to_string(),
    || {},
    tx,
).await
```

### No Messages Received

**Problem**: The example connects but no messages are received

**Solution**: Ensure your WebSocket server echoes messages back. Some servers only receive messages but don't send responses. Try using a WebSocket echo server for testing.

## Related Examples

This is currently the only example for `moosicbox_app_ws`. For related networking examples, see:

- `packages/ws/` - Server-side WebSocket handling
- `packages/tunnel_sender/` - WebSocket-based tunnel sender implementation
