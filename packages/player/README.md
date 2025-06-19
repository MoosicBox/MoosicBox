# MoosicBox Player

A high-performance audio playback engine with support for multiple audio formats, gapless playback, and multi-zone audio distribution.

## Overview

The MoosicBox Player is the core audio playback component that provides:

- **Multi-Format Playback**: Support for FLAC, AAC, MP3, Opus, and more
- **Gapless Playback**: Seamless transitions between tracks
- **Multi-Zone Audio**: Distribute audio to multiple output devices
- **Session Management**: Handle multiple concurrent playback sessions
- **Quality Control**: Dynamic quality adjustment and sample rate conversion
- **Remote Playback**: Control playback across network-connected devices
- **Audio Processing**: Real-time audio effects and processing

## Features

### Audio Format Support
- **FLAC** - Lossless high-quality audio
- **AAC/M4A** - Efficient lossy compression
- **MP3** - Universal compatibility
- **Opus** - Modern low-latency codec
- **WAV** - Uncompressed audio
- **OGG** - Open-source audio format

### Playback Features
- **Gapless Playback** - No silence between tracks
- **Crossfade** - Smooth transitions with overlap
- **Seek Support** - Precise position control
- **Volume Control** - Per-session and global volume
- **Replay Gain** - Automatic volume normalization
- **Audio Visualization** - Real-time spectrum analysis

### Multi-Zone Audio
- **Zone Management** - Create and manage audio zones
- **Synchronized Playback** - Play same audio across multiple zones
- **Individual Control** - Independent playback control per zone
- **Group Operations** - Control multiple zones together

### Session Management
- **Multiple Sessions** - Support concurrent playback sessions
- **Session Persistence** - Maintain state across restarts
- **Remote Sessions** - Control sessions across network
- **Playback Queue** - Manage upcoming tracks

## Usage

### Basic Playback

```rust
use moosicbox_player::{Player, PlayerConfig};
use moosicbox_music_models::TrackApiSource;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create player with default configuration
    let config = PlayerConfig::default();
    let player = Player::new(config).await?;

    // Play a track
    let track_id = 123;
    player.play_track(track_id, TrackApiSource::Local).await?;

    // Control playback
    player.pause().await?;
    player.resume().await?;
    player.seek(30.0).await?; // Seek to 30 seconds
    player.set_volume(0.8).await?; // Set volume to 80%

    Ok(())
}
```

### Multi-Zone Audio Setup

```rust
use moosicbox_player::{Player, AudioZone};

async fn setup_multi_zone() -> Result<(), Box<dyn std::error::Error>> {
    let player = Player::new(PlayerConfig::default()).await?;

    // Create audio zones
    let living_room = AudioZone::new("Living Room").await?;
    let kitchen = AudioZone::new("Kitchen").await?;
    let bedroom = AudioZone::new("Bedroom").await?;

    // Add zones to player
    player.add_zone(living_room).await?;
    player.add_zone(kitchen).await?;
    player.add_zone(bedroom).await?;

    // Play synchronized across multiple zones
    let zones = vec!["Living Room", "Kitchen"];
    player.play_to_zones(track_id, &zones).await?;

    Ok(())
}
```

### Session Management

```rust
use moosicbox_player::{Player, PlaybackSession};

async fn manage_sessions() -> Result<(), Box<dyn std::error::Error>> {
    let player = Player::new(PlayerConfig::default()).await?;

    // Create a new playback session
    let session = player.create_session("user-123").await?;

    // Add tracks to session queue
    let track_ids = vec![123, 124, 125];
    session.add_to_queue(&track_ids).await?;

    // Start playback
    session.play().await?;

    // Control session playback
    session.next_track().await?;
    session.previous_track().await?;
    session.set_repeat_mode(RepeatMode::All).await?;
    session.set_shuffle(true).await?;

    Ok(())
}
```

### Advanced Audio Configuration

