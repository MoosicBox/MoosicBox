# MoosicBox Qobuz Integration

High-resolution music streaming integration with Qobuz's lossless and Hi-Res audio service.

## Overview

The MoosicBox Qobuz package provides:

- **Qobuz Hi-Res Support**: Lossless FLAC streaming up to 24-bit/192kHz
- **Complete Catalog Access**: Browse, search, and stream Qobuz's high-quality music library
- **Editorial Content**: Access to Qobuz's curated playlists and magazine content
- **User Library Management**: Sync favorites, purchases, and playlists
- **Seamless Integration**: Direct integration with MoosicBox audio pipeline
- **Multiple Quality Tiers**: Support for MP3, CD, and Hi-Res quality levels

## Features

### Audio Quality
- **MP3 320**: High-quality lossy streaming (320 kbps)
- **CD Quality**: Lossless FLAC (16-bit/44.1kHz)
- **Hi-Res 24/96**: Studio master quality (24-bit/96kHz)
- **Hi-Res 24/192**: Ultra high-resolution (24-bit/192kHz)
- **Adaptive Streaming**: Automatic quality selection based on connection

### Content Access
- **Complete Catalog**: Access to Qobuz's extensive high-quality music library
- **Editorial Content**: Qobuz Magazine articles and reviews
- **Curated Playlists**: Expert-curated playlists by genre and mood
- **New Releases**: Latest albums in high-resolution
- **Exclusive Content**: Qobuz-exclusive releases and remasters

### User Features
- **Personal Library**: Sync purchased albums and favorites
- **Playlist Management**: Create and manage playlists
- **Purchase History**: Access to purchased high-resolution albums
- **Cross-Device Sync**: Sync library across multiple devices
- **Offline Caching**: Cache tracks for offline playback

## Usage

### Basic Setup

```rust
use moosicbox_qobuz::{QobuzClient, QobuzConfig, QobuzQuality};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure Qobuz client
    let config = QobuzConfig {
        app_id: "your_app_id".to_string(),
        app_secret: "your_app_secret".to_string(),
        quality: QobuzQuality::HiRes24, // Hi-Res quality
        country: "US".to_string(),
        cache_dir: Some("./qobuz_cache".into()),
    };

    // Create Qobuz client
    let mut client = QobuzClient::new(config).await?;

    // Login with credentials
    client.login("username", "password").await?;

    Ok(())
}
```

### Authentication

```rust
use moosicbox_qobuz::{QobuzClient, QobuzAuth};

async fn authenticate_qobuz() -> Result<QobuzClient, Box<dyn std::error::Error>> {
    let mut client = QobuzClient::new(config).await?;

    // Username/Password authentication
    client.login("username", "password").await?;

    // Check authentication status
    let user_info = client.get_user_info().await?;
    println!("Logged in as: {} ({})", user_info.display_name, user_info.email);
    println!("Subscription: {:?}", user_info.subscription);
    println!("Hi-Res available: {}", user_info.hires_available);

    Ok(client)
}
```

### Search and Browse

```rust
use moosicbox_qobuz::{QobuzClient, QobuzSearchType};

async fn search_music(client: &QobuzClient) -> Result<(), Box<dyn std::error::Error>> {
    // Search for albums
    let album_results = client.search("Kind of Blue", QobuzSearchType::Albums, 20).await?;

    for album in album_results.albums {
        println!("Album: {} - {} ({})", album.title, album.artist.name, album.release_date);
        println!("  Quality: {} ({}kHz/{}bit)",
                 album.maximum_bit_depth, album.maximum_sampling_rate, album.maximum_bit_depth);
        println!("  Tracks: {}", album.tracks_count);
        println!("  Price: ${}", album.price.display_value);
    }

    // Search for tracks
    let track_results = client.search("So What", QobuzSearchType::Tracks, 50).await?;

    for track in track_results.tracks {
        println!("Track: {} - {} ({})", track.title, track.performer.name, track.album.title);
        println!("  Quality: {}kHz/{}bit", track.maximum_sampling_rate, track.maximum_bit_depth);
        println!("  Duration: {}:{:02}", track.duration / 60, track.duration % 60);
    }

    // Search for artists
    let artist_results = client.search("Miles Davis", QobuzSearchType::Artists, 10).await?;

    for artist in artist_results.artists {
        println!("Artist: {}", artist.name);
        println!("  Albums: {}", artist.albums_count);
        println!("  Image: {}", artist.image.medium);
    }

    Ok(())
}
```

