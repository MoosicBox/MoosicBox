# MoosicBox YouTube Music

YouTube Music API integration providing streaming access and library management for YouTube Music content within the MoosicBox ecosystem.

## Features

- **YouTube Music API**: Complete integration with YouTube Music streaming service
- **Authentication**: OAuth2 device flow authentication for user accounts
- **Library Access**: Manage favorite artists, albums, and tracks
- **Search Functionality**: Search across YouTube Music's catalog
- **Audio Streaming**: High-quality audio streaming with multiple quality options
- **Playlist Management**: Access and manage YouTube Music playlists
- **Database Integration**: Optional local caching and configuration storage
- **Artist/Album Browsing**: Browse artist catalogs and album collections
- **Track Information**: Detailed track metadata and playback information

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_yt = "0.1.1"

# Enable features as needed
moosicbox_yt = { version = "0.1.1", features = ["db", "api"] }
```

## Usage

### Setting Up the API

```rust
use moosicbox_yt::{YtMusicApi, YtMusicApiBuilder};
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "db")]
    let db = LibraryDatabase::new().await?;

    let yt_api = YtMusicApiBuilder::new()
        .with_db(db)
        .build()
        .await?;

    println!("YouTube Music API ready");
    Ok(())
}
```

### Authentication Flow

```rust
use moosicbox_yt::{device_authorization, device_authorization_token};

async fn authenticate_user() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = "your-client-id".to_string();
    let client_secret = "your-client-secret".to_string();

    // Start device authorization
    let auth_response = device_authorization(client_id.clone(), true).await?;

    println!("Visit: {}", auth_response["verification_url"]);
    println!("Enter code: {}", auth_response["user_code"]);

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
        println!("  {} (ID: {})", artist.title, artist.id);
        if let Some(description) = &artist.description {
            println!("    {}", description);
        }
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
        println!("  Tracks: {}", album_detail.track_count.unwrap_or(0));

        // Get album tracks
        let tracks_result = album_tracks(
            &db, &album_id, Some(0), Some(5), None, None, None, None
        ).await?;

        println!("  First 5 tracks:");
        for track in tracks_result.page.items() {
            println!("    {}: {}", track.number, track.title);
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

    if let Some(artists) = &results.artists {
        println!("  Artists:");
        for artist in artists {
            println!("    {} ({})", artist.title, artist.id);
        }
    }

    if let Some(albums) = &results.albums {
        println!("  Albums:");
        for album in albums {
            println!("    {} - {}", album.artist, album.title);
        }
    }

    if let Some(tracks) = &results.tracks {
        println!("  Tracks:");
        for track in tracks {
            println!("    {} - {}", track.artist, track.title);
        }
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
                 album.date_released.map(|d| d.format("%Y").to_string())
                      .unwrap_or_default());
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

- `db` - Enable database integration for token storage and caching
- `api` - Enable API endpoint functionality

## Authentication

The package supports OAuth2 device flow authentication:

1. Call `device_authorization()` to get verification URL and user code
2. User visits URL and enters code
3. Call `device_authorization_token()` to complete flow
4. Tokens are automatically managed (with `db` feature enabled)

## Dependencies

- `moosicbox_music_api` - Generic music API trait
- `moosicbox_music_models` - Common music data models
- `switchy_database` - Database integration (optional)
- `switchy_http` - HTTP client functionality
- `serde_json` - JSON serialization
- `tokio` - Async runtime support
