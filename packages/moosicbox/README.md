# MoosicBox

The main umbrella package that re-exports all MoosicBox components and provides a unified interface to the entire MoosicBox ecosystem.

## Overview

The `moosicbox` package serves as the central hub for the MoosicBox music server ecosystem, providing:

- **Unified API**: Single import point for all MoosicBox functionality
- **Component Integration**: Seamless interaction between all modules
- **Dependency Management**: Coordinated versioning of all components
- **Feature Flags**: Centralized control over optional functionality
- **Documentation Hub**: Central reference for the entire ecosystem

## Purpose

This package acts as a convenient way to:

1. **Import All Components**: Get access to the entire MoosicBox functionality with a single dependency
2. **Ensure Compatibility**: All included components are tested together and guaranteed to work
3. **Simplify Integration**: Reduced complexity when building applications using MoosicBox
4. **Centralized Configuration**: Unified feature flag management across all components

## Included Components

### Core Components
- **moosicbox_server** - Main music server functionality
- **moosicbox_player** - Audio playback engine
- **moosicbox_library** - Music library management
- **moosicbox_audio_zone** - Multi-zone audio control
- **moosicbox_session** - Session and state management

### API Components
- **moosicbox_music_api** - Music streaming API
- **moosicbox_music_api_api** - RESTful API endpoints
- **moosicbox_music_api_models** - Data models for API
- **moosicbox_music_api_helpers** - API utility functions

### Audio Processing
- **moosicbox_audio_decoder** - Audio format decoding
- **moosicbox_audio_encoder** - Audio format encoding
- **moosicbox_audio_output** - Audio output management
- **moosicbox_resampler** - Audio sample rate conversion

### Streaming Sources
- **moosicbox_qobuz** - Qobuz streaming integration (optional)
- **moosicbox_tidal** - Tidal streaming integration (optional)
- **moosicbox_yt** - YouTube Music integration (optional)

### Utilities
- **moosicbox_files** - File handling and streaming
- **moosicbox_image** - Image processing and optimization
- **moosicbox_downloader** - Download management
- **moosicbox_search** - Search functionality
- **moosicbox_scan** - Library scanning and indexing

### Infrastructure
- **moosicbox_config** - Configuration management
- **moosicbox_logging** - Centralized logging
- **moosicbox_assert** - Assertion utilities
- **moosicbox_env_utils** - Environment utilities
- **moosicbox_json_utils** - JSON processing utilities

### Networking
- **moosicbox_tunnel** - Tunnel client functionality
- **moosicbox_tunnel_sender** - Tunnel communication
- **moosicbox_ws** - WebSocket support
- **moosicbox_middleware** - HTTP middleware

### User Interface
- **moosicbox_app_models** - Application data models
- **moosicbox_app_native_ui** - Native UI components
- **moosicbox_admin_htmx** - Admin interface

## Usage

### As a Library Dependency

Add to your `Cargo.toml`:

```toml
[dependencies]
moosicbox = { version = "0.1.0", features = ["all-sources", "all-formats"] }
```

### Basic Example

```rust
use moosicbox::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    moosicbox_logging::init(None, None)?;

    // Create a new music library
    let library = Library::new("./music")?;

    // Scan for music files
    library.scan()?;

    // Create a player
    let player = Player::new()?;

    // Play a track
    let tracks = library.search("your favorite song")?;
    if let Some(track) = tracks.first() {
        player.play(track)?;
    }

    Ok(())
}
```

## Feature Flags

### Audio Sources
- `qobuz` - Enable Qobuz streaming support
- `tidal` - Enable Tidal streaming support
- `yt` - Enable YouTube Music support
- `all-sources` - Enable all streaming sources

### Audio Formats
- `format-aac` - AAC/M4A format support
- `format-flac` - FLAC format support
- `format-mp3` - MP3 format support
- `format-opus` - Opus format support
- `all-formats` - Enable all audio formats

### Combined Features
- `default` - Recommended default feature set
- `all-sources` - All streaming service integrations
- `fail-on-warnings` - Treat warnings as errors (development)

## Integration Examples

### Building a Simple Music Server

```rust
use moosicbox::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the server with default configuration
    let server = MoosicBoxServer::builder()
        .with_library_path("./music")
        .with_port(8001)
        .with_features(&["qobuz", "tidal"])
        .build()?;

    // Start the server
    server.run().await?;

    Ok(())
}
```

### Creating a Custom Player

```rust
use moosicbox::prelude::*;

fn create_custom_player() -> Result<(), Box<dyn std::error::Error>> {
    let audio_output = AudioOutput::new(AudioOutputConfig {
        sample_rate: 44100,
        channels: 2,
        buffer_size: 4096,
    })?;

    let player = Player::builder()
        .with_audio_output(audio_output)
        .with_gapless_playback(true)
        .with_crossfade_duration(Duration::from_secs(3))
        .build()?;

    // Use the player...

    Ok(())
}
```

### Multi-Zone Audio Setup

```rust
use moosicbox::prelude::*;

fn setup_audio_zones() -> Result<(), Box<dyn std::error::Error>> {
    let zone_manager = AudioZoneManager::new();

    // Create zones for different rooms
    let living_room = zone_manager.create_zone("Living Room")?;
    let kitchen = zone_manager.create_zone("Kitchen")?;
    let bedroom = zone_manager.create_zone("Bedroom")?;

    // Add players to zones
    living_room.add_player("living_room_speaker_1")?;
    living_room.add_player("living_room_speaker_2")?;
    kitchen.add_player("kitchen_speaker")?;
    bedroom.add_player("bedroom_speaker")?;

    // Play synchronized music across zones
    let track = /* get track from library */;
    zone_manager.play_synchronized(&track, &[&living_room, &kitchen])?;

    Ok(())
}
```

## Architecture

The `moosicbox` package follows a modular architecture where each component can be used independently:

```
moosicbox
├── Core Server (moosicbox_server)
├── Audio Engine (moosicbox_player, moosicbox_audio_*)
├── Library Management (moosicbox_library, moosicbox_scan)
├── Streaming Sources (moosicbox_qobuz, moosicbox_tidal, moosicbox_yt)
├── API Layer (moosicbox_music_api_*)
├── Utilities (moosicbox_files, moosicbox_image, etc.)
└── Infrastructure (moosicbox_config, moosicbox_logging, etc.)
```

## Development

### Building from Source

```bash
# Clone the repository
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox

# Build the main package
cargo build --package moosicbox --features "all-sources"

# Run tests
cargo test --package moosicbox
```

### Contributing

When adding new components to the MoosicBox ecosystem:

1. Add the component to the `Cargo.toml` dependencies
2. Re-export public APIs in `src/lib.rs`
3. Update feature flags if needed
4. Add integration tests
5. Update documentation

## Compatibility

- **Rust Version**: MSRV 1.70+
- **Operating Systems**: Linux, macOS, Windows
- **Architectures**: x86_64, ARM64

## See Also

- [MoosicBox Server](../server/README.md) - Standalone server binary
- [MoosicBox Native App](../app/native/README.md) - Desktop client application
- [MoosicBox Tunnel Server](../tunnel_server/README.md) - Remote access proxy
- [Project Documentation](../../README.md) - Main project documentation
