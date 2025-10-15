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

## Features

- **WsClient**: Async WebSocket client with automatic reconnection
- **WsHandle**: Connection handle for sending messages and closing connections
- **WebsocketSender Trait**: Abstraction for sending WebSocket messages
- **Authentication Support**: Client ID and signature token authentication
- **Automatic Ping/Pong**: Built-in keep-alive mechanism with 5-second intervals
- **Profile-based Connections**: Support for MoosicBox profile routing
- **Error Handling**: Comprehensive error types for different failure scenarios
- **Graceful Shutdown**: Cancellation token support for clean connection termination

## Dependencies

Core dependencies:

- **tokio-tungstenite**: WebSocket protocol implementation
- **tokio**: Async runtime with macros, time, and tracing features
- **futures-channel**: Unbounded MPSC channels for message passing
- **futures-util**: Stream utilities and async combinators
- **bytes**: Efficient byte buffer handling
- **async-trait**: Async trait support
- **switchy_async**: Async runtime abstraction with cancellation support
- **log**: Logging facade
- **thiserror**: Error type derivation

MoosicBox dependencies:

- **moosicbox_assert**: Assertion utilities
- **moosicbox_env_utils**: Environment utilities
- **moosicbox_logging**: Logging utilities with macros
