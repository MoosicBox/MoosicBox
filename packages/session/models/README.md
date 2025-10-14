# MoosicBox Session Models

Data models for session management, playback control, and connection handling.

## Overview

The MoosicBox Session Models package provides:

- **Session Management**: Playback session data structures
- **Connection Models**: Client connection and registration
- **Playback Control**: Session update and playlist management
- **Player Registration**: Audio player configuration and setup
- **API Integration**: REST-compatible session models

## Installation

Add this to your Cargo.toml:

[dependencies]
moosicbox_session_models = { path = "../session/models" }

## Dependencies

- **serde**: Serialization and deserialization
- **moosicbox_audio_zone_models**: Audio zone models
- **moosicbox_music_models**: Music and track models
- **moosicbox_json_utils**: JSON parsing utilities
- **switchy_database**: Database value types and traits
- **strum/strum_macros**: Enum string conversions
- **log**: Logging facade
- **utoipa** (optional): OpenAPI schema generation
