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

- **Serde**: Serialization and deserialization
- **MoosicBox Core Models**: Core music and audio types
