# MoosicBox Menu Models

Data models for album versions and audio quality metadata.

## Overview

The MoosicBox Menu Models package provides:

- **Album Version Models**: Data structures for album versions with audio quality metadata
- **Audio Quality Information**: Format, bit depth, sample rate, and channel information
- **Track Management**: Album version track lists with source tracking
- **API Models**: REST-compatible album version data structures

## Installation

Add this to your Cargo.toml:

```toml
[dependencies]
moosicbox_menu_models = { path = "../menu/models" }
```

## Dependencies

- **serde**: Serialization and deserialization
- **moosicbox_music_models**: Music data models and types
- **moosicbox_assert**: Assertion utilities
- **log**: Logging framework
- **utoipa**: OpenAPI schema generation (optional, enabled with `openapi` feature)

## Features

- `api` (default): Enables API model types
- `openapi` (default): Adds OpenAPI schema support via utoipa
