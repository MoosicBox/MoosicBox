# MoosicBox Qobuz Integration

High-resolution music streaming integration with Qobuz's lossless and Hi-Res audio service.

## Overview

The MoosicBox Qobuz package provides integration with Qobuz for streaming high-quality music. It implements the `MusicApi` trait to enable Qobuz as a music source within the MoosicBox ecosystem.

**Key Features:**

- Authentication via username/password
- Access to Qobuz catalog (artists, albums, tracks)
- Favorite management (add/remove artists, albums, tracks)
- Search functionality
- High-resolution audio streaming (FLAC up to 24-bit/192kHz)
- Multiple quality tiers (MP3 320, CD Quality, Hi-Res 24/96, Hi-Res 24/192)

## Implementation Details

This package provides:

- `QobuzMusicApi`: Main API implementation conforming to `MusicApi` trait
- Models for Qobuz entities (`QobuzArtist`, `QobuzAlbum`, `QobuzTrack`)
- Authentication handling with automatic token refresh
- API endpoints for HTTP integration (when `api` feature is enabled)
- Database persistence for credentials and app configuration (when `db` feature is enabled)

## Feature Flags

Available features in `Cargo.toml`:

- `api` - Enable Actix-web API endpoints (default)
- `db` - Enable database persistence for credentials (default)
- `openapi` - Enable OpenAPI schema generation (default)
- `scan` - Enable library scanning support (default)
- `fail-on-warnings` - Treat warnings as errors in dependencies

## Usage

### Basic Setup

```rust
use moosicbox_qobuz::QobuzMusicApi;
use switchy::database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new(/* ... */);

    // Build the Qobuz API
    let qobuz = QobuzMusicApi::builder()
        .with_db(db)
        .build()
        .await?;

    Ok(())
}
```

### Authentication

The API uses username/password authentication with automatic credential persistence (when `db` feature is enabled):

```rust
use moosicbox_qobuz::user_login;
use switchy::database::profiles::LibraryDatabase;

async fn login(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    let result = user_login(
        db,
        "username",
        "password",
        None,        // optional app_id
        Some(true),  // persist credentials
    )
    .await?;

    println!("Login successful: {}", result);
    Ok(())
}
```

### Using the MusicApi Interface

The `QobuzMusicApi` implements the `MusicApi` trait, providing a consistent interface:

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::id::Id;

async fn get_artist(api: &QobuzMusicApi, artist_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    let artist = api.artist(&Id::from(artist_id)).await?;

    if let Some(artist) = artist {
        println!("Artist: {}", artist.title);
    }

    Ok(())
}

async fn get_albums(api: &QobuzMusicApi) -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_music_api::models::AlbumsRequest;

    let request = AlbumsRequest {
        page: Some(moosicbox_paging::PageRequest { offset: 0, limit: 50 }),
        ..Default::default()
    };

    let albums = api.albums(&request).await?;

    for album in albums.iter() {
        println!("Album: {} - {}", album.title, album.artist);
    }

    Ok(())
}
```

### Direct Function Usage

Core functions are also available for direct use:

```rust
use moosicbox_qobuz::{artist, album, track, search};
use moosicbox_music_models::id::Id;

async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get artist
    let artist = artist(db, &Id::from(12345), None, None).await?;
    println!("Artist: {}", artist.name);

    // Get album
    let album = album(db, &"album_id".into(), None, None).await?;
    println!("Album: {}", album.title);

    // Get track
    let track = track(db, &Id::from(67890), None, None).await?;
    println!("Track: {}", track.title);

    // Search
    let results = search(db, "Miles Davis", Some(0), Some(10), None, None).await?;
    println!("Found {} artists", results.artists.items.len());
    println!("Found {} albums", results.albums.items.len());
    println!("Found {} tracks", results.tracks.items.len());

    Ok(())
}
```

### Favorites Management

```rust
use moosicbox_qobuz::{
    favorite_artists, favorite_albums, favorite_tracks,
    add_favorite_artist, remove_favorite_artist,
    add_favorite_album, remove_favorite_album,
    add_favorite_track, remove_favorite_track,
};

async fn manage_favorites(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get favorite artists
    let artists = favorite_artists(db, Some(0), Some(50), None, None).await?;

    // Get favorite albums
    let albums = favorite_albums(db, Some(0), Some(50), None, None, None).await?;

    // Get favorite tracks
    let tracks = favorite_tracks(db, Some(0), Some(50), None, None).await?;

    // Add to favorites
    add_favorite_artist(db, &Id::from(12345), None, None).await?;
    add_favorite_album(db, &"album_id".into(), None, None).await?;
    add_favorite_track(db, &Id::from(67890), None, None).await?;

    // Remove from favorites
    remove_favorite_artist(db, &Id::from(12345), None, None).await?;
    remove_favorite_album(db, &"album_id".into(), None, None).await?;
    remove_favorite_track(db, &Id::from(67890), None, None).await?;

    Ok(())
}
```

### Track Streaming

```rust
use moosicbox_qobuz::{track_file_url, QobuzAudioQuality};

