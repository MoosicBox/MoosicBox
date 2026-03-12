# MoosicBox Audio Zone Models

Data models for multi-zone audio configuration and management.

## Overview

The MoosicBox Audio Zone Models package provides:

- **Audio Zone Configuration**: Zone setup and management models
- **Player Assignment**: Player-to-zone mapping structures
- **Session Integration**: Zone-aware session management
- **API Models**: REST-compatible zone data structures

## Features

- **api** (default): Enables database API integration types via `switchy_database/api`
- **openapi** (default): Enables OpenAPI schema generation for API types
- **fail-on-warnings**: Enables strict warning enforcement across crate dependencies

## Installation

Add this to your Cargo.toml:

```toml
[dependencies]
moosicbox_audio_zone_models = "0.1.4"
```

Enable optional features as needed:

```toml
[dependencies]
moosicbox_audio_zone_models = { version = "0.1.4", default-features = false, features = ["api", "openapi"] }
```

## Core Models

- **AudioZone/ApiAudioZone**: Zone configuration with ID, name, and players
- **AudioZoneWithSession/ApiAudioZoneWithSession**: Zone configuration with session integration
- **Player/ApiPlayer**: Player information including audio output ID, name, and playback state
- **CreateAudioZone**: Model for creating new audio zones
- **UpdateAudioZone**: Model for updating existing zones

## Usage

```rust
use moosicbox_audio_zone_models::{
    ApiAudioZone, ApiAudioZoneWithSession, ApiPlayer, AudioZone, AudioZoneWithSession,
    CreateAudioZone, Player, UpdateAudioZone,
};

let create = CreateAudioZone {
    name: "Living Room".to_string(),
};

let player = Player {
    id: 1,
    audio_output_id: "sink-1".to_string(),
    name: "Left Speaker".to_string(),
    playing: true,
    created: "2026-01-01T00:00:00Z".to_string(),
    updated: "2026-01-01T00:00:00Z".to_string(),
};

let zone = AudioZone {
    id: 10,
    name: create.name,
    players: vec![player],
};

let api_zone: ApiAudioZone = zone.clone().into();
let _round_trip_zone: AudioZone = api_zone.into();

let api_player = ApiPlayer {
    player_id: 2,
    audio_output_id: "sink-2".to_string(),
    name: "Right Speaker".to_string(),
    playing: false,
};

let _player_from_api: Player = api_player.into();

let api_zone_with_session = ApiAudioZoneWithSession {
    id: 11,
    session_id: 20,
    name: "Kitchen".to_string(),
    players: vec![],
};

let _zone_with_session: AudioZoneWithSession = api_zone_with_session.into();

let _update = UpdateAudioZone {
    id: 10,
    name: Some("Main Room".to_string()),
    players: Some(vec![1, 2]),
};
```

## Dependencies

- **serde**: Serialization and deserialization
- **moosicbox_assert**: Assertion helpers used by this crate
- **moosicbox_json_utils**: JSON utilities and database value conversion
- **switchy_database**: Database integration and value types
- **log**: Logging facade
- **utoipa** (optional): OpenAPI schema generation support
