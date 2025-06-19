# MoosicBox WebSocket

Real-time bidirectional communication system for live music playback control, status updates, and multi-client synchronization.

## Overview

The MoosicBox WebSocket package provides:

- **Real-Time Communication**: Instant playback control and status updates
- **Multi-Client Sync**: Synchronized playback across multiple devices
- **Authentication Integration**: Secure WebSocket connections with token-based auth
- **Event Broadcasting**: Efficient message routing and client management
- **Connection Management**: Automatic reconnection and heartbeat monitoring
- **Scalable Architecture**: Support for horizontal scaling and load balancing

## Features

### Real-Time Events
- **Playback Control**: Play, pause, skip, seek commands
- **Status Updates**: Track changes, progress updates, volume changes
- **Queue Management**: Real-time queue updates and modifications
- **Library Updates**: Live library scanning and metadata updates
- **User Presence**: Multi-user session awareness

### Connection Management
- **Auto-Reconnection**: Automatic reconnection with exponential backoff
- **Heartbeat Monitoring**: Connection health checking and timeout handling
- **Session Recovery**: Resume sessions after disconnection
- **Graceful Degradation**: Fallback to polling for unreliable connections

### Security & Authentication
- **Token-Based Auth**: JWT token authentication for WebSocket connections
- **Permission-Based Filtering**: Role-based message filtering
- **Rate Limiting**: Per-connection and per-user rate limiting
- **Origin Validation**: Cross-origin request security

## Usage

### Basic WebSocket Server

```rust
use moosicbox_ws::{WebSocketServer, WebSocketConfig, MessageHandler};
use moosicbox_auth::AuthManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure WebSocket server
    let config = WebSocketConfig {
        bind_address: "0.0.0.0:8080".to_string(),
        max_connections: 1000,
        heartbeat_interval_seconds: 30,
        connection_timeout_seconds: 60,
        max_message_size: 1024 * 1024, // 1MB
        enable_compression: true,
    };

    // Create auth manager for authentication
    let auth = AuthManager::new(auth_config).await?;

    // Create WebSocket server
    let ws_server = WebSocketServer::new(config, auth).await?;

    // Start server
    ws_server.start().await?;

    println!("WebSocket server listening on ws://0.0.0.0:8080");

    Ok(())
}
```

### Client Connection and Authentication

```rust
use moosicbox_ws::{WebSocketClient, ClientConfig, WebSocketMessage};

async fn connect_client() -> Result<(), Box<dyn std::error::Error>> {
    // Configure client
    let client_config = ClientConfig {
        url: "ws://localhost:8080".to_string(),
        auth_token: Some("your_jwt_token".to_string()),
        reconnect_attempts: 5,
        reconnect_delay_ms: 1000,
        heartbeat_interval_seconds: 30,
    };

    // Create and connect client
    let mut client = WebSocketClient::new(client_config).await?;

    // Handle incoming messages
    client.on_message(|message| async move {
        match message {
            WebSocketMessage::PlaybackStatus(status) => {
                println!("Playback status: {:?}", status);
            },
            WebSocketMessage::TrackChanged(track) => {
                println!("Now playing: {} - {}", track.artist, track.title);
            },
            WebSocketMessage::QueueUpdated(queue) => {
                println!("Queue updated: {} tracks", queue.tracks.len());
            },
            _ => {},
        }
    });

    // Send playback command
    client.send(WebSocketMessage::PlaybackCommand {
        command: "play".to_string(),
        track_id: Some(12345),
    }).await?;

    // Keep connection alive
    client.run().await?;

    Ok(())
}
```

### Message Broadcasting

