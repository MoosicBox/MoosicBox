# MoosicBox App State

Application state management system for MoosicBox native applications.

## Overview

The MoosicBox App State package provides:

- **Global State Management**: Centralized application state with async-safe access
- **Player Management**: Audio player lifecycle and session management
- **WebSocket Integration**: Real-time communication with MoosicBox servers
- **Audio Zone Support**: Multi-zone audio configuration and control
- **UPnP Support**: Network audio device discovery and integration (optional)
- **Persistence**: SQLite-based state persistence with HyperChad integration
- **Event System**: Comprehensive event listeners for state changes

## Features

### Core State Management

- **Thread-Safe State**: Arc<RwLock<T>> wrapped state for concurrent access
- **Connection Management**: API and WebSocket connection state
- **Session Tracking**: Current playback sessions and targets
- **Profile Management**: User profile and authentication state
- **Audio Zone State**: Multi-zone audio configuration

### Player Management

- **Local Players**: Local audio playback management
- **Network Players**: UPnP/DLNA network player support
- **Session Players**: Per-session player instances
- **Output Management**: Audio output scanning and configuration
- **Quality Control**: Playback quality settings and management

### Real-Time Communication

- **WebSocket Handling**: Automatic connection management and reconnection
- **Message Buffering**: Message queuing and delivery
- **Event Broadcasting**: State change notifications
- **API Proxying**: Authenticated API request proxying

### Event System

- **Lifecycle Events**: Before/after event hooks for major operations
- **State Change Events**: Notifications for state modifications
- **Session Events**: Playback session state changes
- **Connection Events**: Network connection status updates

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_state = { path = "../app/state" }

# Optional: Enable UPnP support
moosicbox_app_state = {
    path = "../app/state",
    features = ["upnp"]
}
```

## Usage

### Basic State Management

```rust
use moosicbox_app_state::{AppState, UpdateAppState};

// Create new application state
let app_state = AppState::new();

// Update state
let update = UpdateAppState {
    api_url: Some(Some("https://api.moosicbox.com".to_string())),
    profile: Some(Some("default".to_string())),
    ..Default::default()
};

app_state.set_state(update).await?;
```

### Player Management

```rust
use moosicbox_app_state::{AppState, PlayerType};
use moosicbox_audio_output::AudioOutputFactory;
use moosicbox_session::models::ApiPlaybackTarget;

// Create a new player for a session
let player = app_state.new_player(
    session_id,
    playback_target,
    audio_output,
    PlayerType::Local,
).await?;

// Get players for a session
let players = app_state.get_players(session_id, Some(&playback_target)).await;
```

### Event Listeners

```rust
// Add event listeners during state creation
let app_state = AppState::new()
    .with_on_after_handle_playback_update_listener(|update_session| async move {
        println!("Session updated: {:?}", update_session);
    })
    .with_on_current_sessions_updated_listener(|sessions| async move {
        println!("Sessions updated: {} active", sessions.len());
    });
```

### WebSocket Integration

```rust
// WebSocket operations are handled automatically
// State changes trigger WebSocket messages when connected

// Manual WebSocket operations
app_state.start_ws_connection().await?;
app_state.queue_ws_message(payload, true).await?;
app_state.close_ws_connection().await?;
```

## State Structure

### Core State Fields

- **Connection**: API URL, profile, tokens, connection info
- **Players**: Active players, audio zones, UPnP devices
- **Sessions**: Current sessions, playback targets, quality settings
- **WebSocket**: Connection handles, message buffers, join handles
- **Persistence**: SQLite persistence layer

### Audio Management

- **Audio Zones**: Multi-zone configuration and active players
- **Output Scanning**: Available audio output discovery
- **UPnP Integration**: Network device discovery and control
- **Session Players**: Per-session player management

## Event System

### Available Event Hooks

- `on_before_handle_playback_update`
- `on_after_handle_playback_update`
- `on_before_update_playlist`
- `on_after_update_playlist`
- `on_before_handle_ws_message`
- `on_after_handle_ws_message`
- `on_before_set_state`
- `on_after_set_state`
- `on_current_sessions_updated`
- `on_audio_zone_with_sessions_updated`
- `on_connections_updated`

## Feature Flags

### Network Features

- **`upnp`**: Enable UPnP/DLNA network device support (enabled by default)

### Music Source Features

- **`all-sources`**: Enable all music source integrations (enabled by default)
- **`qobuz`**: Enable Qobuz music source
- **`tidal`**: Enable Tidal music source
- **`yt`**: Enable YouTube music source

### Audio Format Features

- **`aac`**: Enable AAC audio format support
- **`flac`**: Enable FLAC audio format support
- **`mp3`**: Enable MP3 audio format support
- **`opus`**: Enable Opus audio format support

### Development Features

- **`fail-on-warnings`**: Treat warnings as errors during compilation

## Error Handling

All operations return `Result<T, AppStateError>` with comprehensive error types:

- **PlayerError**: Audio player operation failures
- **InitUpnpError**: UPnP initialization failures
- **RegisterPlayersError**: Player registration failures
- **ScanOutputsError**: Audio output scanning failures
- **ProxyRequestError**: API proxy request failures

## Threading and Concurrency

- **Thread-Safe**: All state access is protected by Arc<RwLock<T>>
- **Async Operations**: All operations are async-compatible
- **Event Listeners**: Async event handler support
- **WebSocket**: Background WebSocket handling with join handles

## Dependencies

- **HyperChad**: State persistence framework
- **MoosicBox Player**: Audio playback management
- **MoosicBox Session**: Session and connection models
- **MoosicBox Audio Output**: Audio output management
- **MoosicBox WebSocket**: WebSocket communication
- **Tokio**: Async runtime and synchronization

## Integration

This package is designed for integration with MoosicBox native applications and provides the foundation for:

- Desktop applications
- Mobile applications (via Tauri)
- Audio management systems
- Multi-zone audio controllers
