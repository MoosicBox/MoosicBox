# MoosicBox WebSocket

WebSocket message handling and communication abstractions for MoosicBox session management and real-time updates.

## Overview

The MoosicBox WebSocket package provides:

- **Message Processing**: Handles inbound/outbound WebSocket message payloads
- **Session Management**: WebSocket-based session creation, updates, and deletion
- **Connection Tracking**: Manages WebSocket client connections and registration
- **Broadcasting Abstractions**: `WebsocketSender` trait for sending messages to clients
- **Audio Zone Integration**: WebSocket support for audio zone and player management

## Architecture

This package is designed as a **framework-agnostic message handler**. It does not provide:

- WebSocket server implementation (handled by integration layer)
- Network transport (provided by framework like `actix-web`, `axum`, etc.)
- Authentication implementation (uses `moosicbox_session` integration)

Instead, it provides the core message processing logic that can be integrated with any WebSocket server framework through the `WebsocketSender` trait.

## Features

### Message Types

**Inbound Messages** (Client → Server):

- `Ping` - Connection heartbeat
- `GetConnectionId` - Request connection identifier
- `GetSessions` - Request active sessions list
- `CreateSession` - Create new playback session
- `UpdateSession` - Update session state (play/pause/seek/volume)
- `DeleteSession` - Delete playback session
- `RegisterConnection` - Register new WebSocket connection
- `RegisterPlayers` - Register audio players for a connection
- `CreateAudioZone` - Create multi-device audio zone
- `SetSeek` - Synchronize seek position across clients

**Outbound Messages** (Server → Client):

- `ConnectionId` - Connection identifier response
- `Sessions` - Active sessions list
- `SessionUpdated` - Session state change notification
- `AudioZoneWithSessions` - Audio zones with associated sessions
- `DownloadEvent` - Download progress events
- `ScanEvent` - Library scan progress events
- `Connections` - Active connections list
- `SetSeek` - Seek position synchronization

### Connection Management

The package tracks active connections using an in-memory `BTreeMap`:

- Connects clients and assigns connection IDs
- Registers connections with database
- Handles disconnection and cleanup
- Broadcasts connection status to all clients

## Usage

### Implementing WebSocket Integration

To integrate this package with a WebSocket server framework, implement the `WebsocketSender` trait:

```rust
use moosicbox_ws::{WebsocketSender, WebsocketSendError};
use async_trait::async_trait;

struct MyWebSocketHandler {
    // Your WebSocket connection management
}

#[async_trait]
impl WebsocketSender for MyWebSocketHandler {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        // Send message to specific connection
        todo!()
    }

    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        // Broadcast message to all connections
        todo!()
    }

    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        // Broadcast to all except specified connection
        todo!()
    }

    async fn ping(&self) -> Result<(), WebsocketSendError> {
        // Send ping to maintain connections
        todo!()
    }
}
```

### Processing Messages

```rust
use moosicbox_ws::{process_message, WebsocketContext};
use switchy_database::config::ConfigDatabase;

async fn handle_websocket_message(
    config_db: &ConfigDatabase,
    body: serde_json::Value,
    sender: &impl WebsocketSender,
    connection_id: String,
    profile: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = WebsocketContext {
        connection_id,
        profile,
        player_actions: vec![],
    };

    let response = process_message(config_db, body, context, sender).await?;

    println!("Response: {} - {}", response.status_code, response.body);
    Ok(())
}
```

### Connection Lifecycle

```rust
use moosicbox_ws::{connect, disconnect, WebsocketContext};

fn on_connect(sender: &impl WebsocketSender, connection_id: String) {
    let context = WebsocketContext {
        connection_id,
        profile: None,
        player_actions: vec![],
    };

    let response = connect(sender, &context);
    println!("Connected: {}", response.body);
}

async fn on_disconnect(
    db: &ConfigDatabase,
    sender: &impl WebsocketSender,
    connection_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = WebsocketContext {
        connection_id,
        profile: None,
        player_actions: vec![],
    };

    let response = disconnect(db, sender, &context).await?;
    println!("Disconnected: {}", response.body);
    Ok(())
}
```

### Session Management

```rust
use moosicbox_ws::{update_session, broadcast_sessions};
use moosicbox_session::models::UpdateSession;
use switchy_database::profiles::PROFILES;

async fn update_playback(
    config_db: &ConfigDatabase,
    sender: &impl WebsocketSender,
    session_id: u64,
    playing: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let profile = "default";
    let db = PROFILES.get(profile).ok_or("Profile not found")?;

    let update = UpdateSession {
        session_id,
        profile: profile.to_string(),
        playing,
        ..Default::default()
    };

    // Update session and broadcast to all clients
    update_session(config_db, &db, sender, None, &update).await?;

    Ok(())
}
```

