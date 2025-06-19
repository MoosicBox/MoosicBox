# MoosicBox Tidal Integration

High-quality music streaming integration with Tidal's lossless audio service.

## Overview

The MoosicBox Tidal package provides:

- **Tidal HiFi Support**: Lossless FLAC streaming up to 24-bit/96kHz
- **Tidal Masters**: Support for MQA (Master Quality Authenticated) tracks
- **Complete Catalog Access**: Browse, search, and stream Tidal's music library
- **Playlist Management**: Create, modify, and sync Tidal playlists
- **User Library**: Access personal favorites, albums, and followed artists
- **Real-Time Streaming**: Direct integration with MoosicBox audio pipeline

## Features

### Audio Quality
- **Tidal HiFi**: CD-quality lossless FLAC (16-bit/44.1kHz)
- **Tidal HiFi Plus**: Hi-Res lossless up to 24-bit/96kHz
- **Tidal Masters**: MQA tracks with enhanced audio quality
- **Adaptive Streaming**: Automatic quality adjustment based on connection

### Content Access
- **Full Catalog**: Access to Tidal's complete music library
- **Curated Playlists**: Tidal's editorial and algorithmic playlists
- **Artist Radio**: Discover similar artists and tracks
- **New Releases**: Latest albums and singles
- **Exclusive Content**: Tidal-exclusive releases and content

### User Features
- **Personal Library**: Sync favorites, albums, and playlists
- **Offline Caching**: Cache tracks for offline playback
- **Cross-Device Sync**: Sync listening across multiple devices
- **Listening History**: Track and resume playback history

## Usage

### Basic Setup

```rust
use moosicbox_tidal::{TidalClient, TidalConfig, TidalQuality};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure Tidal client
    let config = TidalConfig {
        client_id: "your_client_id".to_string(),
        client_secret: "your_client_secret".to_string(),
        quality: TidalQuality::HiFi, // Lossless quality
        country_code: "US".to_string(),
        cache_dir: Some("./tidal_cache".into()),
    };

    // Create Tidal client
    let mut client = TidalClient::new(config).await?;

    // Login with credentials
    client.login("username", "password").await?;

    Ok(())
}
```

### Authentication

```rust
use moosicbox_tidal::{TidalClient, TidalAuth};

async fn authenticate_tidal() -> Result<TidalClient, Box<dyn std::error::Error>> {
    let mut client = TidalClient::new(config).await?;

    // Option 1: Username/Password authentication
    client.login("username", "password").await?;

    // Option 2: OAuth2 authentication
    let auth_url = client.get_auth_url().await?;
    println!("Please visit: {}", auth_url);

    // User authorizes and provides code
    let auth_code = "user_provided_code";
    client.authenticate_with_code(auth_code).await?;

    // Option 3: Token-based authentication
    let access_token = "existing_access_token";
    client.authenticate_with_token(access_token).await?;

    Ok(client)
}
```

### Search and Browse

```rust
use moosicbox_tidal::{TidalClient, TidalSearchType, TidalSearchResults};

async fn search_music(client: &TidalClient) -> Result<(), Box<dyn std::error::Error>> {
    // Search for tracks
    let track_results = client.search("The Dark Side of the Moon", TidalSearchType::Tracks, 50).await?;

    for track in track_results.tracks {
        println!("Track: {} - {} ({})", track.title, track.artist.name, track.album.title);
        println!("  Quality: {:?}", track.audio_quality);
        println!("  Duration: {}s", track.duration);
    }

    // Search for albums
    let album_results = client.search("Pink Floyd", TidalSearchType::Albums, 20).await?;

    for album in album_results.albums {
        println!("Album: {} - {} ({})", album.title, album.artist.name, album.release_date);
        println!("  Tracks: {}", album.number_of_tracks);
        println!("  Quality: {:?}", album.audio_quality);
    }

    // Search for artists
    let artist_results = client.search("Pink Floyd", TidalSearchType::Artists, 10).await?;

    for artist in artist_results.artists {
        println!("Artist: {}", artist.name);
        println!("  Followers: {}", artist.popularity);
    }

    Ok(())
}
```

### Track Streaming

