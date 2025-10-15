# MoosicBox Music API Models

Data models and types for music API operations and requests.

## Overview

The MoosicBox Music API Models package provides:

- **Request Models**: Structured request types for music API operations
- **Filter Models**: Advanced filtering and search capabilities
- **Ordering Types**: Sorting and ordering enums for API responses
- **Quality Types**: Audio quality and format specifications
- **Source Types**: Track and image source definitions

## Models

### Request Types

- **AlbumsRequest**: Album query with filtering, sorting, and pagination
- **AlbumFilters**: Advanced album filtering by name, artist, type, etc.
- **PagingRequest**: Pagination parameters for large result sets

### Ordering & Sorting

- **ArtistOrder/ArtistOrderDirection**: Artist sorting options
- **AlbumOrder/AlbumOrderDirection**: Album sorting options
- **TrackOrder/TrackOrderDirection**: Track sorting options

### Audio Quality

- **TrackAudioQuality**: Audio quality levels (Low, FlacLossless, FlacHiRes, FlacHighestRes)
- **AudioFormat**: Supported audio formats integration
- **Quality Mapping**: Quality to format conversion

### Source Types

- **TrackSource**: Local file paths and remote URLs for tracks
- **ImageCoverSource**: Local and remote image sources
- **ImageCoverSize**: Cover art size specifications

### Search Integration

- **Search Models**: Search request and response types (with `api-search` feature)
- **Query Processing**: Search query parsing and handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_api_models = { path = "../music_api/models" }

# Enable API search functionality
moosicbox_music_api_models = {
    path = "../music_api/models",
    features = ["api-search"]
}
```

## Usage

### Album Requests

```rust
use moosicbox_music_api_models::{AlbumsRequest, AlbumFilters};
use moosicbox_music_models::{AlbumSort, AlbumType};
use moosicbox_paging::PagingRequest;

let request = AlbumsRequest {
    sources: Some(vec![AlbumSource::Local]),
    sort: Some(AlbumSort::NameAsc),
    filters: Some(AlbumFilters {
        album_type: Some(AlbumType::Lp),
        artist: Some("Beatles".to_string()),
        ..Default::default()
    }),
    page: Some(PagingRequest {
        offset: 0,
        limit: 50,
    }),
};
```

### Audio Quality

```rust
use moosicbox_music_api_models::TrackAudioQuality;

// Specify desired audio quality
let quality = TrackAudioQuality::FlacHiRes;

// Quality levels:
// - Low: MP3 320kbps
// - FlacLossless: FLAC 16-bit 44.1kHz
// - FlacHiRes: FLAC 24-bit ≤96kHz
// - FlacHighestRes: FLAC 24-bit >96kHz ≤192kHz
```

### Track Sources

```rust
use moosicbox_music_api_models::TrackSource;
use moosicbox_music_models::{AudioFormat, TrackApiSource};

// Local file source
let local_source = TrackSource::LocalFilePath {
    path: "/music/artist/album/track.flac".to_string(),
    format: AudioFormat::Flac,
    track_id: Some(123.into()),
    source: TrackApiSource::Local,
};

// Remote URL source
let remote_source = TrackSource::RemoteUrl {
    url: "https://api.service.com/track/123".to_string(),
    format: AudioFormat::Flac,
    track_id: Some(123.into()),
    source: TrackApiSource::Api(api_source),
    headers: Some(vec![("Authorization".to_string(), "Bearer token".to_string())]),
};
```

### Image Sources

```rust
use moosicbox_music_api_models::{ImageCoverSource, ImageCoverSize};

// Local cover art
let local_cover = ImageCoverSource::LocalFilePath(
    "/covers/album_123.jpg".to_string()
);

// Remote cover art
let remote_cover = ImageCoverSource::RemoteUrl {
    url: "https://covers.service.com/album/123".to_string(),
    headers: Some(vec![("User-Agent".to_string(), "MoosicBox".to_string())]),
};

// Size specifications
let size = ImageCoverSize::Large; // Max, Large, Medium, Small, Thumbnail
```

## Feature Flags

- **`api`**: Enable API-related functionality (default)
- **`api-search`**: Enable API search models and types (default)
- **`db`**: Enable database integration (default)
- **`openapi`**: Enable OpenAPI schema generation (default)
- **`search`**: Enable core search functionality with Tantivy integration
- **`fail-on-warnings`**: Treat warnings as errors during compilation

## Dependencies

### Core Dependencies

- **MoosicBox Music Models**: Core music data types
- **MoosicBox JSON Utils**: JSON parsing utilities (with `serde_json` feature)
- **MoosicBox Paging**: Pagination support
- **Switchy Database**: Database type integration
- **Serde**: Serialization and deserialization
- **Strum**: Enum string conversion

### Optional Dependencies

- **Tantivy**: Full-text search engine (with `search` feature)
- **utoipa**: OpenAPI schema generation (with `openapi` feature)
