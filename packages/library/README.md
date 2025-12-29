# MoosicBox Library

Music library management package providing database operations for artists, albums, and tracks with support for filtering, search, and pagination.

## Features

- **Artist Management**: List and retrieve artists from the library
- **Album Management**: Browse albums with filtering, sorting, and pagination
- **Track Management**: Access track information with detailed metadata
- **Search Functionality**: Full-text search across artists, albums, and tracks
- **Pagination Support**: Efficient browsing of large music collections
- **Filtering & Sorting**: Filter albums by artist, name, source, and sort by various criteria
- **Database Integration**: Async database operations with proper error handling
- **Version Support**: Manage multiple album versions from different sources with varying quality levels

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_library = "0.1.4"

# Enable additional features
moosicbox_library = { version = "0.1.4", features = ["api"] }
```

## Usage

### Working with Artists

```rust
use moosicbox_library::favorite_artists;
use switchy_database::profiles::LibraryDatabase;

// LibraryDatabase is obtained from the PROFILES registry or via actix-web request extraction
async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get favorite artists with pagination
    let artists_result = favorite_artists(
        &db,
        Some(0),    // offset
        Some(20),   // limit
        None,       // order
        None,       // order direction
    ).await?;

    println!("Found {} artists", artists_result.page.total().unwrap_or(0));

    for artist in artists_result.page.items() {
        println!("Artist: {} (ID: {})", artist.title, artist.id);
    }

    Ok(())
}
```

### Album Operations

```rust
use moosicbox_library::{favorite_albums, album, album_tracks, album_versions};
use moosicbox_music_api_models::AlbumsRequest;
use moosicbox_music_models::id::Id;
use switchy_database::profiles::LibraryDatabase;

// LibraryDatabase is obtained from the PROFILES registry or via actix-web request extraction
async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get favorite albums
    let request = AlbumsRequest::default();
    let albums_result = favorite_albums(&db, &request).await?;

    for album in albums_result.page.items() {
        println!("Album: {} by {}", album.title, album.artist);
        println!("  Date: {:?}", album.date_released);

        // Get album details
        let album_id = Id::Number(album.id);
        if let Some(album_detail) = album(&db, &album_id).await? {
            println!("  Detailed info: {} versions", album_detail.versions.len());
        }

        // Get album tracks
        let tracks_result = album_tracks(&db, &album_id, Some(0), Some(10)).await?;
        println!("  Tracks ({}):", tracks_result.page.items().len());

        for track in tracks_result.page.items() {
            println!("    {}: {} ({}s)", track.number, track.title, track.duration);
        }

        // Get album versions
        let versions = album_versions(&db, &album_id).await?;
        println!("  Available versions: {}", versions.len());
        for version in versions {
            println!("    Source: {:?}, Format: {:?}", version.source, version.format);
        }
    }

    Ok(())
}
```

### Track Management

```rust
use moosicbox_library::{favorite_tracks, track};
use moosicbox_music_models::id::Id;
use switchy_database::profiles::LibraryDatabase;

// LibraryDatabase is obtained from the PROFILES registry or via actix-web request extraction
async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get favorite tracks
    let tracks_result = favorite_tracks(
        &db,
        None,       // track_ids filter
        Some(0),    // offset
        Some(50),   // limit
        None,       // order
        None,       // order direction
    ).await?;

    println!("Favorite tracks: {}", tracks_result.page.items().len());

    for track in tracks_result.page.items() {
        println!("Track: {} - {} ({}s)", track.artist, track.title, track.duration);

        // Get detailed track info
        let track_id = Id::Number(track.id);
        if let Some(track_detail) = track(&db, &track_id).await? {
            println!("  Album: {}", track_detail.album);
            println!("  Track #: {}", track_detail.number);
        }
    }

    Ok(())
}
```

### Filtering and Sorting Albums

```rust
use moosicbox_library::{filter_albums, sort_albums};
use moosicbox_music_api_models::{AlbumsRequest, AlbumFilters};
use moosicbox_music_models::{AlbumSort, id::Id};

fn example_filtering() {
    let albums = vec![]; // Your album collection

    // Create filter request
    let request = AlbumsRequest {
        sources: None,
        sort: Some(AlbumSort::ArtistAsc),
        filters: Some(AlbumFilters {
            name: Some("Dark Side".to_string()),
            artist_id: Some(Id::Number(123)),
            artist_api_id: None,
            artist: None,
            album_type: None,
            search: None,
        }),
        page: None,
    };

    // Filter albums
    let filtered: Vec<_> = filter_albums(&albums, &request).collect();
    println!("Filtered to {} albums", filtered.len());

    // Sort albums
    let sorted = sort_albums(filtered, &request);
    for album in sorted {
        println!("  {}: {}", album.artist, album.title);
    }
}
```

### Search Operations

```rust
use moosicbox_library::{search, LibrarySearchType};
use moosicbox_music_api_models::search::api::ApiGlobalSearchResult;

