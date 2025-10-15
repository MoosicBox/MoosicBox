# MoosicBox Tauri Application

Native desktop application for MoosicBox music streaming platform built with Tauri.

## Overview

The MoosicBox Tauri Application provides:

- **Native Desktop App**: Cross-platform desktop application for Windows, macOS, Linux, and Android
- **Web Integration**: Embedded web view with native API access
- **Music Streaming**: Full MoosicBox music streaming functionality via WebSocket communication
- **Player Integration**: Native media player controls via Tauri plugin
- **mDNS Discovery**: Automatic discovery of MoosicBox servers on the local network
- **UPnP Support**: UPnP device integration for audio streaming
- **HTTP Proxy**: Built-in HTTP proxy for API communication
- **WebSocket Support**: Real-time communication with MoosicBox servers

## Features

### Desktop Application Features

- **Cross-platform**: Runs on Windows, macOS, Linux, and Android
- **Native Performance**: Rust backend with web frontend
- **File Access**: Local file system access and management via Tauri plugins
- **Notifications**: System notifications via Tauri plugin

### Music Player Features

- **Audio Playback**: High-quality audio playback with multiple format support (via `moosicbox_player`)
- **Session Management**: Multi-device session management with WebSocket synchronization
- **Quality Control**: Configurable audio quality settings
- **Media Controls**: Play/pause/next/previous controls (Android native media integration)

### Streaming Integration

- **Multiple Sources**: Support for Tidal, Qobuz, YouTube Music, and local files (via feature flags)
- **Real-time Sync**: Real-time synchronization with other clients via WebSocket
- **API Proxy**: Proxied HTTP requests to MoosicBox API with authentication

### Development Features

- **Bundled Mode**: Self-contained application with bundled MoosicBox services (optional)
- **Native UI**: Optional native UI components with HyperChad (via `moosicbox-app-native` feature)
- **mDNS Scanner**: Automatic server discovery on local network
- **UPnP Listener**: UPnP device discovery and integration

## Installation

### Prerequisites

**System Dependencies:**

```bash
# macOS
brew install node

# Ubuntu/Debian
sudo apt update
sudo apt install libwebkit2gtk-4.0-dev build-essential curl wget libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev

# Arch Linux
sudo pacman -S webkit2gtk base-devel curl wget openssl gtk3 libappindicator-gtk3 librsvg

# Fedora
sudo dnf install webkit2gtk3-devel.x86_64 openssl-devel curl wget libappindicator-gtk3-devel librsvg2-devel
sudo dnf group install "C Development Tools and Libraries"
```

**Rust and Tauri CLI:**

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Tauri CLI
cargo install tauri-cli
```

### Build from Source

```bash
# Clone the repository
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox

# Build the Tauri application
cd packages/app/tauri
npm install
cargo tauri build

# Development mode
cargo tauri dev
```

## Usage

This is a Tauri application binary, not a library. The main entry point is:

```rust
// main.rs
fn main() {
    moosicbox_lib::run()
}
```

### Tauri Commands

The application exposes the following Tauri commands that can be invoked from the frontend:

#### Application State Management

```typescript
// TypeScript/JavaScript frontend code

// Update application connection and settings
await invoke('set_state', {
  state: {
    connectionId: 'connection-uuid',
    connectionName: 'My Connection',
    apiUrl: 'https://api.moosicbox.com',
    clientId: 'client-uuid',
    signatureToken: 'signature-token',
    apiToken: 'api-token',
    profile: 'default',
    playbackTarget: { type: 'local' },
    currentSessionId: 123
  }
});

// Called during app startup
await invoke('on_startup');
```

#### Playback Control

```typescript
// Set audio playback quality
await invoke('set_playback_quality', {
  quality: 'High' // or 'Low', 'FlacHighestRes', etc.
});
```

#### WebSocket Communication

```typescript
// Propagate WebSocket messages from frontend to backend
await invoke('propagate_ws_message', {
  message: {
    // InboundPayload structure
    UpdateSession: {
      payload: {
        sessionId: 123,
        playing: true,
        position: 5,
        // ... other session fields
      }
    }
  }
});
```

#### API Proxy

```typescript
// Proxy GET requests to MoosicBox API
const result = await invoke('api_proxy_get', {
  url: '/menu',
  headers: {
    'Content-Type': 'application/json'
  }
});