```rust
use moosicbox_player::{PlayerConfig, AudioOutputConfig, AudioProcessing};

fn create_advanced_player() -> Result<Player, Box<dyn std::error::Error>> {
    let audio_config = AudioOutputConfig {
        sample_rate: 48000,
        bit_depth: 24,
        channels: 2,
        buffer_size: 4096,
        latency: AudioLatency::Low,
    };

    let processing = AudioProcessing {
        enable_replay_gain: true,
        enable_crossfade: true,
        crossfade_duration: Duration::from_secs(3),
        enable_gapless: true,
        enable_resampling: true,
    };

    let config = PlayerConfig {
        audio_output: audio_config,
        audio_processing: processing,
        max_concurrent_streams: 4,
        buffer_ahead_seconds: 30.0,
        ..Default::default()
    };

    Player::new(config)
}
```

### Remote Playback Control

```rust
use moosicbox_player::{Player, RemotePlayer};

async fn control_remote_player() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to remote player
    let remote_player = RemotePlayer::connect("http://192.168.1.100:8001").await?;

    // Control remote playback
    remote_player.play_track(track_id, TrackApiSource::Local).await?;
    remote_player.set_volume(0.7).await?;
    remote_player.seek(45.0).await?;

    // Get playback status
    let status = remote_player.get_status().await?;
    println!("Playing: {}", status.current_track.title);
    println!("Position: {:.2}s", status.position_seconds);

    Ok(())
}
```

## Configuration

### Player Configuration

```rust
use moosicbox_player::PlayerConfig;

let config = PlayerConfig {
    // Audio output settings
    sample_rate: 44100,
    bit_depth: 16,
    channels: 2,
    buffer_size: 2048,

    // Playback settings
    enable_gapless: true,
    enable_crossfade: true,
    crossfade_duration_ms: 3000,

    // Quality settings
    max_quality: AudioQuality::Lossless,
    prefer_lossless: true,

    // Performance settings
    max_concurrent_streams: 2,
    buffer_ahead_seconds: 15.0,
    preload_next_track: true,

    // Network settings
    connection_timeout_ms: 5000,
    read_timeout_ms: 10000,
};
```

### Audio Output Backends

The player supports multiple audio output backends:

```rust
// CPAL (Cross-platform)
let config = PlayerConfig {
    audio_backend: AudioBackend::Cpal,
    ..Default::default()
};

// PulseAudio (Linux)
let config = PlayerConfig {
    audio_backend: AudioBackend::PulseAudio,
    ..Default::default()
};

// JACK (Professional audio)
let config = PlayerConfig {
    audio_backend: AudioBackend::Jack,
    ..Default::default()
};

// ASIO (Windows, low-latency)
let config = PlayerConfig {
    audio_backend: AudioBackend::Asio,
    ..Default::default()
};
```

## API Integration

### RESTful API

The player can be controlled via HTTP API when used with MoosicBox Server:

```bash
# Start playback
curl -X POST "http://localhost:8001/player/play" \
  -H "Content-Type: application/json" \
  -d '{"track_id": 123, "source": "LOCAL"}'

# Control playback
curl -X POST "http://localhost:8001/player/pause"
curl -X POST "http://localhost:8001/player/resume"
curl -X POST "http://localhost:8001/player/next"
curl -X POST "http://localhost:8001/player/previous"

# Set volume (0.0 to 1.0)
curl -X POST "http://localhost:8001/player/volume" \
  -H "Content-Type: application/json" \
  -d '{"volume": 0.8}'

# Seek to position (seconds)
curl -X POST "http://localhost:8001/player/seek" \
  -H "Content-Type: application/json" \
  -d '{"position": 45.5}'

# Get playback status
curl "http://localhost:8001/player/status"
```

### WebSocket Events

Real-time playback events via WebSocket:

