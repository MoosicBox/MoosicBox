# MoosicBox Library

Basic music library management providing database operations for artists, albums, and tracks with support for favorites, search, and pagination.

## Features

- **Artist Management**: List, retrieve, and manage favorite artists
- **Album Management**: Browse albums with filtering, sorting, and pagination
- **Track Management**: Access track information and manage favorites
- **Search Functionality**: Search across artists, albums, and tracks
- **Pagination Support**: Efficient browsing of large music collections
- **Filtering & Sorting**: Filter albums by artist and sort by various criteria
- **Database Integration**: Async database operations with proper error handling
- **Version Support**: Handle multiple album versions from different sources

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_library = "0.1.1"

# Enable additional features
moosicbox_library = { version = "0.1.1", features = ["api"] }
```

## Usage

### Working with Artists

```rust
use moosicbox_library::{favorite_artists, add_favorite_artist, remove_favorite_artist};
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;

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
use moosicbox_music_models::Id;
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;

    // Get favorite albums
    let request = AlbumsRequest::default();
    let albums_result = favorite_albums(&db, &request).await?;

    for album in albums_result.page.items() {
        println!("Album: {} by {}", album.title, album.artist);
        println!("  Date: {:?}, Duration: {}s", album.date_released, album.duration);

        // Get album details
        let album_id = Id::Number(album.id);
        if let Some(album_detail) = album(&db, &album_id).await? {
            println!("  Detailed info: {} tracks", album_detail.versions.len());
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
            println!("    Source: {:?}, Quality: {:?}", version.source, version.audio_format);
        }
    }

    Ok(())
}
```

### Track Management

```rust
use moosicbox_library::{favorite_tracks, track, add_favorite_track, remove_favorite_track};
use moosicbox_music_models::Id;
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;

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
use moosicbox_music_api_models::{AlbumsRequest, AlbumFilters, AlbumSort};
use moosicbox_music_models::{Id, ApiSource};

fn example_filtering() {
    let albums = vec![]; // Your album collection

    // Create filter request
    let request = AlbumsRequest {
        sources: Some(vec![ApiSource::Library]),
        sort: Some(AlbumSort::Artist),
        filters: Some(AlbumFilters {
            name: Some("Dark Side".to_string()),
            artist_id: Some(Id::Number(123)),
            artist_api_id: None,
            search: None,
        }),
        ..Default::default()
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
use moosicbox_library::{search, SearchType, LibrarySearchType};

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

    if let Some(artists) = &results.artists {
        println!("  Artists: {}", artists.len());
        for artist in artists {
            println!("    {}", artist.name);
        }
    }

    if let Some(albums) = &results.albums {
        println!("  Albums: {}", albums.len());
        for album in albums {
            println!("    {} - {}", album.artist_name, album.name);
        }
    }

    if let Some(tracks) = &results.tracks {
        println!("  Tracks: {}", tracks.len());
        for track in tracks {
            println!("    {} - {}", track.artist_name, track.name);
        }
    }

    Ok(())
}
```

### Artist Albums

```rust
use moosicbox_library::artist_albums;
use moosicbox_library_models::LibraryAlbumType;
use moosicbox_music_models::Id;
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;
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

    // Get only studio albums
    let studio_albums = artist_albums(
        &db,
        &artist_id,
        Some(0),
        Some(10),
        Some(LibraryAlbumType::Studio),
    ).await?;

    println!("Studio albums: {}", studio_albums.page.items().len());

    Ok(())
}
```

### Track File URLs

```rust
use moosicbox_library::{track_file_url, LibraryAudioQuality};
use moosicbox_music_models::Id;
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;
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

- `favorite_artists()` - Get paginated favorite artists
- `favorite_albums()` - Get paginated favorite albums with filtering
- `favorite_tracks()` - Get paginated favorite tracks
- `artist()` - Get artist by ID
- `album()` - Get album by ID
- `track()` - Get track by ID
- `artist_albums()` - Get albums by specific artist
- `album_tracks()` - Get tracks in specific album
- `album_versions()` - Get available versions of an album

### Utility Functions

- `filter_albums()` - Filter albums by criteria
- `sort_albums()` - Sort albums by specified order
- `search()` - Search across library content
- `track_file_url()` - Get file URL for track playback

### Management Functions

- `add_favorite_artist()` - Add artist to favorites (placeholder)
- `remove_favorite_artist()` - Remove artist from favorites (placeholder)
- `add_favorite_album()` - Add album to favorites (placeholder)
- `remove_favorite_album()` - Remove album from favorites (placeholder)
- `add_favorite_track()` - Add track to favorites (placeholder)
- `remove_favorite_track()` - Remove track from favorites (placeholder)

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

- `api` - Enable API integration features

## Dependencies

- `switchy_database` - Database connection and operations
- `moosicbox_music_models` - Core music data models
- `moosicbox_library_models` - Library-specific data models
- `moosicbox_paging` - Pagination utilities
- `moosicbox_search` - Search functionality
