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

### Core Models

#### LibraryArtist

- **Basic Info**: ID, title, cover artwork
- **API Sources**: Integration with multiple music services via `ApiSources`
- **Conversion**: Converts to generic `Artist` model via `From` trait

#### LibraryAlbum

- **Album Data**: Title, artist, type, release dates (released and added)
- **Artwork**: Cover art path and blur effect flag
- **Versions**: Multiple quality versions via `AlbumVersionQuality` (Hi-Res, CD, etc.)
- **Sources**: Local and streaming service source tracking via `AlbumSource`
- **Directory**: Local filesystem path information
- **Conversion**: Converts to/from generic `Album` model via `TryFrom` trait

#### LibraryTrack

- **Track Info**: Number, title, duration, format details
- **Audio Metadata**: Bitrate, sample rate, channels, bit depth
- **File Info**: Local file path and byte size
- **Album Association**: Connected album and artist information
- **Multi-Source**: Tracks from local files and streaming services via `TrackApiSource`
- **Conversion**: Converts to generic `Track` model via `From` trait
- **Helper Methods**: `directory()` to extract parent directory from file path

#### LibraryAlbumType

- **Album Categories**: `Lp`, `Live`, `Compilations`, `EpsAndSingles`, `Other`
- **Conversion**: Maps to/from generic `AlbumType` via `From` trait
- **Serialization**: JSON (via serde) and database compatible
- **String Parsing**: Implements `FromStr` for parsing from strings (SCREAMING_SNAKE_CASE format)

### API Models

#### ApiLibraryArtist

- API-compatible artist model with ID fields for external sources (Tidal, Qobuz, YT)
- `contains_cover` flag instead of cover path

#### ApiLibraryAlbum (available with `api` feature)

- API-compatible album model with `ApiAlbumVersionQuality` for versions
- `contains_cover` flag instead of artwork path
- Converts to/from `LibraryAlbum` and `ApiAlbum`

#### ApiLibraryTrack (available with `api` feature)

- API-compatible track model without file path information
- Includes all metadata and source tracking
- Converts to/from `LibraryTrack` and `Track`

## Features

- **api**: Enables API model conversions and integrations
- **openapi**: Provides OpenAPI schema support via `utoipa`
- **db**: Enables database support with `switchy_database` integration
- **all-formats**: Enables all audio format support (aac, flac, mp3, opus)
- **all-os-formats**: Enables OS-compatible formats only (aac, flac, opus)
- Individual format support: **aac**, **flac**, **mp3**, **opus**

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
use moosicbox_library_models::{LibraryArtist, LibraryAlbum, LibraryAlbumType};
use moosicbox_music_models::{AlbumSource, ApiSources};

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
    date_added: None,
    artwork: None,
    directory: None,
    source: AlbumSource::Local,
    blur: false,
    versions: vec![],
    album_sources: ApiSources::default(),
    artist_sources: ApiSources::default(),
};
```

### Model Conversions

```rust
use moosicbox_library_models::{LibraryArtist, LibraryAlbum};
use moosicbox_music_models::{Artist, Album};

// Convert library models to generic models
let library_artist = LibraryArtist { /* ... */ };
let artist: Artist = library_artist.into();

let library_album = LibraryAlbum { /* ... */ };
let album: Result<Album, _> = library_album.try_into();
```

### Database Support (with `db` feature)

The `db` feature enables database integration via `switchy_database`:

- **Row Conversions**: All models implement `ToValueType` and `AsModelResult` for database row parsing
- **Query Support**: `LibraryAlbum` implements `AsModelQuery` for async database queries with relationship loading
- **Type Mappings**: `LibraryAlbumType` implements database value conversions
- **ID Traits**: All models implement `AsId` for database operations
- **Helper Functions**:
    - `get_album_version_qualities()`: Retrieves album versions from the database with sorting
    - `sort_album_versions()`: Sorts album versions by sample rate, bit depth, and source

These traits enable seamless conversion between database rows and library models, supporting complex queries with joins and relationship loading.

### Utility Functions

```rust
use moosicbox_library_models::sort_album_versions;
use moosicbox_music_models::AlbumVersionQuality;

let mut versions: Vec<AlbumVersionQuality> = vec![/* ... */];
sort_album_versions(&mut versions);
// Versions are now sorted by: sample rate (desc), bit depth (desc), source
```

## Dependencies

Core dependencies:

- **moosicbox_music_models**: Core music data types and models
- **moosicbox_date_utils**: Date parsing and formatting (with chrono support)
- **moosicbox_json_utils**: JSON utilities and database value conversions
- **moosicbox_assert**: Assertion utilities
- **serde** / **serde_json**: Serialization and deserialization
- **strum** / **strum_macros**: Enum string conversions
- **log**: Logging support

Optional dependencies (feature-gated):

- **switchy_database**: Database abstraction layer (enabled with `db` feature)
- **utoipa**: OpenAPI schema generation (enabled with `openapi` feature)
- **async-trait**: Async trait support (enabled with `db` feature)
