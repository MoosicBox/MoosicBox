# MoosicBox YouTube Music

YouTube Music API integration framework providing streaming access and library management for YouTube Music content within the MoosicBox ecosystem.

**Note**: This package provides a framework for YouTube Music integration. The actual API endpoints are currently stubbed and require implementation to connect to YouTube Music services.

## Features

- **YouTube Music API Framework**: Structure for integrating with YouTube Music streaming service
- **Authentication Framework**: Device flow authentication structure for user accounts
- **Library Access**: Manage favorite artists, albums, and tracks
- **Search Functionality**: Search across YouTube Music's catalog
- **Audio Streaming**: High-quality audio streaming with multiple quality options (High, Lossless, HiResLossless)
- **Database Integration**: Local caching and configuration storage with the `db` feature
- **Artist/Album Browsing**: Browse artist catalogs and album collections by type (LP, EPs/Singles, Compilations)
- **Track Information**: Detailed track metadata and playback information
- **MusicApi Trait**: Implements the generic `MusicApi` trait for compatibility with MoosicBox ecosystem

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_yt = "0.1.4"

# Enable features as needed
moosicbox_yt = { version = "0.1.4", features = ["db", "api", "openapi"] }
```

## Usage

### Setting Up the API

```rust
use moosicbox_yt::YtMusicApi;
#[cfg(feature = "db")]
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "db")]
    let db = LibraryDatabase::new().await?;

    #[cfg(feature = "db")]
    let yt_api = YtMusicApi::builder()
        .with_db(db)
        .build()
        .await?;

    println!("YouTube Music API ready");
    Ok(())
}
```

### Authentication Flow

**Note**: The authentication endpoints are currently framework stubs. To use authentication, you need to implement the actual YouTube Music OAuth2 flow.

```rust
use moosicbox_yt::{device_authorization, device_authorization_token};
#[cfg(feature = "db")]
use switchy_database::profiles::LibraryDatabase;

async fn authenticate_user() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = "your-client-id".to_string();
    let client_secret = "your-client-secret".to_string();

    // Start device authorization
    let auth_response = device_authorization(client_id.clone(), true).await?;

    println!("Visit: {}", auth_response["url"]);
    println!("Device code: {}", auth_response["device_code"]);

    // Wait for user to authorize, then get token
    let device_code = auth_response["device_code"].as_str().unwrap().to_string();

    #[cfg(feature = "db")]
    let db = LibraryDatabase::new().await?;

    let token_response = device_authorization_token(
        #[cfg(feature = "db")] &db,
        client_id,
        client_secret,
        device_code,
        #[cfg(feature = "db")] Some(true), // persist token
    ).await?;

    println!("Authentication successful!");
    Ok(())
}
```

### Browsing Favorite Artists

```rust
use moosicbox_yt::{favorite_artists, YtArtistOrder, YtArtistOrderDirection};

#[cfg(feature = "db")]
async fn browse_favorite_artists() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;

    let artists_result = favorite_artists(
        &db,
        Some(0),     // offset
        Some(20),    // limit
        Some(YtArtistOrder::Date),
        Some(YtArtistOrderDirection::Desc),
        None,        // country_code
        None,        // locale
        None,        // device_type
        None,        // access_token (will be fetched from db)
        None,        // user_id
    ).await?;

    println!("Favorite Artists:");
    for artist in artists_result.page.items() {
        println!("  {} (ID: {})", artist.name, artist.id);
    }

    Ok(())
}
```

### Managing Albums

```rust
use moosicbox_yt::{favorite_albums, album, album_tracks, add_favorite_album};
use moosicbox_music_models::Id;

