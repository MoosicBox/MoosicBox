# MoosicBox Session

Session management library for the MoosicBox ecosystem, providing basic user session handling, playback state management, and connection tracking for audio devices and players.

## Features

- **Session Management**: Create, update, and delete user sessions
- **Playback State**: Track session playback state and playlist management
- **Audio Zone Integration**: Associate sessions with audio zones and players
- **Connection Management**: Track device connections and player registrations
- **Database Integration**: Store session data with PostgreSQL and SQLite support
- **Event System**: Optional event notifications for player updates

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_session = "0.1.4"

# Enable API endpoints
moosicbox_session = { version = "0.1.4", features = ["api"] }

# Enable event system
moosicbox_session = { version = "0.1.4", features = ["events"] }
```

## Usage

### Creating and Managing Sessions

```rust
use moosicbox_session::{create_session, get_session, update_session, delete_session};
use moosicbox_session::models::{CreateSession, CreateSessionPlaylist, UpdateSession, PlaybackTarget};
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db: LibraryDatabase = /* your database connection */;

    // Create a new session
    let create_session = CreateSession {
        name: "My Music Session".to_string(),
        audio_zone_id: Some(1),
        playlist: CreateSessionPlaylist {
            tracks: vec![101, 102, 103],  // Track IDs
        },
    };

    let session = create_session(&db, &create_session).await?;
    println!("Created session: {}", session.id);

    // Update session
    let update_session = UpdateSession {
        session_id: session.id,
        profile: "default".to_string(),
        playback_target: PlaybackTarget::AudioZone { audio_zone_id: 1 },
        play: Some(false),
        stop: Some(true),
        name: Some("Paused Session".to_string()),
        active: Some(false),
        playing: Some(false),
        position: Some(30),    // 30 seconds
        seek: None,
        volume: Some(0.8),     // 80% volume
        playlist: None,
        quality: None,
    };

    update_session(&db, &update_session).await?;

    // Get session
    let session = get_session(&db, session.id).await?;
    println!("Session state: {:?}", session);

    Ok(())
}
```

### Managing Connections and Players

```rust
use moosicbox_session::{register_connection, get_connections, delete_connection, delete_player};
use moosicbox_session::models::{RegisterConnection, RegisterPlayer};
use switchy_database::config::ConfigDatabase;

async fn setup_connections(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Register a new connection
    let connection = RegisterConnection {
        connection_id: "device-123".to_string(),
        name: "Living Room Speaker".to_string(),
        players: vec![
            RegisterPlayer {
                name: "Main Speaker".to_string(),
                audio_output_id: "speaker-1".to_string(),
            }
        ],
    };

    let registered = register_connection(db, &connection).await?;
    println!("Registered connection: {}", registered.id);

    // Get all connections
    let connections = get_connections(db).await?;
    for conn in connections {
        println!("Connection: {} with {} players", conn.name, conn.players.len());
    }

    // Delete a player by ID
    if let Some(conn) = connections.first() {
        if let Some(player) = conn.players.first() {
            delete_player(db, player.id).await?;
            println!("Deleted player: {}", player.id);
        }
    }

    // Delete a connection
    delete_connection(db, "device-123").await?;
    println!("Deleted connection");

    Ok(())
}
```

### Session Playlist Management

```rust
use moosicbox_session::{get_session_playlist, get_session_playlist_tracks};

async fn manage_playlist(db: &LibraryDatabase, session_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Get session playlist
    if let Some(playlist) = get_session_playlist(db, session_id).await? {
        println!("Session playlist: {}", playlist.id);
        println!("Playlist has {} tracks", playlist.tracks.len());

        // Display tracks from the playlist
        for track in &playlist.tracks {
            println!("Track: {}", track.title);
        }

        // Or get tracks separately by playlist ID
        let tracks = get_session_playlist_tracks(db, playlist.id).await?;
        println!("Retrieved {} tracks", tracks.len());
    }

    Ok(())
}
```

### Audio Zone Integration

```rust
use moosicbox_session::{get_session_audio_zone, set_session_audio_zone};
use moosicbox_session::models::SetSessionAudioZone;

async fn manage_audio_zone(db: &LibraryDatabase, session_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Get current audio zone
    if let Some(audio_zone) = get_session_audio_zone(db, session_id).await? {
        println!("Session audio zone: {}", audio_zone.id);
    }

    // Set new audio zone
    let set_zone = SetSessionAudioZone {
        session_id,
        audio_zone_id: 456,
    };

    set_session_audio_zone(db, &set_zone).await?;
    println!("Updated session audio zone");

    Ok(())
}
```

### Session State Queries

```rust
use moosicbox_session::{get_sessions, get_session_playing};

async fn query_sessions(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get all sessions
    let sessions = get_sessions(db).await?;
    println!("Found {} sessions", sessions.len());

    for session in sessions {
        // Check if session is playing
        if let Some(playing) = get_session_playing(db, session.id).await? {
            println!("Session {}: {}", session.id, if playing { "Playing" } else { "Paused" });
        }
    }

    Ok(())
}
```

## Core Types

### Session

Represents a user session with playback state, playlist, and audio zone information.

### Connection

Represents a device connection with associated players.

### Player

Represents an audio output device or player within a connection.

### SessionPlaylist

Links sessions to playlists for playback management.

## Error Handling

The library uses `DatabaseFetchError` for database operations.

## Dependencies

Core dependencies:

- `moosicbox_session_models`: Session data models and types
- `moosicbox_audio_zone`: Audio zone integration
- `moosicbox_music_models`: Music API track models
- `moosicbox_library`: Library integration for track retrieval
- `moosicbox_json_utils`: JSON parsing and database utilities
- `switchy_database`: Database abstraction layer
- `switchy_async`: Async runtime utilities

Optional dependencies:

- `actix-web`: Web framework for API endpoints (feature: `api`)
- `moosicbox_paging`: Pagination support (feature: `api`)
- `utoipa`: OpenAPI documentation (feature: `openapi`)
- `tokio`: Event system runtime (feature: `events`)

This library provides the foundation for managing user sessions and playback state in the MoosicBox ecosystem.
