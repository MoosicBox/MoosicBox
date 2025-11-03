# WebSocket Message Handler Example

A practical demonstration of implementing WebSocket message handling and connection lifecycle management using `moosicbox_ws`.

## Summary

This example shows how to implement the `WebsocketSender` trait to create a basic WebSocket message handler, manage connections, and broadcast messages to clients. It provides a foundation for integrating `moosicbox_ws` with any WebSocket server framework.

## What This Example Demonstrates

- **WebsocketSender trait implementation** - Creating a custom message sender with in-memory connection tracking
- **Connection lifecycle management** - Handling client connections and disconnections
- **Message broadcasting patterns** - Sending messages to specific connections, all connections, or all except one
- **Connection state tracking** - Managing connection metadata using `WebsocketContext`
- **Message type handling** - Working with `InboundPayload` and `OutboundPayload` message types
- **Ping/keepalive mechanism** - Maintaining active connections

## Prerequisites

- Basic understanding of asynchronous Rust programming
- Familiarity with WebSocket concepts
- Knowledge of JSON serialization with `serde`

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/ws/examples/basic_message_handler/Cargo.toml
```

Or from the example directory:

```bash
cd packages/ws/examples/basic_message_handler
cargo run
```

## Expected Output

When you run the example, you should see output similar to:

```
🚀 MoosicBox WebSocket Message Handler Example
================================================

Step 1: Creating WebSocket sender...
✅ WebSocket sender created

Step 2: Simulating client connections...
✅ Client 1 connected: client-abc-123 (Status: 200)
✅ Client 2 connected: client-xyz-456 (Status: 200)
📊 Total active connections: 2

Step 3: Sending targeted messages...
✉️  Sent to [client-abc-123]: {"type":"CONNECTION_ID","connectionId":"client-abc-123"}

Step 4: Broadcasting to all connections...
📢 Broadcast to [client-abc-123]: {"type":"SESSIONS","payload":[]}
📢 Broadcast to [client-xyz-456]: {"type":"SESSIONS","payload":[]}

Step 5: Broadcasting to all except client 1...
📢 Broadcast to [client-xyz-456] (except client-abc-123): {"type":"SESSION_UPDATED"...}

Step 6: Sending ping to all connections...
🏓 Ping sent to 2 connection(s)

Step 7: Processing inbound messages...
📨 Received inbound message: Ping(EmptyPayload {})

Step 8: Message summary...
Messages received by client 1:
  1. {"type":"CONNECTION_ID","connectionId":"client-abc-123"}
  2. {"type":"SESSIONS","payload":[]}

Messages received by client 2:
  1. {"type":"SESSIONS","payload":[]}
  2. {"type":"SESSION_UPDATED","payload":{"sessionId":1,"playing":true}}

Step 9: Disconnecting clients...
✅ Client 1 disconnected
✅ Client 2 disconnected
📊 Total active connections: 0