```rust
use moosicbox_ws::{WebSocketServer, BroadcastMessage, ClientGroup};

async fn broadcast_messages(ws_server: &WebSocketServer) -> Result<(), Box<dyn std::error::Error>> {
    // Broadcast to all connected clients
    let status_message = BroadcastMessage::PlaybackStatus {
        is_playing: true,
        current_track: Some(track_info),
        position_seconds: 120,
        volume: 0.8,
    };

    ws_server.broadcast_to_all(status_message).await?;

    // Broadcast to specific user's sessions
    let user_id = 123;
    let user_message = BroadcastMessage::UserNotification {
        message: "Your playlist has been updated".to_string(),
        level: "info".to_string(),
    };

    ws_server.broadcast_to_user(user_id, user_message).await?;

    // Broadcast to clients in a specific room
    let room_id = "listening_party_456";
    let room_message = BroadcastMessage::RoomEvent {
        event_type: "user_joined".to_string(),
        data: serde_json::json!({
            "user_id": 789,
            "username": "music_lover"
        }),
    };

    ws_server.broadcast_to_room(room_id, room_message).await?;

    // Broadcast to clients with specific permissions
    let admin_message = BroadcastMessage::AdminNotification {
        message: "Server maintenance scheduled".to_string(),
        scheduled_time: chrono::Utc::now() + chrono::Duration::hours(2),
    };

    ws_server.broadcast_to_permission("admin", admin_message).await?;

    Ok(())
}
```

### Playback Control Integration

```rust
use moosicbox_ws::{WebSocketServer, PlaybackEventHandler};
use moosicbox_player::Player;

struct PlaybackHandler {
    player: Player,
    ws_server: WebSocketServer,
}

impl PlaybackEventHandler for PlaybackHandler {
    async fn handle_play_command(&self, track_id: Option<u64>) -> Result<(), Box<dyn std::error::Error>> {
        // Execute playback command
        match track_id {
            Some(id) => {
                self.player.play_track_by_id(id).await?;
            },
            None => {
                self.player.resume().await?;
            }
        }

        // Broadcast status update
        let status = self.player.get_status().await?;
        self.ws_server.broadcast_to_all(BroadcastMessage::PlaybackStatus {
            is_playing: status.is_playing,
            current_track: status.current_track,
            position_seconds: status.position_seconds,
            volume: status.volume,
        }).await?;

        Ok(())
    }

    async fn handle_pause_command(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.player.pause().await?;

        let status = self.player.get_status().await?;
        self.ws_server.broadcast_to_all(BroadcastMessage::PlaybackStatus {
            is_playing: false,
            current_track: status.current_track,
            position_seconds: status.position_seconds,
            volume: status.volume,
        }).await?;

        Ok(())
    }

    async fn handle_seek_command(&self, position_seconds: f64) -> Result<(), Box<dyn std::error::Error>> {
        self.player.seek(position_seconds).await?;

        self.ws_server.broadcast_to_all(BroadcastMessage::SeekUpdate {
            position_seconds,
        }).await?;

        Ok(())
    }

    async fn handle_volume_command(&self, volume: f64) -> Result<(), Box<dyn std::error::Error>> {
        self.player.set_volume(volume).await?;

        self.ws_server.broadcast_to_all(BroadcastMessage::VolumeUpdate {
            volume,
        }).await?;

        Ok(())
    }

    async fn handle_queue_command(&self, action: QueueAction) -> Result<(), Box<dyn std::error::Error>> {
        match action {
            QueueAction::Add { track_id, position } => {
                self.player.add_to_queue(track_id, position).await?;
            },
            QueueAction::Remove { queue_item_id } => {
                self.player.remove_from_queue(queue_item_id).await?;
            },
            QueueAction::Reorder { from, to } => {
                self.player.reorder_queue(from, to).await?;
            },
            QueueAction::Clear => {
                self.player.clear_queue().await?;
            },
        }

        let queue = self.player.get_queue().await?;
        self.ws_server.broadcast_to_all(BroadcastMessage::QueueUpdated {
            queue,
        }).await?;

        Ok(())
    }
}
```

### Real-Time Progress Updates

```rust
use moosicbox_ws::{WebSocketServer, ProgressTracker};
use tokio::time::{interval, Duration};

async fn setup_progress_updates(
    ws_server: WebSocketServer,
    player: Player
) -> Result<(), Box<dyn std::error::Error>> {
    let mut progress_interval = interval(Duration::from_millis(500)); // 2Hz updates

    loop {
        progress_interval.tick().await;

        if let Ok(status) = player.get_status().await {
            if status.is_playing {
                // Send progress update to all clients
                ws_server.broadcast_to_all(BroadcastMessage::ProgressUpdate {
                    position_seconds: status.position_seconds,
                    duration_seconds: status.duration_seconds,
                    buffered_seconds: status.buffered_seconds,
                }).await?;
            }
        }
    }
}
```

