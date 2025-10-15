# Tauri Plugin Player

A Tauri plugin that provides media player control functionality for desktop and mobile platforms. Includes full Android support, with iOS planned.

## Features

- **Player State Management**: Update and manage player state including playback status, position, seek, volume, and playlists
- **Media Event Channel**: Receive media control events (play/pause, next/previous track) from native platform integrations (Android only)
- **Cross-Platform Support**: Works on desktop (stub implementation) and Android (full native integration)
- **Type-Safe API**: Fully typed Rust and Kotlin APIs with serde serialization

## Architecture

### Rust Core (`src/`)

- **`lib.rs`**: Plugin initialization and registration with Tauri
- **`commands.rs`**: Tauri command handlers (`update_state`)
- **`models.rs`**: Shared data models (`Track`, `Playlist`, `UpdateState`, `MediaEvent`, etc.)
- **`error.rs`**: Error types and result handling
- **`desktop.rs`**: Desktop platform implementation (stub)
- **`mobile.rs`**: Mobile platform implementation (delegates to native code)

### Android Implementation (`android/`)

- **`PlayerPlugin.kt`**: Tauri plugin interface (`com.moosicbox.playerplugin`) with commands:
    - `initChannel`: Initialize event channel for media events
    - `updateState`: Update player state from frontend
- **`Player.kt`**: Player companion object with state management and `sendMediaEvent` for event emission

## Data Models

### Track

```rust
pub struct Track {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub album: String,
    pub album_cover: Option<String>,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub duration: f64,
}
```

### UpdateState

```rust
pub struct UpdateState {
    pub playing: Option<bool>,
    pub position: Option<u16>,
    pub seek: Option<f64>,
    pub volume: Option<f64>,
    pub playlist: Option<Playlist>,
}
```

### MediaEvent

```rust
pub struct MediaEvent {
    pub play: Option<bool>,
    pub next_track: Option<bool>,
    pub prev_track: Option<bool>,
}
```

## Usage

### Setup

Add the plugin to your Tauri application in `src-tauri/src/main.rs`:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(app_tauri_plugin_player::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Rust API

Access the player from any Tauri context:

```rust
use app_tauri_plugin_player::PlayerExt;

fn example<R: Runtime>(app: &AppHandle<R>) {
    let player = app.player();
    // Use player methods
}
```

### Command API

The plugin exposes the following Tauri command:

- `update_state`: Update player state from the frontend

## Dependencies

- `tauri`: ^2.0.0
- `serde`: Serialization framework
- `thiserror`: Error handling

## Platform Support

- **Desktop** (Windows, macOS, Linux): Stub implementation (methods return empty responses)
- **Android**: Full native implementation with media session integration
- **iOS**: Planned (binding macro exists in Rust but no implementation files present)

## Development

### Building

```bash
cargo build
```

### Android

The Android implementation is located in `android/` and follows standard Tauri plugin conventions for mobile platforms.

## Package Information

- **Name**: `app-tauri-plugin-player`
- **Version**: 0.1.4
- **Description**: Player plugin
- **License**: See workspace license

## Notes

- The desktop implementation currently provides stub methods (`update_state` and `init_channel`) that return empty responses
- iOS binding exists only as a macro in `src/mobile.rs` - no implementation files are present
- The JavaScript/TypeScript guest bindings (`guest-js/`) are not currently included in the repository