// Proxy POST requests to MoosicBox API
const result = await invoke('api_proxy_post', {
  url: '/sessions',
  body: { name: 'My Session' },
  headers: {
    'Content-Type': 'application/json'
  }
});
```

#### mDNS Server Discovery

```typescript
// Fetch discovered MoosicBox servers on the local network
const servers = await invoke('fetch_moosicbox_servers');
// Returns: Array<{ id: string, name: string, host: string, dns: string }>
```

#### Window Management (Desktop Only)

```typescript
// Show the main window (not available on Android)
await invoke('show_main_window');
```

### Event Listeners

The application emits events that the frontend can listen to:

```typescript
import { listen } from '@tauri-apps/api/event';

// Listen for WebSocket connection events
await listen('ws-connect', (event) => {
  console.log('WebSocket connected:', event.payload);
  // payload: { connection_id: string, ws_url: string }
});

// Listen for WebSocket messages from the backend
await listen('ws-message', (event) => {
  console.log('WebSocket message:', event.payload);
  // payload: OutboundPayload (various message types)
});

// Listen for SSE events (when using moosicbox-app-native feature)
await listen('sse-event', (event) => {
  console.log('SSE event:', event.payload);
  // payload: { id?: string, event: string, data: string }
});
```

## Building and Distribution

### Development Build

```bash
# Start development server
cargo tauri dev

# Build for development
cargo tauri build --debug
```

### Production Build

```bash
# Build for production
cargo tauri build

# Build for specific target
cargo tauri build --target x86_64-pc-windows-msvc
```

### Code Signing

```bash
# macOS code signing
export APPLE_CERTIFICATE="Developer ID Application: Your Name"
export APPLE_CERTIFICATE_PASSWORD="your-password"
cargo tauri build

# Windows code signing
export WINDOWS_CERTIFICATE_THUMBPRINT="your-thumbprint"
cargo tauri build
```

### Distribution

```bash
# Create installer packages
cargo tauri build --bundles msi  # Windows MSI
cargo tauri build --bundles dmg  # macOS DMG
cargo tauri build --bundles deb  # Linux DEB
cargo tauri build --bundles rpm  # Linux RPM
```

## Feature Flags

### Core Features

- **`bundled`**: Include bundled MoosicBox services (self-contained server)
- **`client`**: Enable client-specific functionality
- **`moosicbox-app-native`**: Enable native UI components with HyperChad
- **`custom-protocol`**: Enable Tauri custom protocol (required for production builds)
- **`android`**: Android platform support
- **`desktop`**: Desktop platform support (includes tunnel support when bundled)

### Audio Output

- **`cpal`**: CPAL audio output (default)
- **`asio`**: ASIO audio output
- **`jack`**: JACK audio output

### Audio Formats

- **`all-formats`**: All audio formats (default)
- **`all-os-formats`**: OS-compatible formats only (AAC, FLAC)
- **`format-aac`**: AAC format support
- **`format-flac`**: FLAC format support
- **`format-mp3`**: MP3 format support

### Audio Decoders

- **`all-decoders`**: All audio decoders
- **`all-os-decoders`**: OS-compatible decoders (AAC, FLAC)
- **`decoder-aac`**: AAC decoder
- **`decoder-flac`**: FLAC decoder
- **`decoder-mp3`**: MP3 decoder

### Streaming Sources

- **`all-sources`**: All streaming sources (default)
- **`tidal`**: Tidal streaming integration
- **`qobuz`**: Qobuz streaming integration
- **`yt`**: YouTube Music integration

### Development Features

- **`fail-on-warnings`**: Treat warnings as errors during compilation
- **`devtools`**: Enable Tauri devtools
- **`tauri-logger`**: Use Tauri's logging plugin instead of custom logger

## Architecture

### Core Components

1. **lib.rs**: Main application entry point with the `run()` function
   - Initializes Tauri application
   - Sets up plugins (fs, dialog, notification, player)
   - Configures WebSocket and state management
   - Spawns background services (mDNS, UPnP)
   - Registers Tauri commands

2. **mdns.rs**: mDNS service discovery
   - `fetch_moosicbox_servers()` command to retrieve discovered servers
   - Background scanner for automatic server discovery

3. **AppState** (from `moosicbox_app_state`): Central state management
   - Connection management (API URL, tokens, client ID)
   - WebSocket connection handling
   - Player state synchronization
   - Session management

4. **Native UI** (optional, via `moosicbox-app-native` feature):
   - HyperChad-based native UI rendering
   - Custom URI scheme handler for `tauri://` protocol
   - SSE-style event system for UI updates