```rust
use moosicbox_tidal::{TidalClient, TidalTrack};
use moosicbox_audio_output::AudioOutput;

async fn stream_track(
    client: &TidalClient,
    track_id: u64,
    audio_output: &mut AudioOutput
) -> Result<(), Box<dyn std::error::Error>> {
    // Get track info
    let track = client.get_track(track_id).await?;
    println!("Streaming: {} - {}", track.title, track.artist.name);

    // Get streaming URL
    let stream_url = client.get_stream_url(track_id, TidalQuality::HiFi).await?;

    // Stream audio data
    let mut audio_stream = client.stream_track(stream_url).await?;

    // Read and play audio chunks
    let mut buffer = vec![0u8; 8192];
    while let Ok(bytes_read) = audio_stream.read(&mut buffer).await {
        if bytes_read == 0 {
            break;
        }

        // Convert to audio samples and play
        let samples = convert_to_samples(&buffer[..bytes_read]);
        audio_output.write_samples(&samples).await?;
    }

    Ok(())
}
```

### Playlist Management

```rust
use moosicbox_tidal::{TidalClient, TidalPlaylist, TidalPlaylistRequest};

async fn manage_playlists(client: &TidalClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get user playlists
    let playlists = client.get_user_playlists().await?;

    for playlist in playlists {
        println!("Playlist: {} ({} tracks)", playlist.title, playlist.number_of_tracks);
    }

    // Create new playlist
    let new_playlist = TidalPlaylistRequest {
        title: "My MoosicBox Playlist".to_string(),
        description: Some("Created via MoosicBox".to_string()),
        public: false,
    };

    let playlist = client.create_playlist(new_playlist).await?;
    println!("Created playlist: {}", playlist.title);

    // Add tracks to playlist
    let track_ids = vec![12345678, 87654321];
    client.add_tracks_to_playlist(playlist.uuid, track_ids).await?;

    // Get playlist tracks
    let tracks = client.get_playlist_tracks(playlist.uuid).await?;

    for track in tracks {
        println!("  Track: {} - {}", track.title, track.artist.name);
    }

    Ok(())
}
```

### User Library

```rust
use moosicbox_tidal::{TidalClient, TidalLibraryType};

async fn access_user_library(client: &TidalClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get favorite tracks
    let favorite_tracks = client.get_user_favorites(TidalLibraryType::Tracks).await?;
    println!("Favorite tracks: {}", favorite_tracks.len());

    // Get favorite albums
    let favorite_albums = client.get_user_favorites(TidalLibraryType::Albums).await?;
    println!("Favorite albums: {}", favorite_albums.len());

    // Get followed artists
    let followed_artists = client.get_user_favorites(TidalLibraryType::Artists).await?;
    println!("Followed artists: {}", followed_artists.len());

    // Add track to favorites
    let track_id = 12345678;
    client.add_to_favorites(track_id, TidalLibraryType::Tracks).await?;

    // Remove from favorites
    client.remove_from_favorites(track_id, TidalLibraryType::Tracks).await?;

    Ok(())
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TIDAL_CLIENT_ID` | Tidal API client ID | Required |
| `TIDAL_CLIENT_SECRET` | Tidal API client secret | Required |
| `TIDAL_QUALITY` | Default audio quality | `HiFi` |
| `TIDAL_COUNTRY_CODE` | Country code for content | `US` |
| `TIDAL_CACHE_DIR` | Directory for caching | `./cache/tidal` |
| `TIDAL_MAX_CONCURRENT_STREAMS` | Max concurrent streams | `5` |

### Quality Settings

```rust
use moosicbox_tidal::TidalQuality;

// Audio quality options
let quality = TidalQuality::Low;       // 96 kbps AAC
let quality = TidalQuality::High;      // 320 kbps AAC
let quality = TidalQuality::HiFi;      // 1411 kbps FLAC (CD quality)
let quality = TidalQuality::Master;    // MQA (up to 24-bit/96kHz)
```

### Caching Configuration

```rust
use moosicbox_tidal::{TidalConfig, TidalCacheConfig};

let config = TidalConfig {
    cache_dir: Some("./tidal_cache".into()),
    cache_config: TidalCacheConfig {
        max_size_gb: 10.0,           // Maximum cache size
        max_track_cache_hours: 24,   // Cache tracks for 24 hours
        enable_metadata_cache: true,  // Cache track metadata
        enable_artwork_cache: true,   // Cache album artwork
        cleanup_interval_hours: 6,    // Clean up cache every 6 hours
    },
    ..Default::default()
};
```