fn search_library() -> Result<(), Box<dyn std::error::Error>> {
    let query = "Pink Floyd";
    let search_types = vec![
        LibrarySearchType::Artists,
        LibrarySearchType::Albums,
        LibrarySearchType::Tracks,
    ];

    let results = search(
        query,
        Some(0),    // offset
        Some(20),   // limit
        Some(&search_types),
    )?;

    println!("Search results for '{}':", query);
    println!("Found {} results", results.results.len());

    for result in &results.results {
        match result {
            ApiGlobalSearchResult::Artist(artist) => {
                println!("  Artist: {}", artist.title);
            }
            ApiGlobalSearchResult::Album(album) => {
                println!("  Album: {} - {}", album.artist, album.title);
            }
            ApiGlobalSearchResult::Track(track) => {
                println!("  Track: {} - {}", track.artist, track.title);
            }
        }
    }

    Ok(())
}
```

### Artist Albums

```rust
use moosicbox_library::artist_albums;
use moosicbox_library_models::LibraryAlbumType;
use moosicbox_music_models::id::Id;
use switchy_database::profiles::LibraryDatabase;

// LibraryDatabase is obtained from the PROFILES registry or via actix-web request extraction
async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    let artist_id = Id::Number(123);

    // Get all albums by artist
    let albums_result = artist_albums(
        &db,
        &artist_id,
        Some(0),    // offset
        Some(10),   // limit
        None,       // album type filter
    ).await?;

    println!("Albums by artist: {}", albums_result.page.items().len());

    for album in albums_result.page.items() {
        println!("  {} ({})", album.title, album.date_released.unwrap_or_default());
    }

    // Get only LP albums
    let lp_albums = artist_albums(
        &db,
        &artist_id,
        Some(0),
        Some(10),
        Some(LibraryAlbumType::Lp),
    ).await?;

    println!("LP albums: {}", lp_albums.page.items().len());

    Ok(())
}
```

### Track File URLs

```rust
use moosicbox_library::{track_file_url, LibraryAudioQuality};
use moosicbox_music_models::id::Id;
use switchy_database::profiles::LibraryDatabase;

// LibraryDatabase is obtained from the PROFILES registry or via actix-web request extraction
async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    let track_id = Id::Number(456);

    // Get high quality track URL
    let url = track_file_url(
        &db,
        LibraryAudioQuality::High,
        &track_id,
    ).await?;

    println!("Track URL: {}", url);

    Ok(())
}
```

## API Reference

### Core Functions

- `favorite_artists()` - Get paginated artists from the library
- `favorite_albums()` - Get paginated albums with filtering and sorting
- `favorite_tracks()` - Get paginated tracks with optional ID filtering
- `artist()` - Get artist by ID
- `album()` - Get album by ID
- `album_from_source()` - Get album by ID and API source
- `track()` - Get track by ID
- `artist_albums()` - Get albums by specific artist with optional type filtering
- `album_tracks()` - Get tracks in specific album
- `album_versions()` - Get available versions of an album grouped by quality

### Utility Functions

- `filter_albums()` - Filter albums by criteria
- `sort_albums()` - Sort albums by specified order
- `search()` - Search across library content
- `track_file_url()` - Get file URL for track playback
- `reindex_global_search_index()` - Rebuild the search index from library data

### Management Functions

**Note:** The following functions are currently non-functional placeholders that return `Ok(())`:

- `add_favorite_artist()` - Placeholder: Add artist to favorites
- `remove_favorite_artist()` - Placeholder: Remove artist from favorites
- `add_favorite_album()` - Placeholder: Add album to favorites
- `remove_favorite_album()` - Placeholder: Remove album from favorites
- `add_favorite_track()` - Placeholder: Add track to favorites
- `remove_favorite_track()` - Placeholder: Remove track from favorites

## Error Handling

The library provides specific error types for different operations:

- `LibraryFavoriteArtistsError` - Errors when fetching favorite artists
- `LibraryFavoriteAlbumsError` - Errors when fetching favorite albums
- `LibraryFavoriteTracksError` - Errors when fetching favorite tracks
- `LibraryArtistError` - Errors when fetching artist information
- `LibraryAlbumError` - Errors when fetching album information
- `LibraryTrackError` - Errors when fetching track information
- `SearchError` - Errors during search operations

## Enums and Types

### Order and Direction

- `LibraryArtistOrder`, `LibraryAlbumOrder`, `LibraryTrackOrder`
- `LibraryArtistOrderDirection`, `LibraryAlbumOrderDirection`, `LibraryTrackOrderDirection`

### Search Types

- `LibrarySearchType` - Artists, Albums, Tracks, Videos, Playlists, UserProfiles
- `SearchType` - Enum variants for search filtering

### Audio Quality

- `LibraryAudioQuality` - High, Lossless, HiResLossless

## Features

The following cargo features are available:

- `api` - Enable API integration features (actix-web support)
- `openapi` - Enable OpenAPI/utoipa schema generation
- `fail-on-warnings` - Treat warnings as errors during compilation
- `all-encoders` - Enable all audio encoders
- `all-formats` - Enable all audio format support
- Format-specific features: `format-aac`, `format-flac`, `format-mp3`, `format-opus`
- Encoder-specific features: `encoder-aac`, `encoder-flac`, `encoder-mp3`, `encoder-opus`

## Dependencies

- `switchy_database` - Database connection and operations
- `moosicbox_music_models` - Core music data models
- `moosicbox_library_models` - Library-specific data models
- `moosicbox_paging` - Pagination utilities
- `moosicbox_search` - Search functionality