### Track Streaming

```rust
use moosicbox_qobuz::{QobuzClient, QobuzTrack};
use moosicbox_audio_output::AudioOutput;

async fn stream_track(
    client: &QobuzClient,
    track_id: u32,
    audio_output: &mut AudioOutput
) -> Result<(), Box<dyn std::error::Error>> {
    // Get track info
    let track = client.get_track(track_id).await?;
    println!("Streaming: {} - {} ({})",
             track.title, track.performer.name, track.album.title);
    println!("Quality: {}kHz/{}bit",
             track.maximum_sampling_rate, track.maximum_bit_depth);

    // Get streaming URL
    let stream_url = client.get_stream_url(track_id, QobuzQuality::HiRes24).await?;

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

### Album and Artist Information

```rust
use moosicbox_qobuz::QobuzClient;

async fn get_album_info(client: &QobuzClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get album information
    let album_id = "0060253764544";
    let album = client.get_album(album_id).await?;

    println!("Album: {} - {}", album.title, album.artist.name);
    println!("Released: {}", album.release_date_original);
    println!("Quality: {}kHz/{}bit", album.maximum_sampling_rate, album.maximum_bit_depth);
    println!("Duration: {} minutes", album.duration / 60);
    println!("Genre: {}", album.genre.name);
    println!("Label: {}", album.label.name);

    // Get album tracks
    for (i, track) in album.tracks.items.iter().enumerate() {
        println!("{}. {} - {} ({}:{:02})",
                 track.track_number, track.title, track.performer.name,
                 track.duration / 60, track.duration % 60);
    }

    Ok(())
}

async fn get_artist_info(client: &QobuzClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get artist information
    let artist_id = 23242;
    let artist = client.get_artist(artist_id).await?;

    println!("Artist: {}", artist.name);
    println!("Albums: {}", artist.albums_count);
    println!("Biography: {}", artist.biography.summary);

    // Get artist albums
    let albums = client.get_artist_albums(artist_id, None).await?;

    for album in albums.albums.items {
        println!("  Album: {} ({})", album.title, album.release_date_original);
        println!("    Quality: {}kHz/{}bit",
                 album.maximum_sampling_rate, album.maximum_bit_depth);
    }

    Ok(())
}
```

### User Library and Favorites

```rust
use moosicbox_qobuz::{QobuzClient, QobuzFavoriteType};

async fn manage_user_library(client: &QobuzClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get user favorites
    let favorite_albums = client.get_user_favorites(QobuzFavoriteType::Albums).await?;
    println!("Favorite albums: {}", favorite_albums.albums.items.len());

    let favorite_tracks = client.get_user_favorites(QobuzFavoriteType::Tracks).await?;
    println!("Favorite tracks: {}", favorite_tracks.tracks.items.len());

    let favorite_artists = client.get_user_favorites(QobuzFavoriteType::Artists).await?;
    println!("Favorite artists: {}", favorite_artists.artists.items.len());

    // Get purchased albums
    let purchases = client.get_user_purchases().await?;
    println!("Purchased albums: {}", purchases.albums.items.len());

    for album in purchases.albums.items {
        println!("  Purchased: {} - {} ({}kHz/{}bit)",
                 album.title, album.artist.name,
                 album.maximum_sampling_rate, album.maximum_bit_depth);
    }

    // Add album to favorites
    let album_id = "0060253764544";
    client.add_album_to_favorites(album_id).await?;

    // Remove from favorites
    client.remove_album_from_favorites(album_id).await?;

    Ok(())
}
```

### Playlist Management

```rust
use moosicbox_qobuz::{QobuzClient, QobuzPlaylist, QobuzPlaylistRequest};

