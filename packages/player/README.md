# MoosicBox Player

A high-performance audio playback engine with support for multiple audio formats and session-based playback management.

## Overview

The MoosicBox Player is the core audio playback component that provides:

- **Multi-Format Playback**: Support for FLAC, AAC, MP3, and Opus
- **Session Management**: Handle concurrent playback sessions with state tracking
- **Quality Control**: Dynamic format conversion and sample rate resampling
- **Local & Remote Playback**: Play files directly or stream from remote sources
- **HTTP API Integration**: Control playback via REST endpoints

## Features

### Audio Format Support

The player supports the following audio formats (when corresponding feature flags are enabled):

- **FLAC** - Lossless high-quality audio (`decoder-flac`, `encoder-flac`)
- **AAC/M4A** - Efficient lossy compression (`decoder-aac`, `encoder-aac`)
- **MP3** - Universal compatibility (`decoder-mp3`, `encoder-mp3`)
- **Opus** - Modern low-latency codec (`decoder-opus`, `encoder-opus`)

### Playback Features

- **Gapless Playback** - Seamless transitions between tracks
- **Seek Support** - Precise position control within tracks
- **Volume Control** - Per-session volume management
- **Pause/Resume** - Full playback state control
- **Queue Management** - Play albums, tracks, or playlists
- **Progress Tracking** - Real-time playback position updates

### Session Management

- **Multiple Sessions** - Support concurrent playback sessions
- **Session State** - Track playing status, position, volume, and quality
- **Playback Targets** - Direct playback to specific audio zones
- **Event System** - Broadcast playback state changes to listeners

## Core Types

### PlaybackHandler

The main interface for controlling playback:

```rust
use moosicbox_player::{PlaybackHandler, Player};

// Create a player implementation (e.g., LocalPlayer)
let handler = PlaybackHandler::new(player);

// Control playback
handler.play_track(session_id, profile, track, seek, volume, quality, playback_target, retry_options).await?;
handler.pause(retry_options).await?;
handler.resume(retry_options).await?;
handler.seek(position, retry_options).await?;
handler.next_track(seek, retry_options).await?;
handler.previous_track(seek, retry_options).await?;
handler.stop(retry_options).await?;
```

### Player Trait

Implement this trait to create custom player backends:

```rust
#[async_trait]
pub trait Player: std::fmt::Debug + Send {
    // Optional hook: called before play event processing
    // Use this to execute custom logic before playback starts (e.g., state preparation, logging)
    async fn before_play_playback(&self, seek: Option<f64>) -> Result<(), PlayerError> {
        Ok(())
    }
    async fn trigger_play(&self, seek: Option<f64>) -> Result<(), PlayerError>;
    async fn trigger_stop(&self) -> Result<(), PlayerError>;
    async fn trigger_seek(&self, seek: f64) -> Result<(), PlayerError>;
    // Optional hook: called before playback state update
    // Use this to execute custom logic before state changes (e.g., validation, resource preparation)
    async fn before_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }
    // Optional hook: called after playback state update
    // Use this to execute custom logic after state changes (e.g., notifications, cleanup)
    async fn after_update_playback(&self) -> Result<(), PlayerError> {
        Ok(())
    }
    async fn trigger_pause(&self) -> Result<(), PlayerError>;
    async fn trigger_resume(&self) -> Result<(), PlayerError>;
    fn player_status(&self) -> Result<ApiPlaybackStatus, PlayerError>;
    fn get_source(&self) -> &PlayerSource;
}
```

### Playback

The state object for active playback:

```rust
pub struct Playback {
    pub id: u64,
    pub session_id: u64,
    pub profile: String,
    pub tracks: Vec<Track>,
    pub playing: bool,
    pub position: u16,
    pub quality: PlaybackQuality,
    pub progress: f64,
    pub volume: Arc<AtomicF64>,
    pub playback_target: Option<PlaybackTarget>,
    pub abort: CancellationToken,
}
```

## Usage

### Local Player

The `LocalPlayer` (available with the `local` feature) plays audio directly using the audio output backend:

