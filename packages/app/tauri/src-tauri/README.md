# MoosicBox Tauri Application

Tauri application for the MoosicBox music streaming platform.

## Overview

The MoosicBox Tauri Application provides:

- **Native App**: Cross-platform application for desktop platforms, with Tauri mobile entry point support
- **Web Integration**: Embedded web view with native API access
- **Music Streaming**: Full MoosicBox music streaming functionality
- **Player Integration**: Native media player controls and system integration
- **File Management**: Local file access and management
- **System Notifications**: System notification support
- **HTTP Proxy**: Built-in HTTP proxy for API communication
- **WebSocket Support**: Real-time communication with MoosicBox servers

## Features

### Desktop Application Features

- **Cross-platform**: Runs on Windows, macOS, and Linux
- **Native Performance**: Rust backend with web frontend
- **System Integration**: Notifications
- **File Access**: Local file system access and management
- **Window Management**: Multiple windows and advanced window controls

### Music Player Features

- **Audio Playback**: High-quality audio playback with multiple format support
- **Playlist Management**: Create, edit, and manage playlists
- **Library Integration**: Browse and search music library
- **Queue Management**: Advanced queue and playback controls
- **Metadata Display**: Rich metadata display with album art

### Streaming Integration

- **Multiple Sources**: Support for Tidal, Qobuz, YouTube Music, and local files
- **Real-time Sync**: Real-time synchronization with other clients
- **Session Management**: Multi-device session management
- **Quality Control**: Configurable audio quality settings

### Development Features

- **Bundled Mode**: Self-contained application with bundled services
- **Native UI**: Optional native UI components with HyperChad
- **HTTP Server**: Built-in HTTP server for web interface
- **Action System**: Comprehensive action handling system

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

This is a Tauri application. The main entry point is:

```rust
fn main() {
    moosicbox_lib::run();
}
```

The application provides the following Tauri commands that can be invoked from the frontend (JavaScript/TypeScript):

- `on_startup()` - Called when the application starts
- `show_main_window()` - Show the main application window (desktop only)
- `set_state(state: TauriUpdateAppState)` - Update application state (connection settings, credentials, profile, and playback target/session)
- `set_playback_quality(quality: PlaybackQuality)` - Set audio playback quality for active players
- `propagate_ws_message(message: InboundPayload)` - Send WebSocket messages to the backend queue
- `api_proxy_get(url: String, headers: Option<serde_json::Value>)` - Proxy GET requests to the configured MoosicBox API
- `api_proxy_post(url: String, body: Option<serde_json::Value>, headers: Option<serde_json::Value>)` - Proxy POST requests to the configured MoosicBox API
- `fetch_moosicbox_servers()` - Fetch discovered MoosicBox servers via mDNS

`TauriUpdateAppState` fields:

- `connection_id`
- `connection_name`
- `api_url`
- `client_id`
- `signature_token`
- `api_token`
- `profile`
- `playback_target`
- `current_session_id`

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

- **`bundled`**: Include bundled MoosicBox services
- **`moosicbox-app-native`**: Enable native UI components

### Audio Sources

- **`tidal`**: Tidal streaming integration
- **`qobuz`**: Qobuz streaming integration
- **`yt`**: YouTube Music integration

### Development Features

- **`fail-on-warnings`**: Treat warnings as errors
- **`devtools`**: Enable Tauri devtools

## Configuration Files

### `tauri.conf.json`

```json
{
    "build": {
        "beforeDevCommand": "pnpm dev",
        "beforeBuildCommand": "pnpm build",
        "devUrl": "http://localhost:1420",
        "frontendDist": "../dist"
    },
    "identifier": "com.moosicbox",
    "productName": "MoosicBox",
    "version": "0.16.1",
    "bundle": {
        "active": true,
        "targets": "all",
        "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
    },
    "app": {
        "windows": [
            {
                "fullscreen": false,
                "resizable": true,
                "title": "MoosicBox",
                "width": 800,
                "height": 600
            }
        ],
        "security": {
            "csp": null
        }
    }
}
```

## Dependencies

### Core Dependencies

- **tauri**: Application framework and command bridge
- **app-tauri-plugin-player**: Native media control plugin integration
- **moosicbox_app_state**: Shared state and API proxy operations
- **moosicbox_player**: Playback management
- **moosicbox_session**: Session models and updates
- **moosicbox_ws**: WebSocket payload models and communication
- **moosicbox_mdns**: mDNS service discovery
- **serde**: Serialization framework

### Optional Dependencies

- **moosicbox_app_tauri_bundled**: Bundled service runtime (with `bundled`)
- **moosicbox_app_client**: Client integration (with `client`)
- **hyperchad**, **moosicbox_app_native**, **moosicbox_app_native_image**, **moosicbox_app_native_ui**: Native UI stack (with `moosicbox-app-native`)
- **tauri-plugin-log**: Tauri log plugin support (with `tauri-logger`)

## Platform Support

### Supported Platforms

- **Windows**: Windows 10+ (x86_64, aarch64)
- **macOS**: macOS 10.15+ (x86_64, aarch64)
- **Linux**: Ubuntu 18.04+, Debian 10+, Arch Linux, Fedora 31+
- **Android**: Supported via Tauri mobile entry point

### System Requirements

- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 500MB for application, additional for music cache
- **Network**: Internet connection for streaming services
- **Audio**: Audio output device

## Troubleshooting

### Common Issues

**Build Failures:**

```bash
# Clear build cache
cargo clean
rm -rf node_modules
npm install
cargo tauri build
```

**WebView Issues:**

```bash
# Update WebView2 (Windows)
# Install webkit2gtk (Linux)
sudo apt update && sudo apt install webkit2gtk-4.0
```

**Permission Issues:**

```bash
# macOS: Grant permissions in System Preferences
# Linux: Install required packages
sudo apt install libappindicator3-1
```

## Security Considerations

- **CSP**: Content Security Policy configuration
- **File Access**: Scoped file system access
- **API Keys**: Secure storage of API credentials
- **Network**: HTTPS-only communication
- **Updates**: Secure update mechanism

## Integration

This application integrates with:

- **MoosicBox Server**: Core music streaming backend
- **Streaming Services**: Tidal, Qobuz, YouTube Music
- **Local Files**: Local music library
- **System Audio**: Native audio system integration
