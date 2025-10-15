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

## Dependencies

- **tokio-tungstenite**: WebSocket protocol implementation
- **Tokio**: Async runtime
- **futures**: Async stream and channel utilities
- **bytes**: Efficient byte buffer handling