### Room-Based Communication

```rust
use moosicbox_ws::{WebSocketServer, Room, RoomManager};

async fn room_management(ws_server: &WebSocketServer) -> Result<(), Box<dyn std::error::Error>> {
    // Create a listening party room
    let room_config = RoomConfig {
        id: "listening_party_123".to_string(),
        name: "Jazz Night".to_string(),
        description: Some("Listening to classic jazz together".to_string()),
        max_participants: 20,
        is_public: true,
        host_user_id: 456,
    };

    let room = ws_server.create_room(room_config).await?;

    // Handle user joining room
    ws_server.on_room_join(|room_id, user_id, client_id| async move {
        // Add user to room
        room.add_participant(user_id, client_id).await?;

        // Notify other participants
        let join_message = BroadcastMessage::RoomEvent {
            event_type: "user_joined".to_string(),
            data: serde_json::json!({
                "user_id": user_id,
                "username": "get_username_from_db"
            }),
        };

        ws_server.broadcast_to_room(&room_id, join_message).await?;

        // Send room state to new participant
        let room_state = BroadcastMessage::RoomState {
            participants: room.get_participants().await?,
            current_track: room.get_current_track().await?,
            playback_position: room.get_playback_position().await?,
        };

        ws_server.send_to_client(client_id, room_state).await?;

        Ok(())
    });

    // Handle user leaving room
    ws_server.on_room_leave(|room_id, user_id, client_id| async move {
        room.remove_participant(user_id, client_id).await?;

        let leave_message = BroadcastMessage::RoomEvent {
            event_type: "user_left".to_string(),
            data: serde_json::json!({
                "user_id": user_id
            }),
        };

        ws_server.broadcast_to_room(&room_id, leave_message).await?;

        Ok(())
    });

    // Synchronized playback in room
    ws_server.on_room_playback_command(|room_id, command| async move {
        match command {
            RoomPlaybackCommand::Play { track_id, start_position } => {
                // Start synchronized playback
                let sync_message = BroadcastMessage::SynchronizedPlayback {
                    command: "play".to_string(),
                    track_id,
                    start_position,
                    sync_timestamp: chrono::Utc::now(),
                };

                ws_server.broadcast_to_room(&room_id, sync_message).await?;
            },
            RoomPlaybackCommand::Pause { position } => {
                let sync_message = BroadcastMessage::SynchronizedPlayback {
                    command: "pause".to_string(),
                    track_id: None,
                    start_position: position,
                    sync_timestamp: chrono::Utc::now(),
                };

                ws_server.broadcast_to_room(&room_id, sync_message).await?;
            },
        }

        Ok(())
    });

    Ok(())
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `WS_BIND_ADDRESS` | WebSocket server bind address | `0.0.0.0:8080` |
| `WS_MAX_CONNECTIONS` | Maximum concurrent connections | `1000` |
| `WS_HEARTBEAT_INTERVAL` | Heartbeat interval in seconds | `30` |
| `WS_CONNECTION_TIMEOUT` | Connection timeout in seconds | `60` |
| `WS_MAX_MESSAGE_SIZE` | Maximum message size in bytes | `1048576` |
| `WS_ENABLE_COMPRESSION` | Enable message compression | `true` |
| `WS_RATE_LIMIT_PER_MINUTE` | Messages per minute per connection | `60` |

### Advanced Configuration

```rust
use moosicbox_ws::{WebSocketConfig, CompressionConfig, RateLimitConfig};

let config = WebSocketConfig {
    bind_address: "0.0.0.0:8080".to_string(),

    // Connection limits
    max_connections: 1000,
    max_connections_per_ip: 10,
    connection_timeout_seconds: 60,

    // Message handling
    max_message_size: 1024 * 1024, // 1MB
    max_queue_size: 100,
    message_buffer_size: 8192,

    // Heartbeat and keepalive
    heartbeat_interval_seconds: 30,
    ping_timeout_seconds: 10,
    close_timeout_seconds: 5,

    // Compression
    compression_config: CompressionConfig {
        enable_compression: true,
        compression_level: 6,
        compression_threshold: 1024,
        compression_window_bits: 15,
    },

    // Rate limiting
    rate_limit_config: RateLimitConfig {
        messages_per_minute: 60,
        bytes_per_minute: 1024 * 1024,
        burst_size: 10,
        cleanup_interval_seconds: 60,
    },

    // Security
    allowed_origins: vec!["https://app.moosicbox.com".to_string()],
    require_auth: true,
    validate_csrf: true,

    // Scaling
    redis_url: Some("redis://localhost:6379".to_string()),
    node_id: "ws-node-1".to_string(),
    cluster_mode: false,
};
```

## Message Types

### Client to Server Messages

```rust
use moosicbox_ws::ClientMessage;