#[cfg(feature = "db")]
async fn manage_albums() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;

    // Get favorite albums
    let albums_result = favorite_albums(
        &db,
        Some(0),     // offset
        Some(10),    // limit
        None,        // order
        None,        // order_direction
        None,        // country_code
        None,        // locale
        None,        // device_type
        None,        // access_token
        None,        // user_id
    ).await?;

    for album in albums_result.page.items() {
        println!("Album: {} by {}", album.title, album.artist);

        // Get album details
        let album_id = Id::String(album.id.clone());
        let album_detail = album(&db, &album_id, None, None, None, None).await?;
        println!("  Tracks: {}", album_detail.number_of_tracks);

        // Get album tracks
        let tracks_result = album_tracks(
            &db, &album_id, Some(0), Some(5), None, None, None, None
        ).await?;

        println!("  First 5 tracks:");
        for track in tracks_result.page.items() {
            println!("    {}: {}", track.track_number, track.title);
        }
    }

    // Add new favorite album
    let new_album_id = Id::String("album123".to_string());
    add_favorite_album(&db, &new_album_id, None, None, None, None, None).await?;
    println!("Added album to favorites");

    Ok(())
}
```

### Searching Content

```rust
use moosicbox_yt::search;

async fn search_content() -> Result<(), Box<dyn std::error::Error>> {
    let query = "Pink Floyd";
    let results = search(query, Some(0), Some(20)).await?;

    println!("Search results for '{}':", query);

    // Search returns YtSearchResults with complex nested structure
    // You can convert it to formatted results for easier access
    use moosicbox_yt::models::YtSearchResultsFormatted;
    let formatted: YtSearchResultsFormatted = results.into();

    println!("  Artists:");
    for artist in formatted.artists {
        println!("    {} ({})", artist.name, artist.id);
    }

    println!("  Albums:");
    for album in formatted.albums {
        println!("    {} - {}", album.artist, album.title);
    }

    println!("  Tracks:");
    for track in formatted.tracks {
        println!("    {} - {}", track.artist, track.title);
    }

    Ok(())
}
```

### Audio Streaming

```rust
use moosicbox_yt::{track_file_url, track_playback_info, YtAudioQuality};
use moosicbox_music_models::Id;

#[cfg(feature = "db")]
async fn stream_audio() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;
    let track_id = Id::String("track123".to_string());

    // Get streaming URLs
    let urls = track_file_url(
        &db,
        YtAudioQuality::High,
        &track_id,
        None, // access_token
    ).await?;

    for url in urls {
        println!("Stream URL: {}", url);
    }

    // Get detailed playback info
    let playback_info = track_playback_info(
        &db,
        YtAudioQuality::High,
        &track_id,
        None,
    ).await?;

    println!("Playback Info:");
    println!("  Quality: {}", playback_info.audio_quality);
    println!("  Sample Rate: {:?}", playback_info.sample_rate);
    println!("  Bit Depth: {:?}", playback_info.bit_depth);
    println!("  Track Replay Gain: {}", playback_info.track_replay_gain);

    Ok(())
}
```

### Artist Albums and Tracks

```rust
use moosicbox_yt::{artist_albums, YtAlbumType};
use moosicbox_music_models::Id;

#[cfg(feature = "db")]
async fn browse_artist_content() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new().await?;
    let artist_id = Id::String("artist123".to_string());

    // Get artist's studio albums
    let albums_result = artist_albums(
        &db,
        &artist_id,
        Some(0),     // offset
        Some(10),    // limit
        Some(YtAlbumType::Lp), // studio albums only
        None,        // country_code
        None,        // locale
        None,        // device_type
        None,        // access_token
    ).await?;

    println!("Studio Albums:");
    for album in albums_result.page.items() {
        println!("  {} ({})", album.title,
                 album.release_date.as_ref()
                      .and_then(|d| d.split('-').next())
                      .unwrap_or(""));
    }

    // Get EPs and singles
    let eps_result = artist_albums(
        &db, &artist_id, Some(0), Some(10),
        Some(YtAlbumType::EpsAndSingles), None, None, None, None
    ).await?;

    println!("EPs and Singles:");
    for album in eps_result.page.items() {
        println!("  {}", album.title);
    }

    Ok(())
}
```

### Using with MusicApi Trait

```rust
use moosicbox_yt::YtMusicApi;
use moosicbox_music_api::MusicApi;
use moosicbox_music_api_models::AlbumsRequest;

