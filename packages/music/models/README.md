# MoosicBox Music Models

Core data models for music metadata, sources, and API integration.

## Overview

The MoosicBox Music Models package provides:

- **Core Music Types**: Artist, Album, Track data structures
- **API Source Management**: Multi-service source tracking and registration
- **Audio Format Support**: Comprehensive audio format definitions
- **Quality Management**: Audio quality and version tracking
- **Serialization**: JSON and database compatibility

## Models

### Artist

- **Basic Info**: ID, title, cover artwork
- **API Integration**: Source tracking across multiple services
- **Serialization**: Full JSON and database support

### Album

- **Metadata**: Title, artist, type, release dates
- **Versions**: Multiple quality versions and formats
- **Sources**: Local and streaming service tracking
- **Artwork**: Cover art and blur effect support

### Track

- **Audio Info**: Duration, format, bitrate, sample rate
- **File Data**: Local file path and size information
- **Relationships**: Album and artist associations
- **Quality**: Bit depth, channels, audio metadata

### ApiSource

- **Registration**: Dynamic source registration system
- **Multi-Service**: Support for multiple streaming services
- **Library Integration**: Built-in local library source
- **Display Names**: User-friendly service names

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_models = { path = "../music/models" }

# Enable specific features
moosicbox_music_models = {
    path = "../music/models",
    features = ["api", "db", "flac", "mp3", "aac", "opus"]
}
```

## Usage

### API Source Management

```rust
use moosicbox_music_models::ApiSource;

// Register a new API source
let tidal_source = ApiSource::register("tidal", "Tidal");
let qobuz_source = ApiSource::register("qobuz", "Qobuz");

// Use library source
let library_source = ApiSource::library();

// Check if source is library
if source.is_library() {
    println!("This is a local library source");
}
```

### Track Creation

```rust
use moosicbox_music_models::{ApiSource, AudioFormat, Track, TrackApiSource};

let track = Track {
    id: 1.into(),
    number: 1,
    title: "Bohemian Rhapsody".to_string(),
    duration: 355.0,
    album: "A Night at the Opera".to_string(),
    album_id: 1.into(),
    artist: "Queen".to_string(),
    artist_id: 1.into(),
    format: Some(AudioFormat::Flac),
    bit_depth: Some(24),
    sample_rate: Some(96000),
    track_source: TrackApiSource::Local,
    api_source: ApiSource::library(),
    ..Default::default()
};
```

### Multi-Source Management

```rust
use moosicbox_music_models::{ApiSources, ApiSource};

let mut sources = ApiSources::default();
sources.add_source(ApiSource::library(), 123.into());
sources.add_source(tidal_source, 456.into());
sources.add_source(qobuz_source, 789.into());

// Get ID for specific source
if let Some(tidal_id) = sources.get(&tidal_source) {
    println!("Tidal ID: {}", tidal_id);
}
```

## Audio Formats

### Supported Formats

- **FLAC**: Lossless compression (with `flac` feature)
- **MP3**: Lossy compression (with `mp3` feature)
- **AAC**: Advanced Audio Coding (with `aac` feature)
- **Opus**: Modern lossy codec (with `opus` feature)
- **Source**: Original format preservation

### Format Detection

```rust
use moosicbox_music_models::from_extension_to_audio_format;

let format = from_extension_to_audio_format("flac");
assert_eq!(format, Some(AudioFormat::Flac));
```

## Quality Management

### Album Versions

```rust
use moosicbox_music_models::{AlbumVersionQuality, AudioFormat, TrackApiSource};

let hires_version = AlbumVersionQuality {
    format: Some(AudioFormat::Flac),
    bit_depth: Some(24),
    sample_rate: Some(192000),
    channels: Some(2),
    source: TrackApiSource::Local,
};

let cd_version = AlbumVersionQuality {
    format: Some(AudioFormat::Flac),
    bit_depth: Some(16),
    sample_rate: Some(44100),
    channels: Some(2),
    source: TrackApiSource::Local,
};
```

## Feature Flags

- **`api`**: Enable API-compatible model structures
- **`db`**: Enable database-compatible model structures
- **`openapi`**: Enable OpenAPI/utoipa schema generation
- **`tantivy`**: Enable Tantivy search index support
- **`flac`**: Enable FLAC audio format support
- **`mp3`**: Enable MP3 audio format support
- **`aac`**: Enable AAC audio format support
- **`opus`**: Enable Opus audio format support
- **`all-formats`**: Enable all audio format support (includes all-os-formats and mp3)
- **`all-os-formats`**: Enable all OS-supported formats (aac, flac, opus)

## Dependencies

Core dependencies:

- **moosicbox_assert**: Assertion utilities
- **moosicbox_date_utils**: Date parsing and formatting (with chrono features)
- **moosicbox_json_utils**: JSON parsing utilities (with serde_json features)
- **moosicbox_parsing_utils**: Parsing utilities
- **serde**: Serialization and deserialization
- **serde_json**: JSON serialization
- **strum**: Enum string conversion and macros
- **thiserror**: Error handling
- **log**: Logging

Optional dependencies:

- **switchy_database**: Database integration (enabled with `db` feature)
- **utoipa**: OpenAPI schema generation (enabled with `openapi` feature)
- **tantivy**: Search indexing (enabled with `tantivy` feature)

## Integration

This package is the foundation for:

- **Music Library Management**: Core data structures
- **Streaming Service Integration**: Multi-source support
- **API Development**: REST endpoint models
- **Database Storage**: Persistent music metadata