================================================
✨ Example completed successfully!
```

## Code Walkthrough

### 1. Implementing the WebsocketSender Trait

The core of the example is the `SimpleWebsocketSender` struct that implements the `WebsocketSender` trait:

```rust
struct SimpleWebsocketSender {
    connections: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

#[async_trait]
impl WebsocketSender for SimpleWebsocketSender {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        // Send to specific connection
    }

    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        // Broadcast to all connections
    }

    async fn send_all_except(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        // Broadcast to all except one
    }

    async fn ping(&self) -> Result<(), WebsocketSendError> {
        // Send keepalive ping
    }
}
```

This trait is the bridge between `moosicbox_ws` and your WebSocket framework. In production, you would replace the `HashMap<String, Vec<String>>` with actual WebSocket connections.

### 2. Creating WebSocket Context

The `WebsocketContext` struct carries connection metadata:

```rust
let context = WebsocketContext {
    connection_id: "client-abc-123".to_string(),
    profile: Some("default".to_string()),
    player_actions: vec![],
};
```

- `connection_id` - Unique identifier for the connection
- `profile` - Optional profile name for database operations
- `player_actions` - Optional callbacks triggered on session updates

### 3. Handling Connection Lifecycle

The `connect()` function registers a new connection:

```rust
let response = connect(&sender, &context);
println!("Connected: {} (Status: {})", context.connection_id, response.status_code);
```

For disconnection, you would use:

```rust
// Requires a database connection
disconnect(&config_db, &sender, &context).await?;
```

### 4. Message Broadcasting Patterns

**Send to specific connection:**

```rust
sender.send(&connection_id, r#"{"type":"CONNECTION_ID","connectionId":"..."}"#).await?;
```

**Broadcast to all:**

```rust
sender.send_all(r#"{"type":"SESSIONS","payload":[]}"#).await?;
```

**Broadcast to all except one:**

```rust
sender.send_all_except(&connection_id, r#"{"type":"SESSION_UPDATED",...}"#).await?;
```

### 5. Processing Inbound Messages

Inbound messages are deserialized from JSON using `InboundPayload`:

```rust
let message = serde_json::json!({"type": "PING"});
let payload: InboundPayload = serde_json::from_value(message)?;

// In production, process with database:
// process_message(&config_db, body, context, &sender).await?;
```

## Key Concepts

### Framework-Agnostic Design

`moosicbox_ws` does not provide a WebSocket server implementation. Instead, it provides the message processing logic that integrates with any WebSocket framework (actix-web, axum, tokio-tungstenite, etc.) through the `WebsocketSender` trait.

### Message Types

The package uses typed enums for messages:

- **InboundPayload** - Client-to-server messages (Ping, GetSessions, CreateSession, UpdateSession, etc.)
- **OutboundPayload** - Server-to-client messages (ConnectionId, Sessions, SessionUpdated, etc.)

All messages are serialized as JSON with a `type` field that identifies the message type.

### State Management

The package maintains connection state in-memory using a static `LazyLock<Arc<RwLock<BTreeMap>>>`. This is managed internally by the `connect()`, `disconnect()`, and `register_connection()` functions.

### Async Trait Implementation

The `WebsocketSender` trait requires the `async_trait` macro because it contains async methods. This is a common pattern in async Rust when trait methods need to be async.

## Testing the Example

You can verify the example works correctly by checking:

1. **Connection count changes** - Starts at 0, increases to 2, returns to 0
2. **Message delivery** - Each client receives appropriate messages
3. **Selective broadcasting** - `send_all_except()` excludes the specified connection
4. **Ping functionality** - Ping is sent to all active connections

## Troubleshooting

### Compilation Errors

If you encounter compilation errors, ensure:

- You're using the correct Rust edition (2021)
- All workspace dependencies are available
- The `async-trait` dependency is included

### Missing Dependencies

The example requires these dependencies:

```toml
[dependencies]
moosicbox_ws = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "sync"] }
```

### Understanding the Output

The emojis in the output indicate different operations:

- 🚀 - Example startup
- ✅ - Successful operation
- ✉️ - Direct message to specific connection
- 📢 - Broadcast message
- 🏓 - Ping/keepalive
- 📨 - Inbound message received
- 📊 - Statistics/summary
- ✨ - Completion
- 💡 - Tips/insights

## Related Examples

This is currently the only example for `moosicbox_ws`. For related functionality, see:

- [MoosicBox Session](../../session/README.md) - Session management integration
- [MoosicBox Audio Zone](../../audio_zone/README.md) - Multi-device audio synchronization

## Next Steps

To build a production WebSocket server using `moosicbox_ws`:

1. **Choose a WebSocket framework** - Select a framework like actix-web, axum, or tokio-tungstenite
2. **Implement WebsocketSender** - Create a real implementation backed by actual WebSocket connections
3. **Set up database integration** - Use `switchy_database` with `ConfigDatabase` and `LibraryDatabase`
4. **Handle authentication** - Integrate with `moosicbox_session` for user authentication
5. **Process messages** - Call `process_message()` for each incoming WebSocket message
6. **Manage lifecycle** - Call `connect()` and `disconnect()` at appropriate times
7. **Handle errors** - Implement proper error handling for `WebsocketSendError`, `WebsocketMessageError`, etc.

For a complete reference implementation, see the MoosicBox server packages that use this library in production.
