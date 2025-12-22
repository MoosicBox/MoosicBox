# MoosicBox Remote Library

Remote music library client for the MoosicBox ecosystem, providing HTTP-based access to remote MoosicBox servers through the standard Music API interface.

## Features

- **Remote Server Access**: HTTP client for accessing remote MoosicBox servers
- **Music API Implementation**: Full Music API interface for remote servers
- **Artist/Album/Track Queries**: Browse artists, albums, and tracks remotely
- **Search Support**: Search functionality across remote libraries
- **Pagination Support**: Efficient handling of large datasets with pagination
- **Profile Support**: Multi-profile support for different user configurations
- **Error Handling**: Robust error handling for network and API failures

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_remote_library = "0.1.4"
```

## Usage

### Creating a Remote Library Client

```rust
use moosicbox_remote_library::RemoteLibraryMusicApi;
use moosicbox_music_models::ApiSource;

// Create a remote library client
let remote_api = RemoteLibraryMusicApi::new(
    "http://192.168.1.100:8080".to_string(),  // server host
    ApiSource::library(),                      // API source
    "default".to_string(),                     // profile name
);
```

### Using the Music API Interface

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::id::Id;

async fn browse_remote_library() -> Result<(), Box<dyn std::error::Error>> {
    let remote_api = RemoteLibraryMusicApi::new(/* ... */);

    // Get artists with pagination
    let artists_page = remote_api.artists(
        Some(0),    // offset
        Some(50),   // limit
        None,       // order
        None,       // order direction
    ).await?;

    println!("Found {} artists", artists_page.items().len());
    for artist in artists_page.items() {
        println!("Artist: {}", artist.title);
    }

    // Get specific artist
    let artist_id = Id::Number(123);
    if let Some(artist) = remote_api.artist(&artist_id).await? {
        println!("Artist details: {}", artist.title);
    }

    // Get albums for artist
    let albums_page = remote_api.artist_albums(
        &artist_id,
        None,       // album type
        Some(0),    // offset
        Some(20),   // limit
        None,       // order
        None,       // order direction
    ).await?;

    for album in albums_page.items() {
        println!("Album: {}", album.title);
    }

    Ok(())
}
```

### Track and Album Operations

```rust
async fn get_tracks_and_albums() -> Result<(), Box<dyn std::error::Error>> {
    let remote_api = RemoteLibraryMusicApi::new(/* ... */);

    // Get specific album
    let album_id = Id::Number(456);
    if let Some(album) = remote_api.album(&album_id).await? {
        println!("Album: {}", album.title);

        // Get tracks in album
        let tracks_page = remote_api.album_tracks(
            &album_id,
            Some(0),    // offset
            Some(100),  // limit
            None,       // order
            None,       // order direction
        ).await?;

        for track in tracks_page.items() {
            println!("Track {}: {}",
                     track.number,
                     track.title);
        }
    }

    // Get specific track
    let track_id = Id::Number(789);
    if let Some(track) = remote_api.track(&track_id).await? {
        println!("Track: {} - {}",
                 track.artist,
                 track.title);
    }

    Ok(())
}
```

### Search Functionality

```rust
async fn search_remote_library() -> Result<(), Box<dyn std::error::Error>> {
    let remote_api = RemoteLibraryMusicApi::new(/* ... */);

    // Search for music
    let search_results = remote_api.search(
        "Pink Floyd",
        Some(0),   // offset
        Some(20),  // limit
    ).await?;

    println!("Total results found: {}", search_results.results.len());

    for result in search_results.results {
        match result {
            moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Artist(artist) => {
                println!("🎤 Artist: {}", artist.title);
            }
            moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Album(album) => {
                println!("💿 Album: {}", album.title);
            }
            moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Track(track) => {
                println!("🎵 Track: {} - {}", track.artist, track.title);
            }
        }
    }

    Ok(())
}
```

### Album Versions Support

```rust
async fn get_album_versions() -> Result<(), Box<dyn std::error::Error>> {
    let remote_api = RemoteLibraryMusicApi::new(/* ... */);
    let album_id = Id::Number(123);

    // Get different versions of an album (remasters, deluxe editions, etc.)
    let versions_page = remote_api.album_versions(
        &album_id,
        Some(0),   // offset
        Some(10),  // limit
    ).await?;

    for version in versions_page.items() {
        println!("Album version format: {:?}", version.format);
    }

    Ok(())
}
```

## Error Handling

The client handles various error conditions:

```rust
use moosicbox_music_api::Error as MusicApiError;

match remote_api.artist(&artist_id).await {
    Ok(Some(artist)) => println!("Found artist: {}", artist.title),
    Ok(None) => println!("Artist not found"),
    Err(MusicApiError::Other(e)) => eprintln!("Network/API error: {}", e),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Core Types

The library implements the standard `MusicApi` trait, providing:

- Artist, album, and track queries
- Search functionality
- Pagination support
- Album version management
- Cover art source handling

## Dependencies

- `moosicbox_menu_models`: Menu model types with API support
- `moosicbox_music_api`: Music API trait definitions
- `moosicbox_music_models`: Data models for music entities
- `moosicbox_paging`: Pagination utilities
- `switchy_http`: HTTP client for API requests

This library enables MoosicBox applications to seamlessly access remote music libraries as if they were local, providing a unified interface for distributed music systems.