```rust
use moosicbox_player::{PlayerSource, PlaybackHandler};
use moosicbox_player::local::LocalPlayer;
use moosicbox_audio_output::default_output_factory;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a local player
    let local_player = LocalPlayer::new(
        PlayerSource::Local,
        None, // playback type (defaults to Default)
    ).await?;

    // Attach an audio output
    let local_player = local_player.with_output(
        default_output_factory().await
            .ok_or("Missing default audio output")?
    );

    // Create a playback handler
    let playback = local_player.playback.clone();
    let output = local_player.output.clone();
    let mut handler = PlaybackHandler::new(local_player)
        .with_playback(playback)
        .with_output(output);

    // Play a track
    handler.play_track(
        session_id,
        profile,
        track,
        None, // seek
        Some(0.8), // volume
        PlaybackQuality::default(),
        None, // playback_target
        None, // retry_options
    ).await?;

    Ok(())
}
```

### Playing Tracks

```rust
use moosicbox_player::PlaybackHandler;
use moosicbox_music_models::{Track, AudioFormat, PlaybackQuality};

async fn play_example(
    handler: &mut PlaybackHandler,
    track: Track,
    session_id: u64,
    profile: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Play a single track
    handler.play_track(
        session_id,
        profile.clone(),
        track,
        None,
        Some(1.0),
        PlaybackQuality { format: AudioFormat::Source },
        None,
        None,
    ).await?;

    Ok(())
}
```

### Playing Albums

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::id::Id;

async fn play_album_example(
    handler: &mut PlaybackHandler,
    api: &dyn MusicApi,
    album_id: &Id,
    session_id: u64,
    profile: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Play an entire album
    handler.play_album(
        api,
        session_id,
        profile,
        album_id,
        None, // position
        None, // seek
        Some(1.0), // volume
        PlaybackQuality::default(),
        None, // playback_target
        None, // retry_options
    ).await?;

    Ok(())
}
```

### Controlling Playback

```rust
use moosicbox_player::DEFAULT_PLAYBACK_RETRY_OPTIONS;

