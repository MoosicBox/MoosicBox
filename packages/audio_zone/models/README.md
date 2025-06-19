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

[dependencies]
moosicbox_audio_zone_models = { path = "../audio_zone/models" }

## Dependencies

- **Serde**: Serialization and deserialization
- **MoosicBox Core Models**: Core audio and session types
