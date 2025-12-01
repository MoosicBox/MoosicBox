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

### Source Types

- **TrackSource**: Local file paths and remote URLs for tracks
- **ImageCoverSource**: Local and remote image sources
- **ImageCoverSize**: Cover art size specifications
- **FromId**: Trait for ID type conversions between strings and numeric IDs

### Search Integration

- **Search Models**: Search response types (with `api-search` feature)

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
use moosicbox_music_models::{AlbumSort, AlbumSource, AlbumType};
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
    source: TrackApiSource::Local,
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

### ID Types and Conversions

MoosicBox supports flexible ID handling for different API sources. The primary ID type is the `Id` enum from `moosicbox_music_models::id::Id`, which can represent both numeric and string identifiers.

#### Using the Id Enum (Recommended)

The `Id` enum is the main ID type used throughout the codebase. It automatically handles both numeric IDs (for local library items) and string IDs (for external API sources).

```rust
use moosicbox_music_models::id::Id;

// Create IDs from different types
let numeric_id: Id = 12345u64.into();
let string_id: Id = "abc123".into();

// Pattern match to check ID type
match numeric_id {
    Id::Number(n) => println!("Numeric ID: {}", n),
    Id::String(s) => println!("String ID: {}", s),
}

// Extract values safely
if let Some(num) = numeric_id.as_number() {
    println!("ID as u64: {}", num);
}

if let Some(s) = string_id.as_str() {
    println!("ID as string: {}", s);
}

// Convert to string for display
println!("ID: {}", numeric_id); // Uses Display trait

// Parse from string with API source context
use moosicbox_music_models::ApiSource;
let parsed_id = Id::from_str("12345", &ApiSource::library());
```

#### FromId Trait (Low-level Helper)

The `FromId` trait provides generic conversion methods between ID representations and strings. This is a helper trait mainly used internally for type-agnostic ID handling.

```rust
use moosicbox_music_api_models::FromId;

// For numeric types (u64)
let id: u64 = 12345;
let id_string = id.as_string();      // "12345"
let parsed: u64 = u64::into_id("12345");

// For string types
let str_id = String::from("abc123");
let id_string = str_id.as_string();  // "abc123"
let parsed = String::into_id("abc123");
```

**Note**: While `FromId` provides useful conversion methods, prefer using the `Id` enum for most use cases as it provides better type safety and integrates seamlessly with the rest of the MoosicBox API.

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