## Feature Flags

- `tidal` - Enable Tidal streaming integration
- `tidal-hifi` - Enable HiFi quality streaming
- `tidal-master` - Enable MQA Master quality streaming
- `tidal-cache` - Enable local caching of tracks and metadata
- `tidal-playlist` - Enable playlist management features
- `tidal-oauth` - Enable OAuth2 authentication flow

## Integration with MoosicBox

### Server Integration

```toml
[dependencies]
moosicbox-tidal = { path = "../tidal", features = ["tidal-hifi", "tidal-cache"] }
```

```rust
use moosicbox_tidal::TidalClient;
use moosicbox_server::music_api::MusicApi;

// Register Tidal as a music source
async fn setup_tidal_integration() -> Result<(), Box<dyn std::error::Error>> {
    let tidal_client = TidalClient::new(config).await?;

    // Register with MoosicBox server
    let music_api = MusicApi::new();
    music_api.register_source("tidal", Box::new(tidal_client)).await?;

    Ok(())
}
```

### Player Integration

```rust
use moosicbox_tidal::TidalClient;
use moosicbox_player::Player;

async fn setup_tidal_player() -> Result<(), Box<dyn std::error::Error>> {
    let tidal_client = TidalClient::new(config).await?;
    let mut player = Player::new().await?;

    // Add Tidal as a source
    player.add_source("tidal", Box::new(tidal_client)).await?;

    // Play a Tidal track
    player.play_track("tidal:track:12345678").await?;

    Ok(())
}
```

## Error Handling

```rust
use moosicbox_tidal::error::TidalError;

match client.get_track(track_id).await {
    Ok(track) => println!("Track: {}", track.title),
    Err(TidalError::AuthenticationFailed) => {
        eprintln!("Tidal authentication failed - check credentials");
    },
    Err(TidalError::TrackNotFound(id)) => {
        eprintln!("Track not found: {}", id);
    },
    Err(TidalError::QualityNotAvailable { requested, available }) => {
        eprintln!("Quality {:?} not available, using {:?}", requested, available);
    },
    Err(TidalError::RateLimited { retry_after }) => {
        eprintln!("Rate limited, retry after {} seconds", retry_after);
    },
    Err(TidalError::NetworkError(e)) => {
        eprintln!("Network error: {}", e);
    },
    Err(e) => {
        eprintln!("Tidal error: {}", e);
    }
}
```

## Rate Limiting and Best Practices

### API Rate Limits
- **Search**: 100 requests per minute
- **Track Info**: 200 requests per minute
- **Streaming**: 10 concurrent streams per account
- **Playlist Operations**: 50 requests per minute

### Best Practices

```rust
use moosicbox_tidal::{TidalClient, TidalRateLimiter};

// Use built-in rate limiting
let config = TidalConfig {
    rate_limiter: TidalRateLimiter::new(100, 60), // 100 requests per 60 seconds
    ..Default::default()
};

// Batch requests when possible
let track_ids = vec![1, 2, 3, 4, 5];
let tracks = client.get_tracks_batch(track_ids).await?;

// Use caching to reduce API calls
let cached_track = client.get_track_cached(track_id).await?;
```

## Troubleshooting

### Common Issues

1. **Authentication failures**: Verify credentials and subscription status
2. **Quality not available**: Check subscription tier and track availability
3. **Regional restrictions**: Some content may not be available in all regions
4. **Rate limiting**: Implement proper rate limiting and retry logic

### Debug Information

```bash
# Enable Tidal debugging
RUST_LOG=moosicbox_tidal=debug cargo run

# Test Tidal connection
cargo run --bin tidal-test -- --test-connection

# Check subscription status
cargo run --bin tidal-test -- --check-subscription
```

### Authentication Issues

```rust
// Check subscription status
match client.get_subscription_info().await {
    Ok(info) => {
        println!("Subscription: {:?}", info.subscription_type);
        println!("Valid until: {:?}", info.valid_until);
        println!("HiFi available: {}", info.hifi_available);
    },
    Err(e) => eprintln!("Failed to get subscription info: {}", e),
}
```

## See Also

- [MoosicBox Player](../player/README.md) - Audio playback engine
- [MoosicBox Server](../server/README.md) - Main server with Tidal integration
- [MoosicBox Qobuz](../qobuz/README.md) - Alternative high-quality streaming service
