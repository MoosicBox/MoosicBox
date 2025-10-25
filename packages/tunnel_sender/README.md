# MoosicBox Tunnel Sender

Basic WebSocket tunnel communication library for the MoosicBox ecosystem, providing simple message passing and data streaming capabilities over WebSocket connections.

## Features

- **WebSocket Communication**: Basic WebSocket-based messaging
- **Message Types**: Support for text, binary, ping/pong, and frame messages
- **Error Handling**: Basic error types for tunnel operations
- **Tunnel Request Processing**: Handling of tunnel-based HTTP-like requests
- **Audio Streaming Support**: Backend support for streaming audio through tunnels

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_tunnel_sender = "0.1.4"
```

## Usage

### Basic Message Types

```rust
use moosicbox_tunnel_sender::TunnelMessage;
use bytes::Bytes;

// Different message types supported
let text_msg = TunnelMessage::Text("Hello".to_string());
let binary_msg = TunnelMessage::Binary(Bytes::from(b"data".to_vec()));
let ping_msg = TunnelMessage::Ping(vec![1, 2, 3]);
let pong_msg = TunnelMessage::Pong(vec![4, 5, 6]);
let close_msg = TunnelMessage::Close;
// Frame messages are also supported for low-level WebSocket operations
// let frame_msg = TunnelMessage::Frame(frame);
```

### Error Handling

```rust
use moosicbox_tunnel_sender::{SendBytesError, SendMessageError, TunnelRequestError};

// Handle different error types
match send_result {
    Err(TunnelRequestError::BadRequest(msg)) => {
        eprintln!("Bad request: {}", msg);
    }
    Err(TunnelRequestError::NotFound(msg)) => {
        eprintln!("Not found: {}", msg);
    }
    Err(TunnelRequestError::InternalServerError(err)) => {
        eprintln!("Server error: {}", err);
    }
    Ok(response) => {
        println!("Success!");
    }
}
```

### Sender Operations

The library provides `TunnelSender` for establishing and managing tunnel connections, handling message routing, and processing tunnel requests.

## Error Types

The library provides several error types for different failure scenarios:

- `SendBytesError`: Errors when sending binary data
- `SendMessageError`: Errors when sending messages
- `TunnelRequestError`: Comprehensive error type for tunnel requests including:
    - Bad requests
    - Not found errors
    - Invalid queries
    - WebSocket message errors
    - I/O errors
    - JSON serialization errors

## Dependencies

This library integrates with other MoosicBox components:

- `moosicbox_music_api`: For music API integration
- `moosicbox_music_models`: For audio format and source types
- `moosicbox_ws`: For WebSocket message handling
- `tokio-tungstenite`: For WebSocket frame support