async fn control_playback(
    handler: &mut PlaybackHandler,
) -> Result<(), Box<dyn std::error::Error>> {
    // Pause playback
    handler.pause(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    // Resume playback
    handler.resume(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    // Seek to 30 seconds
    handler.seek(30.0, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    // Skip to next track
    handler.next_track(None, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    // Go to previous track
    handler.previous_track(None, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    // Stop playback
    handler.stop(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    Ok(())
}
```

### Updating Playback State

```rust
async fn update_playback(
    handler: &mut PlaybackHandler,
    session_id: u64,
    profile: String,
) -> Result<(), Box<dyn std::error::Error>> {
    handler.update_playback(
        true,              // modify_playback
        None,              // play
        None,              // stop
        Some(true),        // playing
        Some(2),           // position (track index)
        Some(15.0),        // seek
        Some(0.7),         // volume
        None,              // tracks
        None,              // quality
        Some(session_id),
        Some(profile),
        None,              // playback_target
        true,              // trigger_event
        None,              // retry_options
    ).await?;

    Ok(())
}
```

### Event Handling

Listen for playback state changes:

```rust
use moosicbox_player::{on_playback_event, Playback};
use moosicbox_session::models::UpdateSession;

fn my_event_handler(update: &UpdateSession, playback: &Playback) {
    if let Some(playing) = update.playing {
        println!("Playback state changed: {}", playing);
    }
    if let Some(position) = update.position {
        println!("Track position changed: {}", position);
    }
    if let Some(seek) = update.seek {
        println!("Seek position: {:.2}s", seek);
    }
    if let Some(volume) = update.volume {
        println!("Volume changed: {:.2}", volume);
    }
}

// Register event listener
on_playback_event(my_event_handler);
```

## HTTP API Integration

When used with the MoosicBox Server, the player provides REST endpoints for playback control.

### Play Track

```bash
POST /player/play/track?sessionId=1&trackId=123&volume=0.8
```

### Play Album

```bash
POST /player/play/album?sessionId=1&albumId=456&position=0
```

### Play Multiple Tracks

```bash
POST /player/play/tracks?sessionId=1&trackIds=123,124,125&position=0
```

### Pause/Resume

```bash
POST /player/pause
POST /player/resume
```

### Seek

```bash
POST /player/seek?seek=45.5
```

### Next/Previous Track

```bash
POST /player/next-track
POST /player/previous-track
```

### Stop

```bash
POST /player/stop
```

### Get Status

```bash
GET /player/status
```

### Update Playback

```bash
POST /player/update-playback?playing=true&position=2&volume=0.7
```

## Configuration

### Feature Flags

The player supports various feature flags for customization:

**Audio Output Backends:**

- `cpal` - Cross-platform audio output (default)
- `jack` - JACK audio server support
- `asio` - ASIO low-latency support (Windows)

**Audio Decoders:**

- `decoder-aac` - AAC audio decoding
- `decoder-flac` - FLAC audio decoding
- `decoder-mp3` - MP3 audio decoding
- `decoder-opus` - Opus audio decoding
- `all-decoders` - Enable all decoders

**Audio Encoders:**

- `encoder-aac` - AAC audio encoding
- `encoder-flac` - FLAC audio encoding
- `encoder-mp3` - MP3 audio encoding
- `encoder-opus` - Opus audio encoding
- `all-encoders` - Enable all encoders

**Other Features:**

- `api` - Enable HTTP API endpoints
- `openapi` - Generate OpenAPI documentation
- `local` - Enable local player implementation
- `profiling` - Enable performance profiling

### PlayerSource

Configure where audio is sourced from:

```rust
use moosicbox_player::PlayerSource;

// Play local files
let source = PlayerSource::Local;

// Stream from remote server
let source = PlayerSource::Remote {
    host: "http://localhost:8001".to_string(),
    query: None,
    headers: None,
};
```

### PlaybackQuality

Specify output audio format:

```rust
use moosicbox_music_models::{PlaybackQuality, AudioFormat};

let quality = PlaybackQuality {
    format: AudioFormat::Source,  // Use source format
};

let quality = PlaybackQuality {
    format: AudioFormat::Flac,    // Convert to FLAC
};
```

### PlaybackRetryOptions

Configure retry behavior for operations:

```rust
use moosicbox_player::PlaybackRetryOptions;
use std::time::Duration;

let retry_options = PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: Duration::from_millis(500),
};
```

## Error Handling

```rust
use moosicbox_player::PlayerError;

match handler.play_track(/* ... */).await {
    Ok(()) => println!("Playback started"),
    Err(PlayerError::TrackNotFound(id)) => {
        eprintln!("Track {} not found", id);
    },
    Err(PlayerError::UnsupportedFormat(format)) => {
        eprintln!("Unsupported audio format: {:?}", format);
    },
    Err(PlayerError::NoPlayersPlaying) => {
        eprintln!("No active playback session");
    },
    Err(PlayerError::PositionOutOfBounds(pos)) => {
        eprintln!("Position {} is out of bounds", pos);
    },
    Err(e) => {
        eprintln!("Playback error: {:?}", e);
    }
}
```

## Architecture

The player is built on several key components:

- **PlaybackHandler** - High-level playback control interface
- **Player trait** - Pluggable player backend system
- **LocalPlayer** - Direct audio output implementation
- **Symphonia integration** - Audio decoding via symphonia
- **Signal chain** - Audio processing pipeline for format conversion
- **Audio output** - Hardware audio output via `moosicbox_audio_output`

## Dependencies

Key dependencies from Cargo.toml:

- `moosicbox_audio_decoder` - Audio format decoding
- `moosicbox_audio_output` - Audio output backends
- `moosicbox_music_api` - Music metadata and track fetching
- `moosicbox_music_models` - Track and album data models
- `moosicbox_session` - Session and playlist management
- `moosicbox_resampler` - Sample rate conversion
- `symphonia` - Audio codec support
- `actix-web` - HTTP API endpoints (with `api` feature)

## See Also

- [MoosicBox Audio Output](../audio_output/README.md) - Audio output backends
- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio format decoding
- [MoosicBox Session](../session/README.md) - Session management
- [MoosicBox Music API](../music_api/README.md) - Music metadata API
