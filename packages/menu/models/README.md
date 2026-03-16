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
moosicbox_menu_models = "0.1.4"
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

## Usage

Main public types:

- `AlbumVersion`: Core album version model with track list and audio quality metadata
- `api::ApiAlbumVersion` (with `api` feature): Serializable API model for request/response payloads

The crate also provides conversions between these types:

- `From<AlbumVersion> for api::ApiAlbumVersion`
- `From<api::ApiAlbumVersion> for AlbumVersion`
- `From<&api::ApiAlbumVersion> for moosicbox_music_models::AlbumVersionQuality`
- `From<api::ApiAlbumVersion> for moosicbox_music_models::AlbumVersionQuality`

```rust
use moosicbox_menu_models::AlbumVersion;
#[cfg(feature = "api")]
use moosicbox_menu_models::api::ApiAlbumVersion;

fn convert(version: AlbumVersion) {
    #[cfg(feature = "api")]
    {
        let api_version: ApiAlbumVersion = version.clone().into();
        let _domain_version: AlbumVersion = api_version.into();
    }
}
```