### Application Flow

1. **Startup**:
   - Initialize logging
   - Set up data directory
   - Initialize AppState
   - Start bundled services (if `bundled` feature enabled)
   - Start mDNS scanner
   - Start UPnP listener
   - Register playback event handlers

2. **State Management**:
   - Frontend calls `set_state()` to configure connection
   - Backend updates internal state and log properties
   - State changes propagate to listeners

3. **WebSocket Communication**:
   - Frontend receives `ws-connect` event with connection details
   - Frontend establishes WebSocket connection
   - Messages flow bidirectionally via `propagate_ws_message()`
   - Backend emits `ws-message` events to frontend

4. **Playback**:
   - Playback events trigger `on_playback_event()` callback
   - Updates propagate to player plugin (Android)
   - Updates sent via WebSocket to other clients
   - Session state synchronized across devices

## Dependencies

### Core Dependencies

- **tauri**: Desktop application framework (v2.x)
- **moosicbox_app_state**: Application state management with UPnP support
- **moosicbox_player**: Audio playback engine with local playback support
- **moosicbox_session**: Session management
- **moosicbox_ws**: WebSocket message models
- **moosicbox_music_models**: Music data models
- **switchy**: Runtime and service orchestration (mDNS, UPnP)
- **tokio**: Async runtime
- **serde/serde_json**: Serialization

### Tauri Plugins

- **tauri-plugin-fs**: File system access
- **tauri-plugin-dialog**: Native dialogs
- **tauri-plugin-notification**: System notifications
- **app-tauri-plugin-player**: Custom player plugin
- **tauri-plugin-log**: Logging (optional)

### Optional Dependencies

- **moosicbox_app_tauri_bundled**: Bundled MoosicBox server (with `bundled` feature)
- **moosicbox_app_client**: Client functionality (with `client` feature)
- **moosicbox_app_native**: Native UI with HyperChad (with `moosicbox-app-native` feature)
- **hyperchad**: UI framework for native rendering

## Platform Support

### Supported Platforms

- **Windows**: Windows 10+ (x86_64)
- **macOS**: macOS 10.15+ (x86_64, aarch64/Apple Silicon)
- **Linux**: Ubuntu 18.04+, Debian 10+, Arch Linux, Fedora 31+
- **Android**: Android 7.0+ (API level 24+)

### System Requirements

- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 100-500MB for application (varies by platform and features)
- **Network**: Internet connection for streaming services and server communication
- **Audio**: Audio output device

## Troubleshooting

### Common Issues

**Build Failures:**

```bash
# Clear build cache
cargo clean
cd packages/app/tauri
rm -rf node_modules
npm install
cargo tauri build
```

**WebView Issues on Linux:**

```bash
# Install webkit2gtk
sudo apt update && sudo apt install webkit2gtk-4.1-dev
```

**Missing System Dependencies:**

Refer to the Prerequisites section above for platform-specific system dependencies.

**Logging:**

By default, logs are written to `moosicbox_app.log` in the application data directory (except on Android). Enable `TOKIO_CONSOLE=1` environment variable for tokio-console integration.

## Security Considerations

- **File Access**: Scoped file system access via Tauri's permission system
- **API Authentication**: Signature tokens and API tokens for secure server communication
- **Network**: Communication with MoosicBox servers (HTTP/WebSocket)
- **Custom Protocol**: Uses Tauri's custom protocol for production builds

## Integration

This application integrates with:

- **MoosicBox Server**: Core music streaming backend (via HTTP and WebSocket)
- **Streaming Services**: Tidal, Qobuz, YouTube Music (when respective features enabled)
- **Local Files**: Local music library and file system access
- **mDNS**: Automatic discovery of MoosicBox servers on local network
- **UPnP**: UPnP device discovery and integration
- **Audio Playback**: Local audio playback via `moosicbox_player` with multiple output options (CPAL, ASIO, JACK)

## Related Packages

- **moosicbox_app_state**: Application state management (`packages/app/state/`)
- **moosicbox_app_tauri_bundled**: Bundled server mode (`packages/app/tauri/bundled/`)
- **moosicbox_app_native**: Native UI components (`packages/app/native/`)
- **moosicbox_app_client**: Client functionality (`packages/app/client/`)
- **app-tauri-plugin-player**: Tauri player plugin (`packages/app/tauri/plugin-player/`)