async fn get_stream_url(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    let url = track_file_url(
        db,
        &Id::from(12345),
        QobuzAudioQuality::FlacHiRes,  // 24-bit/96kHz
        None,  // access_token
        None,  // app_id
        None,  // app_secret
    )
    .await?;

    println!("Stream URL: {}", url);
    Ok(())
}
```

### Quality Settings

```rust
use moosicbox_qobuz::QobuzAudioQuality;

// Available quality levels
let quality = QobuzAudioQuality::Low;            // MP3 320kbps
let quality = QobuzAudioQuality::FlacLossless;   // FLAC 16-bit/44.1kHz
let quality = QobuzAudioQuality::FlacHiRes;      // FLAC 24-bit/96kHz
let quality = QobuzAudioQuality::FlacHighestRes; // FLAC 24-bit/192kHz
```

## API Endpoints

When the `api` feature is enabled, the following HTTP endpoints are available:

- `POST /auth/login` - Authenticate with username/password
- `GET /artists` - Get artist by ID
- `GET /favorites/artists` - Get favorite artists
- `GET /albums` - Get album by ID
- `GET /favorites/albums` - Get favorite albums
- `GET /artists/albums` - Get albums for an artist
- `GET /albums/tracks` - Get tracks for an album
- `GET /tracks` - Get track by ID
- `GET /favorites/tracks` - Get favorite tracks
- `GET /track/url` - Get streaming URL for a track
- `GET /search` - Search for artists, albums, and tracks

### Binding API Endpoints

```rust
use actix_web::{App, HttpServer};
use moosicbox_qobuz::api::bind_services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(
            bind_services(actix_web::web::scope("/qobuz"))
        )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
```

## Models

### QobuzArtist

```rust
pub struct QobuzArtist {
    pub id: u64,
    pub image: Option<QobuzImage>,
    pub name: String,
}
```

### QobuzAlbum

```rust
pub struct QobuzAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: QobuzAlbumReleaseType,
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub version: Option<String>,
    pub duration: u32,
    pub tracks_count: u32,
    pub maximum_sampling_rate: f32,
    // ... additional fields
}
```

### QobuzTrack

```rust
pub struct QobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub artist: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: String,
    pub album_type: QobuzAlbumReleaseType,
    pub title: String,
    pub duration: u32,
    // ... additional fields
}
```

## Album Types

```rust
pub enum QobuzAlbumReleaseType {
    Album,
    Live,
    Compilation,
    Ep,
    Single,
    EpSingle,
    Other,
    Download,
}
```

## Error Handling

The package uses a custom `Error` type:

```rust
pub enum Error {
    NoUserIdAvailable,
    NoAccessTokenAvailable,
    Unauthorized,
    HttpRequestFailed(u16, String),
    NoAppId,
    NoAppSecretAvailable,
    // ... other variants
}
```

## Integration with MoosicBox

The Qobuz package integrates seamlessly with the MoosicBox music API system:

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_qobuz::QobuzMusicApi;

async fn register_qobuz() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new(/* ... */);

    let qobuz = QobuzMusicApi::builder()
        .with_db(db)
        .build()
        .await?;

    // Use as a MusicApi implementation
    let artists = qobuz.artists(None, None, None, None).await?;

    Ok(())
}
```

## Dependencies

Key dependencies (from `Cargo.toml`):

- `moosicbox_music_api` - Music API trait definitions
- `moosicbox_music_models` - Common music models
- `moosicbox_paging` - Pagination support
- `actix-web` - Web framework (optional, with `api` feature)
- `serde` / `serde_json` - Serialization
- `switchy` - HTTP client, database, and async runtime abstractions

## Notes

- App ID and app secret are automatically fetched from Qobuz's web interface if not provided
- Credentials are automatically refreshed on 401 responses when username is available
- The `db` feature enables persistence of authentication tokens and app configuration
- Search results combine artists, albums, and tracks into a unified response
- All API functions require a `LibraryDatabase` reference when the `db` feature is enabled

## See Also

- [MoosicBox Music API](../music_api/README.md) - Core music API trait definitions
- [MoosicBox Server](../server/README.md) - Main server application
- [MoosicBox Tidal](../tidal/README.md) - Alternative streaming service integration