#[cfg(feature = "db")]
async fn use_music_api_trait() -> Result<(), Box<dyn std::error::Error>> {
    let yt_api = YtMusicApi::builder()
        .with_db(LibraryDatabase::new().await?)
        .build()
        .await?;

    // Use the generic MusicApi interface
    let artists = yt_api.artists(Some(0), Some(10), None, None).await?;
    println!("Artists via MusicApi: {}", artists.page.items().len());

    let albums_request = AlbumsRequest::default();
    let albums = yt_api.albums(&albums_request).await?;
    println!("Albums via MusicApi: {}", albums.page.items().len());

    // Search functionality
    let search_results = yt_api.search("rock music", Some(0), Some(20)).await?;
    if let Some(artists) = &search_results.artists {
        println!("Found {} artists", artists.len());
    }

    Ok(())
}
```

## API Reference

### Core Functions

- `device_authorization()` - Start OAuth2 device flow
- `device_authorization_token()` - Complete OAuth2 flow and get tokens
- `favorite_artists()`, `favorite_albums()`, `favorite_tracks()` - Get user favorites
- `add_favorite_*()`, `remove_favorite_*()` - Manage favorites
- `artist()`, `album()`, `track()` - Get individual items
- `artist_albums()`, `album_tracks()` - Get related content
- `search()` - Search across YouTube Music catalog
- `track_file_url()` - Get streaming URLs
- `track_playback_info()` - Get detailed audio information

### Types and Enums

- `YtAudioQuality` - High, Lossless, HiResLossless
- `YtAlbumType` - Lp, EpsAndSingles, Compilations
- `YtDeviceType` - Browser
- `YtArtistOrder`, `YtAlbumOrder`, `YtTrackOrder` - Sorting options
- `YtMusicApi` - Main API struct implementing `MusicApi` trait

### Models

- `YtArtist`, `YtAlbum`, `YtTrack` - YouTube Music specific data models
- `YtSearchResults` - Search response structure
- `YtTrackPlaybackInfo` - Detailed audio metadata

## Error Handling

The library provides comprehensive error handling through the `Error` enum:

- `NoUserIdAvailable` - User authentication required
- `NoAccessTokenAvailable` - Missing or invalid access token
- `Unauthorized` - Authentication failed
- `RequestFailed` - API request errors
- `Parse` - JSON parsing errors
- `Http` - Network errors
- `Database` - Database operation errors (with `db` feature)

## Features

- `db` - Enable database integration for token storage and caching (required for most operations)
- `api` - Enable API endpoint functionality with actix-web
- `openapi` - Enable OpenAPI/utoipa schema generation for API documentation
- `scan` - Enable library scanning functionality

## Authentication

The package provides a framework for OAuth2 device flow authentication:

1. Call `device_authorization()` to initiate the flow (returns URL and device code)
2. User visits URL to authorize the application
3. Call `device_authorization_token()` to complete flow and retrieve tokens
4. Tokens are automatically stored and managed (requires `db` feature)

**Note**: The actual OAuth2 endpoints are currently stubbed. Implementation requires connecting to YouTube Music's authentication service.

## Dependencies

Core dependencies:

- `moosicbox_music_api` - Generic music API trait implementation
- `moosicbox_music_models` - Common music data models
- `switchy_database` - Database integration (required for most features)
- `switchy_http` - HTTP client functionality
- `serde_json` - JSON serialization and deserialization
- `switchy_async` - Async runtime abstraction
- `async-trait` - Async trait support for MusicApi implementation

Optional dependencies:

- `actix-web` - Web server framework (with `api` feature)
- `utoipa` - OpenAPI documentation generation (with `openapi` feature)

## Implementation Status

This package provides a comprehensive framework for YouTube Music integration. However, please note:

- API endpoints are currently stubbed with placeholder URLs
- Actual YouTube Music API integration requires implementation
- The structure and types are complete and ready for integration
- Database models and authentication flow are fully implemented
