# MoosicBox Tauri WebSocket

WebSocket integration for MoosicBox Tauri applications.

## Overview

The MoosicBox Tauri WebSocket package provides:

- **WebSocket Client**: Tauri-specific WebSocket client implementation
- **Real-time Communication**: Live updates and messaging
- **Event Integration**: WebSocket event handling for Tauri
- **Connection Management**: Automatic reconnection and error handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_tauri_ws = { path = "../app/tauri/ws" }
```

## Dependencies

- **Tauri**: Desktop application framework
- **MoosicBox WebSocket**: Core WebSocket functionality
- **Tokio**: Async runtime
