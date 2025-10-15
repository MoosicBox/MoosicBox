# MoosicBox Library Music API

Music API implementation for local music library integration.

## Overview

The MoosicBox Library Music API provides:

- **Local Library API**: MusicApi implementation for local music collections
- **Database Integration**: Direct integration with library database
- **Search Support**: Full-text search across local music library
- **Scan Integration**: Library scanning and indexing capabilities
- **Profile Support**: Multi-profile library management

## Features

### MusicApi Implementation

- **Complete API**: Full implementation of MusicApi trait for local library
- **Artists**: Browse and manage favorite artists with ordering and pagination
- **Albums**: Album browsing with artist filtering, album type filtering, and pagination
- **Tracks**: Track management and playback with favorite support
- **Search**: Full-text search across library content
- **Album Versions**: Support for multiple quality versions per album

### Library Operations

- **Favorites**: Add/remove favorite artists, albums, and tracks
- **Cover Art**: Album and artist cover art management via local file paths
- **File Access**: Direct file system access for local tracks
- **Track Sizing**: Calculate and cache track sizes for different quality levels
- **Audio Encoding**: Support for multiple audio formats (AAC, FLAC, MP3, Opus)

### Scanning & Indexing

- **Library Scanning**: Automatic music library scanning via MoosicBox Scan integration
- **Scan Control**: Enable/disable and check scan status
- **Profile Integration**: Multi-profile library management via LibraryMusicApiProfiles

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_library_music_api = { path = "../library/music_api" }
```

## Usage

### Basic Library API

```rust
use moosicbox_library_music_api::LibraryMusicApi;
use moosicbox_music_api::MusicApi;
use switchy_database::profiles::LibraryDatabase;

// Create library API instance with existing database connection
let library_api = LibraryMusicApi::new(db);

// Use as MusicApi
let artists = library_api.artists(None, Some(50), None, None).await?;
let albums = library_api.albums(&request).await?;
```

### Library-Specific Operations

```rust
// Get library-specific models
let library_artist = library_api.library_artist(&artist_id).await?;
let library_album = library_api.library_album(&album_id).await?;
let library_track = library_api.library_track(&track_id).await?;

// Get album versions
let versions = library_api.library_album_versions(&album_id).await?;
```

## Features

### Cargo Features

- **api**: Actix-web integration for HTTP endpoints
- **encoder-aac**: AAC audio encoding support
- **encoder-flac**: FLAC audio encoding support
- **encoder-mp3**: MP3 audio encoding support
- **encoder-opus**: Opus audio encoding support
- **all-encoders**: Enable all supported encoders
- **format-aac**: AAC format support
- **format-flac**: FLAC format support
- **format-mp3**: MP3 format support
- **format-opus**: Opus format support

Default features: `all-encoders`, `api`

## Dependencies

- **moosicbox_music_api**: Core music API traits and models
- **moosicbox_library**: Library database operations and models
- **moosicbox_scan**: Library scanning functionality
- **moosicbox_files**: File system operations and content handling
- **moosicbox_profiles**: Multi-profile support
- **moosicbox_paging**: Pagination utilities
- **moosicbox_menu_models**: Menu and UI model types
- **moosicbox_music_models**: Core music domain models
- **switchy_database**: Database abstraction layer
- **async-trait**: Async trait support
- **futures**: Async runtime utilities
- **regex**: Regular expression support
