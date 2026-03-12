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

## Features

- `api` (default): Enables API-facing models used across session endpoints.
- `openapi` (default): Enables `utoipa::ToSchema` derives for OpenAPI schema generation.
- `fail-on-warnings`: Treats warnings as errors for this crate and related model crates.
- `aac`, `flac`, `mp3`, `opus`: Propagates codec model support to `moosicbox_music_models`.

## Usage

This crate is primarily type-driven. The main entry points are public request/response models and conversion types used by session APIs.

- **Session lifecycle**: `CreateSession`, `UpdateSession`, `DeleteSession`, `Session`, `ApiSession`
- **Playlist updates**: `CreateSessionPlaylist`, `UpdateSessionPlaylist`, `ApiUpdateSessionPlaylist`, `SessionPlaylist`, `ApiSessionPlaylist`
- **Playback routing**: `PlaybackTarget`, `ApiPlaybackTarget`, `SetSessionAudioZone`
- **Connection registration**: `RegisterConnection`, `Connection`, `ApiConnection`, `RegisterPlayer`

```rust
use moosicbox_session_models::{
    ApiPlaybackTarget, ApiUpdateSession, CreateSession, CreateSessionPlaylist, PlaybackTarget,
    RegisterConnection, RegisterPlayer, UpdateSession,
};

let create = CreateSession {
    name: "Living Room".to_string(),
    audio_zone_id: Some(7),
    playlist: CreateSessionPlaylist { tracks: vec![101, 202] },
};

let update = UpdateSession {
    session_id: 42,
    profile: "default".to_string(),
    playback_target: PlaybackTarget::AudioZone { audio_zone_id: 7 },
    play: Some(true),
    ..UpdateSession::default()
};

let api_update: ApiUpdateSession = update.into();
let _target = ApiPlaybackTarget::from(PlaybackTarget::ConnectionOutput {
    connection_id: "desktop-client".to_string(),
    output_id: "default".to_string(),
});

let _registration = RegisterConnection {
    connection_id: "desktop-client".to_string(),
    name: "Desktop".to_string(),
    players: vec![RegisterPlayer {
        audio_output_id: "default".to_string(),
        name: "Main Output".to_string(),
    }],
};

let _has_playback_changes = create.playlist.tracks.len() > 1 && api_update.play.is_some();
```

## License

Licensed under `MPL-2.0`.
