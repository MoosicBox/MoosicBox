# MoosicBox Tauri WebSocket

WebSocket client implementation for MoosicBox Tauri applications.

## Overview

The MoosicBox Tauri WebSocket package provides:

- **WebSocket Client**: Tauri-specific async WebSocket client with message handling
- **Real-time Communication**: Live updates and messaging for Tauri applications
- **Connection Management**: Automatic reconnection and error handling
- **Cancellation Support**: Graceful shutdown via cancellation tokens

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_ws = { path = "../app/tauri/ws" }
```

## Usage

### Creating a WebSocket Client

```rust
use moosicbox_app_ws::{WsClient, WsMessage};
use switchy_async::sync::mpsc;

let (client, handle) = WsClient::new("ws://localhost:8080".to_string());
let (tx, mut rx) = mpsc::unbounded();

// Start the WebSocket connection
client.start(
    None,                    // client_id
    None,                    // signature_token
    "default".to_string(),   // profile
    || println!("Connected"),
    tx,
).await?;
```

### Sending Messages

```rust
use moosicbox_app_ws::WebsocketSender;

// Send text message
handle.send("Hello, server!").await?;

// Send ping
handle.ping().await?;
```

### Closing the Connection

```rust
handle.close();
```

## API

### `WsClient`

- `new(url: String) -> (Self, WsHandle)`: Creates a new WebSocket client and handle
- `with_cancellation_token(token: CancellationToken) -> Self`: Sets a custom cancellation token
- `start(...)`: Starts the WebSocket connection with automatic reconnection

### `WsHandle`

- `close()`: Closes the WebSocket connection
- Implements `WebsocketSender` trait

### `WebsocketSender` trait

- `async fn send(&self, data: &str) -> Result<(), WebsocketSendError>`: Sends a text message
- `async fn ping(&self) -> Result<(), WebsocketSendError>`: Sends a ping message

### `WsMessage` enum

- `TextMessage(String)`: Text message
- `Message(Bytes)`: Binary message
- `Ping`: Ping message
