# MoosicBox Library Music API

A `MusicApi` implementation for local library access, providing access to artists, albums, tracks, and search functionality against a local library database.

## Features

- **Local Library Access**: Query artists, albums, and tracks from a local database
- **Search**: Full-text search across the library
- **Favorites Management**: Add/remove artists, albums, and tracks as favorites
- **Profile Support**: Manage multiple library databases via `LibraryMusicApiProfiles`
- **Library Scanning**: Scan local files to populate the library
- **Audio Encoding**: Support for multiple audio formats (AAC, FLAC, MP3, Opus)
- **Actix-web Integration**: Automatic profile extraction from HTTP requests (with `api` feature)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_library_music_api = { workspace = true }
```

## Usage

### Basic Usage

```rust
use moosicbox_library_music_api::LibraryMusicApi;
use switchy_database::profiles::LibraryDatabase;

// Create a LibraryMusicApi from a database
let api = LibraryMusicApi::new(db);

// Or convert from a LibraryDatabase
let api: LibraryMusicApi = db.into();
```

### Using the MusicApi Trait

The `LibraryMusicApi` implements the `MusicApi` trait from `moosicbox_music_api`:

```rust
use moosicbox_music_api::MusicApi;

// Get artists with pagination
let artists = api.artists(Some(0), Some(20), None, None).await?;

// Get a specific album
let album = api.album(&album_id).await?;

// Search the library
let results = api.search("query", Some(0), Some(10)).await?;

// Manage favorites
api.add_album(&album_id).await?;
api.remove_track(&track_id).await?;

// Scan the library
api.enable_scan().await?;
api.scan().await?;
```

### Profile Management

Manage multiple library databases simultaneously:

```rust
use moosicbox_library_music_api::profiles::{PROFILES, LibraryMusicApiProfiles};

// Add a profile
PROFILES.add("my_profile".to_string(), db);

// Retrieve a profile's API
let api = PROFILES.get("my_profile");

// List all profile names
let names = PROFILES.names();

// Remove a profile
PROFILES.remove("my_profile");
```

## Cargo Features

- **`default`**: Enables `all-encoders` and `api`
- **`api`**: Actix-web integration for profile extraction from requests
- **`all-encoders`**: All audio encoders (AAC, FLAC, MP3, Opus)
- **`encoder-aac`**: AAC encoding support
- **`encoder-flac`**: FLAC encoding support
- **`encoder-mp3`**: MP3 encoding support
- **`encoder-opus`**: Opus encoding support
- **`all-formats`**: All audio format support
- **`simulator`**: Testing simulator support

## License

See the [LICENSE](../../../LICENSE) file for details.
