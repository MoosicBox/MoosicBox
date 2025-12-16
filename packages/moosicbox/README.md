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

This package re-exports the following MoosicBox components as optional dependencies. Enable the ones you need via feature flags.

### Core Components

- **moosicbox_player** - Audio playback engine
- **moosicbox_library** - Music library management
- **moosicbox_library_models** - Library data models
- **moosicbox_audio_zone** - Multi-zone audio control
- **moosicbox_audio_zone_models** - Audio zone data models
- **moosicbox_session** - Session and state management
- **moosicbox_session_models** - Session data models

### API & Routing

- **moosicbox_music_api** - Music streaming API
- **moosicbox_auth** - Authentication functionality
- **moosicbox_profiles** - User profile management

### Audio Processing

- **moosicbox_audio_decoder** - Audio format decoding
- **moosicbox_audio_encoder** - Audio format encoding
- **moosicbox_audio_output** - Audio output management
- **moosicbox_resampler** - Audio sample rate conversion

### Streaming Sources

- **moosicbox_qobuz** - Qobuz streaming integration (optional)
- **moosicbox_tidal** - Tidal streaming integration (optional)
- **moosicbox_yt** - YouTube Music integration (optional)
- **moosicbox_remote_library** - Remote library access

### Utilities

- **moosicbox_files** - File handling and streaming
- **moosicbox_image** - Image processing and optimization
- **moosicbox_downloader** - Download management
- **moosicbox_search** - Search functionality
- **moosicbox_scan** - Library scanning and indexing
- **moosicbox_paging** - Pagination utilities
- **moosicbox_menu** - Menu functionality

### Infrastructure

- **moosicbox_config** - Configuration management
- **moosicbox_logging** - Centralized logging
- **moosicbox_assert** - Assertion utilities
- **moosicbox_env_utils** - Environment utilities
- **moosicbox_json_utils** - JSON processing utilities
- **moosicbox_async_service** - Async service utilities
- **moosicbox_channel_utils** - Channel utilities
- **moosicbox_stream_utils** - Stream utilities
- **moosicbox_schema** - Database schema management
- **moosicbox_arb** - ARB file support
- **moosicbox_load_balancer** - Load balancing functionality

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
moosicbox = "0.1.0"
```

By default, all components are enabled. You can selectively enable only what you need:

```toml
[dependencies]
moosicbox = { version = "0.1.0", default-features = false, features = ["player", "library", "logging"] }
```

### Basic Example

This package re-exports underlying components, allowing you to access their APIs:

```rust
// Import specific components you need (requires corresponding features enabled)
use moosicbox::logging;
use moosicbox::player;
use moosicbox::library;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    logging::init(None, None)?;

    // Use the individual component APIs
    // See each component's documentation for specific usage examples

    Ok(())
}
```

**Note**: This package serves as a re-export umbrella. For detailed usage examples and API documentation, refer to the individual component packages.

## Feature Flags

This package provides feature flags that enable specific components and pass through to their dependencies.

### Streaming Sources

- `qobuz` - Enable Qobuz streaming support
- `tidal` - Enable Tidal streaming support
- `yt` - Enable YouTube Music support
- `all-sources` - Enable all streaming sources (qobuz, tidal, yt)

### Component Enablement

Individual components can be enabled with their respective feature flags:

- `admin-htmx`, `app-models`, `app-native-ui`, `arb`, `assert`, `async-service`
- `audio-decoder`, `audio-encoder`, `audio-output`, `audio-zone`, `audio-zone-models`
- `auth`, `channel-utils`, `config`, `downloader`, `env-utils`, `files`
- `image`, `json-utils`, `library`, `library-models`, `load-balancer`, `logging`
- `menu`, `middleware`, `music-api`, `paging`, `player`, `profiles`
- `remote-library`, `resampler`, `scan`, `schema`, `search`
- `session`, `session-models`, `stream-utils`, `tunnel`, `tunnel-sender`, `ws`

### Meta Features

- `default` - Enables all components with their default features (`all-default`)
- `all` - Enable all components (without sub-features)
- `all-default` - Enable all components with their default sub-features
- `fail-on-warnings` - Treat warnings as errors (development)

### Tunnel Encoding

- `tunnel-base64` - Enable base64 encoding for tunnel
- `tunnel-sender-base64` - Enable base64 encoding for tunnel sender

## Integration

This package is designed to be used as a dependency aggregator. Instead of adding multiple individual MoosicBox crates to your `Cargo.toml`, you can add this single package with the features you need.

### Example: Enabling Multiple Components

```toml
[dependencies]
moosicbox = {
    version = "0.1.0",
    features = [
        "player",
        "library",
        "audio-decoder",
        "qobuz",
        "tidal"
    ]
}
```

Then in your Rust code:

```rust
// Access re-exported components
use moosicbox::player;
use moosicbox::library;
use moosicbox::qobuz;

// Use their respective APIs as documented in each component
```

## Architecture

The `moosicbox` package is a pure re-export crate with no additional logic. It follows a modular architecture where each component can be used independently:

```
moosicbox (re-export umbrella)
├── Audio Engine (player, audio_decoder, audio_encoder, audio_output, resampler)
├── Library Management (library, library_models, scan)
├── Session & Zones (session, session_models, audio_zone, audio_zone_models)
├── Streaming Sources (qobuz, tidal, yt, remote_library)
├── API Layer (music_api, auth, profiles)
├── Utilities (files, image, downloader, search, paging, menu)
├── Infrastructure (config, logging, schema, async_service, stream_utils)
├── Networking (tunnel, tunnel_sender, ws, middleware)
└── UI (app_models, app_native_ui, admin_htmx)
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

### Adding Components

When adding new components to this re-export package:

1. Add the component to `Cargo.toml` dependencies as optional
2. Add a feature flag for the component
3. Re-export it in `src/lib.rs` with `#[cfg(feature = "...")]`
4. Add the component to the `all` and `all-default` feature lists
5. Add a `fail-on-warnings` pass-through if applicable
6. Update this README's component list

## Compatibility

- **Rust Version**: MSRV 1.85+
- **Operating Systems**: Linux, macOS, Windows
- **Architectures**: x86_64, ARM64

## See Also

- [Project Documentation](../../README.md) - Main project documentation
- Individual component READMEs in `packages/` directory for specific API documentation
