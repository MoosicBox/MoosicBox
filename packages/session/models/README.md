# `MoosicBox` Session Models

Data models for session management, playback control, and connection handling.

## Overview

The `MoosicBox` Session Models package provides:

- **Session Management**: Playback session data structures
- **Connection Models**: Client connection and registration
- **Playback Control**: Session update and playlist management
- **Player Registration**: Audio player configuration and setup
- **API Integration**: REST-compatible session models

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_session_models = { path = "../session/models" }
```

## Dependencies

- **serde**: Serialization and deserialization
- **`moosicbox_audio_zone_models`**: Audio zone models
- **`moosicbox_music_models`**: Music and track models
- **`moosicbox_json_utils`**: JSON parsing utilities
- **`switchy_database`**: Database value types and traits
- **`strum/strum_macros`**: Enum string conversions
- **log**: Logging facade
- **utoipa** (optional): `OpenAPI` schema generation

## Cargo Features

| Feature            | Default | Description                          |
| ------------------ | ------- | ------------------------------------ |
| `api`              | Yes     | API serialization support            |
| `openapi`          | Yes     | OpenAPI schema generation via utoipa |
| `aac`              | No      | AAC codec support in music models    |
| `flac`             | No      | FLAC codec support in music models   |
| `mp3`              | No      | MP3 codec support in music models    |
| `opus`             | No      | Opus codec support in music models   |
| `fail-on-warnings` | No      | Treat compiler warnings as errors    |

## Core Types

### Session Types

- `Session` - A playback session with name, state, volume, position, and
  playlist
- `SessionPlaylist` - A session's playlist containing tracks
- `ApiSession` / `ApiSessionPlaylist` - API representations of session types

### Session Request Types

- `CreateSession` - Request to create a new session with name and playlist
- `UpdateSession` - Request to update session state (play, stop, seek, volume,
  etc.)
- `DeleteSession` - Request to delete a session
- `SetSessionAudioZone` - Request to associate a session with an audio zone

### Playback Targets

- `PlaybackTarget` - Enum for playback destination (audio zone or connection
  output)
- `ApiPlaybackTarget` - API representation of playback target

### Connection Types

- `Connection` - A client connection with ID, name, timestamps, and players
- `RegisterConnection` - Request to register a new connection
- `ApiConnection` - API representation of a connection
- `RegisterPlayer` - Player registration data with audio output ID and name

## Usage

### Creating a Session

```rust
use moosicbox_session_models::{CreateSession, CreateSessionPlaylist};

let request = CreateSession {
    name: "My Playlist".to_string(),
    audio_zone_id: Some(1),
    playlist: CreateSessionPlaylist {
        tracks: vec![1, 2, 3],
    },
};
```

### Updating Session Playback

```rust
use moosicbox_session_models::{UpdateSession, PlaybackTarget};

let update = UpdateSession {
    session_id: 1,
    profile: "default".to_string(),
    playback_target: PlaybackTarget::AudioZone { audio_zone_id: 1 },
    play: Some(true),
    volume: Some(0.8),
    seek: Some(30.0),
    ..Default::default()
};

// Check if any playback fields were updated
if update.playback_updated() {
    // Handle playback state change
}
```

### Working with Playback Targets

```rust
use moosicbox_session_models::PlaybackTarget;

// Audio zone target
let zone_target = PlaybackTarget::AudioZone { audio_zone_id: 1 };

// Connection output target
let output_target = PlaybackTarget::ConnectionOutput {
    connection_id: "conn-123".to_string(),
    output_id: "output-1".to_string(),
};

// Parse from type string
let target = PlaybackTarget::default_from_str("AUDIO_ZONE");
```

### Registering a Connection

```rust
use moosicbox_session_models::{RegisterConnection, RegisterPlayer};

let connection = RegisterConnection {
    connection_id: "client-abc".to_string(),
    name: "Living Room".to_string(),
    players: vec![
        RegisterPlayer {
            audio_output_id: "speaker-1".to_string(),
            name: "Main Speaker".to_string(),
        },
    ],
};
```

## License

This project is licensed under the Mozilla Public License 2.0 - see the LICENSE
file in the repository root.