### Broadcasting Events

```rust
use moosicbox_ws::{send_download_event, send_scan_event};
use serde::Serialize;

#[derive(Serialize)]
struct DownloadProgress {
    file_id: u64,
    bytes_downloaded: u64,
    total_bytes: u64,
}

async fn notify_download_progress(
    sender: &impl WebsocketSender,
) -> Result<(), Box<dyn std::error::Error>> {
    let progress = DownloadProgress {
        file_id: 123,
        bytes_downloaded: 1024 * 500,
        total_bytes: 1024 * 1024,
    };

    send_download_event(sender, None, progress).await?;
    Ok(())
}

#[derive(Serialize)]
struct ScanProgress {
    scanned_files: u32,
    total_files: u32,
}

async fn notify_scan_progress(
    sender: &impl WebsocketSender,
) -> Result<(), Box<dyn std::error::Error>> {
    let progress = ScanProgress {
        scanned_files: 42,
        total_files: 100,
    };

    send_scan_event(sender, None, progress).await?;
    Ok(())
}
```

### Player Action Callbacks

```rust
use moosicbox_ws::WebsocketContext;
use moosicbox_session::models::UpdateSession;
use std::pin::Pin;
use std::future::Future;

fn my_player_action(
    update: &UpdateSession
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let session_id = update.session_id;
    let playing = update.playing;

    Box::pin(async move {
        // Handle player-specific playback changes
        println!("Session {} playing state: {:?}", session_id, playing);
    })
}

async fn setup_context_with_player_actions(player_id: u64) -> WebsocketContext {
    WebsocketContext {
        connection_id: "conn-123".to_string(),
        profile: Some("default".to_string()),
        player_actions: vec![(player_id, my_player_action)],
    }
}
```

## Dependencies

Based on `Cargo.toml`:

```toml
[dependencies]
moosicbox_assert     = { workspace = true }
moosicbox_audio_zone = { workspace = true }
moosicbox_json_utils = { workspace = true, features = ["database"] }
moosicbox_logging    = { workspace = true, features = ["macros"] }
moosicbox_session    = { workspace = true }
switchy_database     = { workspace = true }

async-trait = { workspace = true, optional = true }
log         = { workspace = true, optional = true }
thiserror   = { workspace = true, optional = true }

serde        = { workspace = true, features = ["derive"] }
serde_json   = { workspace = true }
strum        = { workspace = true }
strum_macros = { workspace = true }
```

## Feature Flags

- `ws` - Core WebSocket functionality (enables `async-trait`, `log`, `thiserror`)
- `aac` - AAC codec support (via `moosicbox_session`)
- `flac` - FLAC codec support (via `moosicbox_session`)
- `mp3` - MP3 codec support (via `moosicbox_session`)
- `opus` - Opus codec support (via `moosicbox_session`)

Default features: `["aac", "flac", "mp3", "opus", "ws"]`

## Error Handling

```rust
use moosicbox_ws::{
    WebsocketSendError,
    WebsocketMessageError,
    WebsocketDisconnectError,
    UpdateSessionError,
};

async fn handle_errors(
    sender: &impl WebsocketSender,
) -> Result<(), WebsocketSendError> {
    match sender.send("conn-id", "message").await {
        Ok(()) => println!("Message sent successfully"),
        Err(WebsocketSendError::DatabaseFetch(e)) => {
            eprintln!("Database error: {}", e);
        },
        Err(WebsocketSendError::Serde(e)) => {
            eprintln!("Serialization error: {}", e);
        },
        Err(WebsocketSendError::Unknown(msg)) => {
            eprintln!("Unknown error: {}", msg);
        },
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    Ok(())
}
```

## Module Structure

```
packages/ws/src/
├── lib.rs          # Package entry point
├── models.rs       # Message payload definitions (InboundPayload, OutboundPayload)
└── ws.rs           # Core WebSocket message processing logic
```

## Integration with MoosicBox

This package is used by MoosicBox server implementations to handle WebSocket communication. The server provides:

- WebSocket upgrade handling
- Connection management
- `WebsocketSender` implementation
- Authentication integration

For example usage in a real server, see the MoosicBox server packages that integrate this library.

## See Also

- [MoosicBox Session](../session/README.md) - Session management and playback state
- [MoosicBox Audio Zone](../audio_zone/README.md) - Multi-device audio synchronization
