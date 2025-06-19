# MoosicBox Library Models

Data models for MoosicBox music library management and storage.

## Overview

The MoosicBox Library Models package provides:

- **Library Data Models**: Core data structures for music library items
- **API Integration**: Models with API source tracking and conversion
- **Database Support**: Database-compatible model structures
- **Type Conversions**: Conversion between library and API models
- **Multi-Source Support**: Track music from multiple streaming services

## Models

### LibraryArtist
- **Basic Info**: ID, title, cover artwork
- **API Sources**: Integration with multiple music services
- **Conversion**: Converts to/from generic Artist model

### LibraryAlbum
- **Album Data**: Title, artist, type, release dates
- **Artwork**: Cover art and blur effect support
- **Versions**: Multiple quality versions (Hi-Res, CD, etc.)
- **Sources**: Local and streaming service source tracking
- **Directory**: Local filesystem path information

### LibraryTrack
- **Track Info**: Number, title, duration, format details
- **Audio Metadata**: Bitrate, sample rate, channels, bit depth
- **File Info**: Local file path and byte size
- **Album Association**: Connected album and artist information
- **Multi-Source**: Tracks from local files and streaming services

### LibraryAlbumType
- **Album Categories**: LP, Live, Compilations, EPs & Singles, Other
- **Conversion**: Maps to/from generic AlbumType
- **Serialization**: JSON and database compatible

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_library_models = { path = "../library/models" }

# Enable specific features
moosicbox_library_models = {
    path = "../library/models",
    features = ["api", "db"]
}
```

## Usage

### Basic Model Usage

```rust
use moosicbox_library_models::{LibraryArtist, LibraryAlbum, LibraryTrack};
use moosicbox_music_models::ApiSources;

// Create a library artist
let artist = LibraryArtist {
    id: 1,
    title: "The Beatles".to_string(),
    cover: Some("/covers/beatles.jpg".to_string()),
    api_sources: ApiSources::default(),
};

// Create a library album
let album = LibraryAlbum {
    id: 1,
    title: "Abbey Road".to_string(),
    artist: "The Beatles".to_string(),
    artist_id: 1,
    album_type: LibraryAlbumType::Lp,
    date_released: Some("1969-09-26".to_string()),
    ..Default::default()
};
```

## Dependencies

- **MoosicBox Music Models**: Core music data types
- **MoosicBox Date Utils**: Date parsing and formatting
- **Serde**: Serialization and deserialization
- **Chrono**: Date and time handling
