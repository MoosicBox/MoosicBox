# MoosicBox Tidal Integration

Integration with Tidal's music streaming service for MoosicBox.

## Overview

The MoosicBox Tidal package provides integration with Tidal's streaming API through the `MusicApi` trait, enabling:

- **OAuth2 Device Flow Authentication**: Secure authentication using Tidal's device authorization flow
- **Music Catalog Access**: Browse and search Tidal's music library
- **Favorites Management**: Access and manage user favorites (artists, albums, tracks)
- **FLAC Streaming**: High-quality lossless audio streaming support
- **Search**: Search across artists, albums, and tracks

## Features

### Authentication

- OAuth2 device authorization flow
- Token refresh handling
- Optional database credential persistence

### Content Access

- Access favorite artists, albums, and tracks
- Browse artist albums (with filtering by album type)
- Retrieve album tracks
- Search across the Tidal catalog

### Audio Streaming

- Track streaming URL retrieval
- Support for multiple quality levels (High, Lossless, HiResLossless)
- Track playback metadata

## Architecture

The package is organized into:

- `lib.rs` - Core API implementation and MusicApi trait integration (packages/tidal/src/lib.rs)
- `models.rs` - Data models for Tidal entities (packages/tidal/src/models.rs)
- `api.rs` - HTTP API endpoints for server integration (packages/tidal/src/api.rs)
- `db/` - Database persistence for configuration (packages/tidal/src/db/)

## Usage

### Authentication

The package uses Tidal's OAuth2 device authorization flow:

```rust
use moosicbox_tidal::{device_authorization, device_authorization_token};

// Step 1: Start device authorization
let auth_result = device_authorization("your_client_id".to_string(), true).await?;
// This opens the authorization URL in the browser

// Step 2: Complete authorization with device code
#[cfg(feature = "db")]
let token_result = device_authorization_token(
    &db,
    "your_client_id".to_string(),
    "your_client_secret".to_string(),
    device_code,
    Some(true), // persist to database
).await?;
```

### Using the MusicApi Trait

```rust
use moosicbox_tidal::TidalMusicApi;
use moosicbox_music_api::MusicApi;

// Create the Tidal API instance
#[cfg(feature = "db")]
let tidal_api = TidalMusicApi::builder()
    .with_db(db)
    .build()
    .await?;

// Get favorite artists
let artists = tidal_api.artists(Some(0), Some(20), None, None).await?;

// Get favorite albums
let albums_request = AlbumsRequest {
    page: Some(PagingRequest { offset: 0, limit: 20 }),
    sort: None,
};
let albums = tidal_api.albums(&albums_request).await?;

// Search
let results = tidal_api.search("Pink Floyd", Some(0), Some(10)).await?;
```

### Direct API Functions

```rust
use moosicbox_tidal::{favorite_albums, album_tracks, track_file_url, TidalAudioQuality};

// Get favorite albums
#[cfg(feature = "db")]
let albums = favorite_albums(
    &db,
    Some(0),    // offset
    Some(20),   // limit
    None,       // order
    None,       // order_direction
    None,       // country_code
    None,       // locale
    None,       // device_type
    None,       // access_token
    None,       // user_id
).await?;

// Get tracks for an album
#[cfg(feature = "db")]
let tracks = album_tracks(
    &db,
    &album_id,
    Some(0),
    Some(100),
    None,
    None,
    None,
    None,
).await?;

// Get streaming URL
#[cfg(feature = "db")]
let urls = track_file_url(
    &db,
    TidalAudioQuality::Lossless,
    &track_id,
    None,
).await?;
```

### Managing Favorites

```rust
use moosicbox_tidal::{add_favorite_artist, remove_favorite_track};

// Add an artist to favorites
#[cfg(feature = "db")]
add_favorite_artist(
    &db,
    &artist_id,
    None, // country_code
    None, // locale
    None, // device_type
    None, // access_token
    None, // user_id
).await?;

// Remove a track from favorites
#[cfg(feature = "db")]
remove_favorite_track(
    &db,
    &track_id,
    None,
    None,
    None,
    None,
    None,
).await?;
```

## Configuration

### Feature Flags

