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
- **Artists**: Browse and manage favorite artists
- **Albums**: Album browsing with filtering and sorting
- **Tracks**: Track management and playback
- **Search**: Full-text search across library content

### Library Operations
- **Favorites**: Add/remove favorite artists, albums, and tracks
- **Versions**: Multiple quality versions per album
- **Cover Art**: Album and artist cover art management
- **File Access**: Direct file system access for local tracks

### Scanning & Indexing
- **Library Scanning**: Automatic music library scanning
- **Metadata Extraction**: Audio file metadata processing
- **Search Indexing**: Full-text search index maintenance
- **Profile Integration**: Multi-profile scanning support

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

// Create library API instance
let db = LibraryDatabase::new("profile_name").await?;
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

## Dependencies

- **MoosicBox Music API**: Core music API traits
- **MoosicBox Library**: Library database operations
- **MoosicBox Scan**: Library scanning functionality
- **MoosicBox Files**: File system operations
- **Switchy Database**: Database abstraction
- **Async Trait**: Async trait support