// Playback control messages
let play_message = ClientMessage::PlaybackCommand {
    command: "play".to_string(),
    track_id: Some(12345),
    start_position: Some(30.0),
};

let pause_message = ClientMessage::PlaybackCommand {
    command: "pause".to_string(),
    track_id: None,
    start_position: None,
};

let seek_message = ClientMessage::SeekCommand {
    position_seconds: 120.5,
};

let volume_message = ClientMessage::VolumeCommand {
    volume: 0.75,
};

// Queue management messages
let add_to_queue = ClientMessage::QueueCommand {
    action: QueueAction::Add {
        track_id: 67890,
        position: Some(3),
    },
};

let clear_queue = ClientMessage::QueueCommand {
    action: QueueAction::Clear,
};

// Room messages
let join_room = ClientMessage::RoomCommand {
    action: RoomAction::Join {
        room_id: "listening_party_123".to_string(),
    },
};

let room_chat = ClientMessage::RoomChat {
    room_id: "listening_party_123".to_string(),
    message: "Great track!".to_string(),
};

// Library actions
let request_library = ClientMessage::LibraryRequest {
    request_type: "get_artists".to_string(),
    filters: Some(serde_json::json!({
        "genre": "jazz"
    })),
};
```

### Server to Client Messages

```rust
use moosicbox_ws::ServerMessage;

// Status updates
let status_update = ServerMessage::PlaybackStatus {
    is_playing: true,
    current_track: Some(track_info),
    position_seconds: 45.2,
    duration_seconds: 180.0,
    volume: 0.8,
    queue_length: 5,
};

let track_changed = ServerMessage::TrackChanged {
    track: new_track_info,
    previous_track: Some(old_track_info),
};

// Queue updates
let queue_update = ServerMessage::QueueUpdated {
    queue: updated_queue,
    current_index: 2,
};

// Library updates
let library_update = ServerMessage::LibraryUpdated {
    update_type: "tracks_added".to_string(),
    affected_items: vec![track_id_1, track_id_2],
    total_changes: 2,
};

// Room events
let room_event = ServerMessage::RoomEvent {
    room_id: "listening_party_123".to_string(),
    event_type: "user_joined".to_string(),
    data: serde_json::json!({
        "user_id": 456,
        "username": "jazz_fan"
    }),
};

// Notifications
let notification = ServerMessage::Notification {
    level: "info".to_string(),
    title: "Scan Complete".to_string(),
    message: "Library scan completed. 42 new tracks added.".to_string(),
    timestamp: chrono::Utc::now(),
};

// Error messages
let error_message = ServerMessage::Error {
    error_type: "permission_denied".to_string(),
    message: "You don't have permission to modify this playlist".to_string(),
    request_id: Some("req_12345".to_string()),
};
```

## Feature Flags

- `ws` - Core WebSocket functionality
- `ws-rooms` - Room-based communication support
- `ws-compression` - Message compression support
- `ws-redis` - Redis-backed scaling and persistence
- `ws-metrics` - Connection and performance metrics
- `ws-auth` - Authentication integration
- `ws-rate-limiting` - Rate limiting and abuse prevention

## Integration with MoosicBox

### Server Integration

```toml
[dependencies]
moosicbox-ws = { path = "../ws", features = ["ws-rooms", "ws-compression"] }
```

```rust
use moosicbox_ws::WebSocketServer;
use moosicbox_server::Server;

async fn setup_websocket_integration() -> Result<(), Box<dyn std::error::Error>> {
    let ws_server = WebSocketServer::new(ws_config, auth_manager).await?;
    let mut server = Server::new().await?;

    // Add WebSocket upgrade endpoint
    server.add_websocket_route("/ws", ws_server.clone()).await?;

    // Add WebSocket management API
    server.add_websocket_api_routes(ws_server.clone()).await?;

    Ok(())
}
```

### Player Integration

```rust
use moosicbox_ws::WebSocketServer;
use moosicbox_player::{Player, PlayerEventHandler};