- `default` - Enables `api`, `db`, `openapi`, and `scan` features
- `api` - Enable HTTP API endpoints (requires actix-web)
- `db` - Enable database persistence for credentials and scanning
- `openapi` - Enable OpenAPI/Swagger documentation (requires utoipa)
- `scan` - Enable library scanning functionality
- `fail-on-warnings` - Treat warnings as errors during compilation

### Audio Quality

The package supports three quality levels through `TidalAudioQuality`:

```rust
use moosicbox_tidal::TidalAudioQuality;

let quality = TidalAudioQuality::High;         // 320 kbps AAC
let quality = TidalAudioQuality::Lossless;     // 1411 kbps FLAC (CD quality)
let quality = TidalAudioQuality::HiResLossless; // Hi-Res FLAC
```

Note: Available quality levels depend on the user's Tidal subscription tier.

## Integration with MoosicBox

### Server Integration

The package provides actix-web endpoints through the `api` feature:

```rust
#[cfg(feature = "api")]
use moosicbox_tidal::api::bind_services;

let scope = actix_web::web::scope("/tidal");
let scope = bind_services(scope);
```

### Player Integration

The `TidalMusicApi` implements the `MusicApi` trait, allowing it to be used as a music source:

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_tidal::TidalMusicApi;

#[cfg(feature = "db")]
let tidal = TidalMusicApi::builder()
    .with_db(db)
    .build()
    .await?;

// Use with any system that accepts MusicApi
let track_source = tidal.track_source(track, quality).await?;
```

## Data Models

The package defines several key models (packages/tidal/src/models.rs):

- `TidalArtist` - Artist information with picture URL
- `TidalAlbum` - Album details including metadata tags
- `TidalTrack` - Track information with quality indicators
- `TidalSearchResults` - Search results across multiple entity types

All models support conversion to/from MoosicBox's common `Artist`, `Album`, and `Track` types.

## API Endpoints

When the `api` feature is enabled, the following endpoints are available:

- `POST /auth/device-authorization` - Start OAuth flow
- `POST /auth/device-authorization/token` - Complete OAuth flow
- `GET /track/url` - Get track streaming URL
- `GET /track/playback-info` - Get track playback metadata
- `GET /favorites/artists` - List favorite artists
- `POST /favorites/artists` - Add artist to favorites
- `DELETE /favorites/artists` - Remove artist from favorites
- `GET /favorites/albums` - List favorite albums
- `POST /favorites/albums` - Add album to favorites
- `DELETE /favorites/albums` - Remove album from favorites
- `GET /favorites/tracks` - List favorite tracks
- `POST /favorites/tracks` - Add track to favorites
- `DELETE /favorites/tracks` - Remove track from favorites
- `GET /artists/albums` - Get albums by artist
- `GET /albums/tracks` - Get tracks by album
- `GET /albums` - Get album by ID
- `GET /artists` - Get artist by ID
- `GET /tracks` - Get track by ID
- `GET /search` - Search Tidal catalog

## Dependencies

Core dependencies from Cargo.toml (packages/tidal/Cargo.toml):

- `moosicbox_music_api` - Music API trait definitions
- `moosicbox_music_models` - Common music data models
- `switchy` - HTTP client and database abstractions
- `serde` / `serde_json` - Serialization
- `actix-web` - Web framework (optional, with `api` feature)
- `utoipa` - OpenAPI documentation (optional, with `openapi` feature)

## Error Handling

The package defines an `Error` enum with variants for:

```rust
use moosicbox_tidal::Error;

match result {
    Err(Error::Unauthorized) => {
        // Re-authenticate
    }
    Err(Error::HttpRequestFailed(status, message)) => {
        // Handle HTTP errors
    }
    Err(Error::NoAccessTokenAvailable) => {
        // Need to authenticate
    }
    _ => {}
}
```

## Notes

- The package requires a valid Tidal client ID and secret for authentication
- Database persistence requires the `db` feature to be enabled
- Streaming quality depends on the user's Tidal subscription level
- All API functions that interact with Tidal require authentication (except device_authorization)
- The package handles automatic token refresh when credentials are persisted to the database

## See Also

- [MoosicBox Player](../player/README.md) - Audio playback engine
- [MoosicBox Server](../server/README.md) - Main server application
- [MoosicBox Qobuz](../qobuz/README.md) - Alternative streaming service integration
