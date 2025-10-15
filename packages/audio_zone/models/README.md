# MoosicBox Audio Zone Models

Data models for multi-zone audio configuration and management.

## Overview

The MoosicBox Audio Zone Models package provides:

- **Audio Zone Configuration**: Zone setup and management models
- **Player Assignment**: Player-to-zone mapping structures
- **Session Integration**: Zone-aware session management
- **API Models**: REST-compatible zone data structures

## Installation

Add this to your Cargo.toml:

```toml
[dependencies]
moosicbox_audio_zone_models = { path = "../audio_zone/models" }
```

## Core Models

- **AudioZone/ApiAudioZone**: Zone configuration with ID, name, and players
- **AudioZoneWithSession/ApiAudioZoneWithSession**: Zone configuration with session integration and ID
- **Player/ApiPlayer**: Player information including ID, audio output ID, name, playback state, and timestamps
- **CreateAudioZone**: Model for creating new audio zones with a name
- **UpdateAudioZone**: Model for updating existing zones with optional name and player list changes

## Dependencies

- **serde**: Serialization and deserialization
- **moosicbox_json_utils**: JSON utilities and database value conversion
- **switchy_database**: Database integration and value types
- **moosicbox_assert**: Assertion utilities
- **log**: Logging functionality
- **utoipa** (optional): OpenAPI schema generation support