async fn manage_playlists(client: &QobuzClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get user playlists
    let playlists = client.get_user_playlists().await?;

    for playlist in playlists.playlists.items {
        println!("Playlist: {} ({} tracks)", playlist.name, playlist.tracks_count);
        println!("  Duration: {} minutes", playlist.duration / 60);
        println!("  Public: {}", playlist.is_public);
    }

    // Create new playlist
    let new_playlist = QobuzPlaylistRequest {
        name: "My Hi-Res Collection".to_string(),
        description: Some("High-resolution favorites".to_string()),
        is_public: false,
        is_collaborative: false,
    };

    let playlist = client.create_playlist(new_playlist).await?;
    println!("Created playlist: {}", playlist.name);

    // Add tracks to playlist
    let track_ids = vec![12345678, 87654321];
    client.add_tracks_to_playlist(playlist.id, track_ids).await?;

    // Get playlist tracks
    let tracks = client.get_playlist_tracks(playlist.id).await?;

    for track in tracks.tracks.items {
        println!("  Track: {} - {} ({}kHz/{}bit)",
                 track.title, track.performer.name,
                 track.maximum_sampling_rate, track.maximum_bit_depth);
    }

    Ok(())
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `QOBUZ_APP_ID` | Qobuz API application ID | Required |
| `QOBUZ_APP_SECRET` | Qobuz API application secret | Required |
| `QOBUZ_QUALITY` | Default audio quality | `HiRes24` |
| `QOBUZ_COUNTRY` | Country code for content | `US` |
| `QOBUZ_CACHE_DIR` | Directory for caching | `./cache/qobuz` |
| `QOBUZ_MAX_CONCURRENT_STREAMS` | Max concurrent streams | `3` |

### Quality Settings

```rust
use moosicbox_qobuz::QobuzQuality;

// Audio quality options
let quality = QobuzQuality::Mp3;      // 320 kbps MP3
let quality = QobuzQuality::Cd;       // 16-bit/44.1kHz FLAC
let quality = QobuzQuality::HiRes24;  // 24-bit/96kHz FLAC
let quality = QobuzQuality::HiRes192; // 24-bit/192kHz FLAC (when available)
```

### Advanced Configuration

```rust
use moosicbox_qobuz::{QobuzConfig, QobuzCacheConfig};

let config = QobuzConfig {
    app_id: "your_app_id".to_string(),
    app_secret: "your_app_secret".to_string(),
    quality: QobuzQuality::HiRes24,
    country: "US".to_string(),
    cache_dir: Some("./qobuz_cache".into()),
    cache_config: QobuzCacheConfig {
        max_size_gb: 15.0,             // Maximum cache size
        max_track_cache_hours: 48,     // Cache tracks for 48 hours
        enable_metadata_cache: true,   // Cache track metadata
        enable_artwork_cache: true,    // Cache album artwork
        cleanup_interval_hours: 8,     // Clean up cache every 8 hours
        prefer_hires_cache: true,      // Prioritize Hi-Res tracks in cache
    },
    max_concurrent_streams: 3,         // Limit concurrent streams
    request_timeout_seconds: 45,       // Request timeout
    enable_editorial_content: true,    // Enable magazine content
    ..Default::default()
};
```

## Feature Flags

- `qobuz` - Enable Qobuz streaming integration
- `qobuz-hires` - Enable Hi-Res quality streaming
- `qobuz-cache` - Enable local caching of tracks and metadata
- `qobuz-playlist` - Enable playlist management features
- `qobuz-editorial` - Enable access to Qobuz magazine content
- `qobuz-purchases` - Enable access to purchased content

## Integration with MoosicBox

### Server Integration

```toml
[dependencies]
moosicbox-qobuz = { path = "../qobuz", features = ["qobuz-hires", "qobuz-cache"] }
```

```rust
use moosicbox_qobuz::QobuzClient;
use moosicbox_server::music_api::MusicApi;

// Register Qobuz as a music source
async fn setup_qobuz_integration() -> Result<(), Box<dyn std::error::Error>> {
    let qobuz_client = QobuzClient::new(config).await?;

    // Register with MoosicBox server
    let music_api = MusicApi::new();
    music_api.register_source("qobuz", Box::new(qobuz_client)).await?;

    Ok(())
}
```

### Player Integration

```rust
use moosicbox_qobuz::QobuzClient;
use moosicbox_player::Player;

async fn setup_qobuz_player() -> Result<(), Box<dyn std::error::Error>> {
    let qobuz_client = QobuzClient::new(config).await?;
    let mut player = Player::new().await?;

    // Add Qobuz as a source
    player.add_source("qobuz", Box::new(qobuz_client)).await?;

    // Play a Qobuz track
    player.play_track("qobuz:track:12345678").await?;

    Ok(())
}
```

## Error Handling

```rust
use moosicbox_qobuz::error::QobuzError;

match client.get_track(track_id).await {
    Ok(track) => println!("Track: {}", track.title),
    Err(QobuzError::AuthenticationFailed) => {
        eprintln!("Qobuz authentication failed - check credentials");
    },
    Err(QobuzError::TrackNotFound(id)) => {
        eprintln!("Track not found: {}", id);
    },
    Err(QobuzError::QualityNotAvailable { requested, available }) => {
        eprintln!("Quality {:?} not available, using {:?}", requested, available);
    },
    Err(QobuzError::SubscriptionRequired { required_tier }) => {
        eprintln!("Subscription required: {:?}", required_tier);
    },
    Err(QobuzError::RegionRestricted { country }) => {
        eprintln!("Content not available in region: {}", country);
    },
    Err(QobuzError::RateLimited { retry_after }) => {
        eprintln!("Rate limited, retry after {} seconds", retry_after);
    },
    Err(e) => {
        eprintln!("Qobuz error: {}", e);
    }
}
```

## Rate Limiting and Best Practices

### API Rate Limits
- **Search**: 60 requests per minute
- **Track Info**: 120 requests per minute
- **Streaming**: 3 concurrent streams per account
- **User Library**: 30 requests per minute

### Best Practices

```rust
use moosicbox_qobuz::{QobuzClient, QobuzRateLimiter};

// Use built-in rate limiting
let config = QobuzConfig {
    rate_limiter: QobuzRateLimiter::new(60, 60), // 60 requests per minute
    ..Default::default()
};

// Batch requests when possible
let track_ids = vec![1, 2, 3, 4, 5];
let tracks = client.get_tracks_batch(track_ids).await?;

// Use caching to reduce API calls
let cached_track = client.get_track_cached(track_id).await?;

// Respect subscription tiers
match client.get_max_quality().await? {
    QobuzQuality::Mp3 => println!("MP3 subscription"),
    QobuzQuality::Cd => println!("CD quality subscription"),
    QobuzQuality::HiRes24 => println!("Hi-Res subscription"),
    QobuzQuality::HiRes192 => println!("Studio subscription"),
}
```

## Troubleshooting

### Common Issues

1. **Authentication failures**: Verify app credentials and user subscription
2. **Quality not available**: Check subscription tier and track availability
3. **Regional restrictions**: Some content may not be available in all regions
4. **Rate limiting**: Implement proper rate limiting and retry logic

### Debug Information

```bash
# Enable Qobuz debugging
RUST_LOG=moosicbox_qobuz=debug cargo run

# Test Qobuz connection
cargo run --bin qobuz-test -- --test-connection

# Check subscription status
cargo run --bin qobuz-test -- --check-subscription
```

### Quality and Subscription Issues

```rust
// Check subscription capabilities
match client.get_subscription_info().await {
    Ok(info) => {
        println!("Subscription: {:?}", info.offer);
        println!("Max quality: {:?}", info.max_quality);
        println!("Hi-Res purchases: {}", info.can_purchase_hires);
        println!("Streaming limit: {}", info.streaming_limit);
    },
    Err(e) => eprintln!("Failed to get subscription info: {}", e),
}

// Get available qualities for a track
let qualities = client.get_available_qualities(track_id).await?;
for quality in qualities {
    println!("Available: {:?} ({}kHz/{}bit)",
             quality.format, quality.sampling_rate, quality.bit_depth);
}
```

## Premium Features

### Hi-Res Audio
- **Studio Masters**: Original studio master recordings
- **Multiple Resolutions**: 24-bit/96kHz and 24-bit/192kHz options
- **Lossless Streaming**: FLAC format for pristine quality
- **Download Purchase**: Buy and own Hi-Res tracks

### Editorial Content
- **Qobuz Magazine**: Access to music journalism and reviews
- **Expert Curation**: Playlists curated by music experts
- **Artist Interviews**: Exclusive interviews and features
- **Album Reviews**: Professional album reviews and ratings

## See Also

- [MoosicBox Player](../player/README.md) - Audio playback engine
- [MoosicBox Server](../server/README.md) - Main server with Qobuz integration
- [MoosicBox Tidal](../tidal/README.md) - Alternative high-quality streaming service
- [MoosicBox Audio Output](../audio_output/README.md) - Audio output handling