struct WebSocketPlayerHandler {
    ws_server: WebSocketServer,
}

impl PlayerEventHandler for WebSocketPlayerHandler {
    async fn on_track_changed(&self, track: TrackInfo) {
        self.ws_server.broadcast_to_all(ServerMessage::TrackChanged {
            track,
            previous_track: None,
        }).await.ok();
    }

    async fn on_playback_state_changed(&self, is_playing: bool, position: f64) {
        self.ws_server.broadcast_to_all(ServerMessage::PlaybackStatus {
            is_playing,
            current_track: None,
            position_seconds: position,
            duration_seconds: 0.0,
            volume: 1.0,
            queue_length: 0,
        }).await.ok();
    }

    async fn on_queue_updated(&self, queue: Queue) {
        self.ws_server.broadcast_to_all(ServerMessage::QueueUpdated {
            queue,
            current_index: 0,
        }).await.ok();
    }
}
```

## Error Handling

```rust
use moosicbox_ws::error::WebSocketError;

match ws_server.send_to_client(client_id, message).await {
    Ok(()) => println!("Message sent successfully"),
    Err(WebSocketError::ClientNotFound(id)) => {
        eprintln!("Client {} not found", id);
    },
    Err(WebSocketError::ConnectionClosed(id)) => {
        eprintln!("Connection {} is closed", id);
    },
    Err(WebSocketError::MessageTooLarge { size, max_size }) => {
        eprintln!("Message too large: {} bytes (max: {})", size, max_size);
    },
    Err(WebSocketError::RateLimitExceeded { client_id, limit }) => {
        eprintln!("Rate limit exceeded for client {}: {}", client_id, limit);
    },
    Err(WebSocketError::AuthenticationFailed) => {
        eprintln!("WebSocket authentication failed");
    },
    Err(WebSocketError::SerializationError(e)) => {
        eprintln!("Failed to serialize message: {}", e);
    },
    Err(e) => {
        eprintln!("WebSocket error: {}", e);
    }
}
```

## Performance and Monitoring

### Connection Metrics

```rust
use moosicbox_ws::{WebSocketServer, ConnectionMetrics};

async fn monitor_connections(ws_server: &WebSocketServer) -> Result<(), Box<dyn std::error::Error>> {
    let metrics = ws_server.get_metrics().await?;

    println!("WebSocket Metrics:");
    println!("  Active connections: {}", metrics.active_connections);
    println!("  Total connections: {}", metrics.total_connections);
    println!("  Messages sent: {}", metrics.messages_sent);
    println!("  Messages received: {}", metrics.messages_received);
    println!("  Bytes sent: {}", metrics.bytes_sent);
    println!("  Bytes received: {}", metrics.bytes_received);
    println!("  Average latency: {:.2}ms", metrics.average_latency_ms);

    // Per-client metrics
    let client_metrics = ws_server.get_client_metrics().await?;
    for metric in client_metrics {
        println!("Client {}: {} msgs sent, {} msgs received, {:.2}ms latency",
                 metric.client_id, metric.messages_sent, metric.messages_received, metric.latency_ms);
    }

    Ok(())
}
```

### Health Checks

```rust
use moosicbox_ws::{WebSocketServer, HealthStatus};

async fn health_check(ws_server: &WebSocketServer) -> HealthStatus {
    let status = ws_server.get_health_status().await;

    match status {
        HealthStatus::Healthy => {
            println!("WebSocket server is healthy");
        },
        HealthStatus::Degraded { reason } => {
            println!("WebSocket server is degraded: {}", reason);
        },
        HealthStatus::Unhealthy { reason } => {
            println!("WebSocket server is unhealthy: {}", reason);
        },
    }

    status
}
```

## See Also

- [MoosicBox Server](../server/README.md) - HTTP server with WebSocket integration
- [MoosicBox Auth](../auth/README.md) - Authentication for WebSocket connections
- [MoosicBox Player](../player/README.md) - Audio player with WebSocket events