```javascript
const ws = new WebSocket('ws://localhost:8001/ws');

ws.onmessage = (event) => {
    const message = JSON.parse(event.data);

    switch (message.type) {
        case 'PLAYBACK_STATE_CHANGED':
            console.log('Playback state:', message.state);
            break;
        case 'TRACK_CHANGED':
            console.log('Now playing:', message.track);
            break;
        case 'POSITION_CHANGED':
            console.log('Position:', message.position);
            break;
        case 'VOLUME_CHANGED':
            console.log('Volume:', message.volume);
            break;
    }
};
```

## Audio Processing

### Crossfade Implementation

```rust
use moosicbox_player::audio::CrossfadeConfig;

let crossfade = CrossfadeConfig {
    duration: Duration::from_secs(4),
    curve: CrossfadeCurve::EqualPower,
    only_between_tracks: false,
    skip_short_tracks: true,
    min_track_length: Duration::from_secs(10),
};

player.set_crossfade_config(crossfade).await?;
```

### Replay Gain Support

```rust
use moosicbox_player::audio::ReplayGainConfig;

let replay_gain = ReplayGainConfig {
    enabled: true,
    mode: ReplayGainMode::Track, // or Album
    preamp_db: 0.0,
    prevent_clipping: true,
    fallback_gain_db: -6.0,
};

player.set_replay_gain_config(replay_gain).await?;
```

### Audio Visualization

```rust
use moosicbox_player::audio::VisualizationConfig;

let visualization = VisualizationConfig {
    enabled: true,
    fft_size: 2048,
    update_rate_hz: 30,
    frequency_bands: 128,
};

player.set_visualization_config(visualization).await?;

// Get spectrum data
let spectrum = player.get_spectrum_data().await?;
for (frequency, amplitude) in spectrum {
    println!("{}Hz: {:.2}dB", frequency, amplitude);
}
```

## Error Handling

```rust
use moosicbox_player::error::PlayerError;

match player.play_track(track_id, source).await {
    Ok(()) => println!("Playback started"),
    Err(PlayerError::TrackNotFound(id)) => {
        eprintln!("Track {} not found", id);
    },
    Err(PlayerError::UnsupportedFormat(format)) => {
        eprintln!("Unsupported audio format: {}", format);
    },
    Err(PlayerError::AudioOutputError(msg)) => {
        eprintln!("Audio output error: {}", msg);
    },
    Err(PlayerError::NetworkError(e)) => {
        eprintln!("Network error: {}", e);
    },
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## Performance Optimization

### Memory Usage
- **Streaming Playback**: Minimal memory footprint for large files
- **Smart Buffering**: Adaptive buffer sizes based on available memory
- **Garbage Collection**: Automatic cleanup of unused resources

### CPU Usage
- **Hardware Acceleration**: Use GPU for audio processing when available
- **Efficient Decoding**: Optimized decoders for each format
- **Thread Pool**: Dedicated threads for audio processing

### Network Optimization
- **Adaptive Bitrate**: Automatically adjust quality based on bandwidth
- **Prefetching**: Download upcoming tracks in background
- **Connection Pooling**: Reuse network connections for efficiency

## Troubleshooting

### Common Issues

1. **No audio output**: Check audio device selection and permissions
2. **Crackling/Distortion**: Adjust buffer size and sample rate
3. **High latency**: Use ASIO (Windows) or JACK for low-latency
4. **Gaps between tracks**: Ensure gapless playback is enabled

### Debug Logging

```bash
# Enable detailed audio logging
RUST_LOG=moosicbox_player=debug cargo run

# Audio output debugging
RUST_LOG=moosicbox_audio_output=debug cargo run

# Network debugging for remote tracks
RUST_LOG=moosicbox_http=debug cargo run
```

## See Also

- [MoosicBox Audio Output](../audio_output/README.md) - Audio output backends
- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio format decoding
- [MoosicBox Files](../files/README.md) - File streaming and format conversion
- [MoosicBox Server](../server/README.md) - HTTP API server
