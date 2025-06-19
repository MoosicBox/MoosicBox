# MoosicBox Native App

A cross-platform desktop music application built with the HyperChad framework, providing a native interface for the MoosicBox music server.

## Overview

The MoosicBox Native App is a desktop client that provides:

- **Cross-Platform Support**: Windows, macOS, and Linux
- **Multiple UI Backends**: Egui (GPU-accelerated), FLTK (lightweight), Web (HTML/JS)
- **Music Streaming**: Connect to local or remote MoosicBox servers
- **Multi-Source Support**: Local library, Tidal, Qobuz, YouTube Music
- **High-Quality Audio**: Support for FLAC, AAC, MP3, Opus formats
- **Modern Interface**: Responsive design with native look and feel
- **Offline Capability**: Download and cache music for offline playback

## Installation

### From Source

```bash
cargo install --path packages/app/native --features "default"
```

### Pre-built Binaries

Download from the [releases page](https://github.com/MoosicBox/MoosicBox/releases) or build locally.

### System Dependencies

#### Linux
```bash
# Ubuntu/Debian
sudo apt-get install pkg-config libssl-dev libgtk-3-dev

# For audio support
sudo apt-get install libasound2-dev libpulse-dev

# For Egui backend
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

#### macOS
```bash
# Using Homebrew
brew install pkg-config openssl

# For audio support
brew install portaudio
```

#### Windows
- Visual Studio Build Tools or Visual Studio Community
- Windows SDK

## Usage

### Basic Usage

Start the native app:
```bash
moosicbox_app_native
```

Or using cargo:
```bash
cargo run --bin moosicbox_app_native --features "default"
```

### Connection Configuration

#### Connect to Local Server
```bash
# Default: connects to localhost:8001
moosicbox_app_native

# Custom server address
MOOSICBOX_SERVER_URL=http://192.168.1.100:8001 moosicbox_app_native
```

#### Connect to Remote Server
```bash
# Through tunnel server
MOOSICBOX_SERVER_URL=https://your-tunnel.moosicbox.com moosicbox_app_native
```

### UI Backend Selection

#### Egui (Default - GPU Accelerated)
```bash
cargo run --features "egui-wgpu"
```

#### FLTK (Lightweight)
```bash
cargo run --features "fltk" --no-default-features
```

#### Web Interface
```bash
cargo run --features "html,vanilla-js" --no-default-features
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MOOSICBOX_SERVER_URL` | MoosicBox server URL | `http://localhost:8001` |
| `MOOSICBOX_AUTH_TOKEN` | Authentication token | - |
| `MOOSICBOX_DOWNLOAD_DIR` | Download directory | `~/Music/MoosicBox` |
| `MOOSICBOX_CACHE_DIR` | Cache directory | Platform-specific |
| `AUDIO_BUFFER_SIZE` | Audio buffer size | `4096` |

### Configuration File

Create `~/.config/moosicbox/config.toml`:

```toml
[server]
url = "http://localhost:8001"
auth_token = "your_token_here"

[audio]
buffer_size = 4096
sample_rate = 44100
bit_depth = 16

[downloads]
directory = "~/Music/MoosicBox"
auto_download_favorites = true
quality = "lossless"

[ui]
theme = "dark"
show_visualizer = true
window_size = [1200, 800]
```

## Features

### Audio Formats Support
- **FLAC** - Lossless audio with full quality
- **AAC/M4A** - High-quality lossy compression
- **MP3** - Universal compatibility
- **Opus** - Modern, efficient codec

### Music Sources
- **Local Library** - Your personal music collection
- **Tidal** - Hi-Fi streaming service
- **Qobuz** - Hi-Res audio streaming
- **YouTube Music** - Google's music service

### Playback Features
- **Gapless Playback** - Seamless album listening
- **Crossfade** - Smooth transitions between tracks
- **Replay Gain** - Volume normalization
- **Equalizer** - Audio enhancement
- **Audio Visualization** - Real-time audio analysis

### Interface Features
- **Library Browser** - Navigate your music collection
- **Search** - Find music across all sources
- **Playlists** - Create and manage playlists
- **Queue Management** - Control playback queue
- **Now Playing** - Current track information with lyrics
- **Download Manager** - Offline music management

## Building

### Development Build

```bash
# Debug build with all features
cargo build --features "default"

# Specific UI backend
cargo build --features "egui-wgpu,all-decoders,all-sources"
```

### Release Build

```bash
# Optimized release build
cargo build --release --features "default"

# Minimal build for distribution
cargo build --release --features "egui-wgpu,all-os-decoders" --no-default-features
```

### Cross-Platform Builds

```bash
# Build for Windows from Linux
cargo build --release --target x86_64-pc-windows-gnu

# Build for macOS from Linux (requires cross-compilation setup)
cargo build --release --target x86_64-apple-darwin

# Build AppImage for Linux
cargo install cargo-appimage
cargo appimage --release
```

## Bundled Mode

For self-contained deployment with embedded server:

```bash
# Build with bundled server
cargo build --release --features "bundled,all-sources,all-decoders"

# This creates a single executable with embedded MoosicBox server
```

### Bundled Mode Features
- **Standalone Operation** - No separate server required
- **Auto-Configuration** - Automatic setup and database initialization
- **Portable** - Single executable with all dependencies
- **Performance** - Direct in-process communication

## Development

### Local Development

```bash
# Start with hot reloading and debug features
cargo run --features "dev,console-subscriber,debug"
```

### Testing Different Backends

```bash
# Test Egui backend
cargo test --features "egui-wgpu"

# Test FLTK backend
cargo test --features "fltk"

# Test web backend
cargo test --features "html,vanilla-js"
```

### Performance Profiling

```bash
# Profile with puffin
cargo run --features "profiling-puffin,egui-wgpu"

# Profile with Tracy
cargo run --features "profiling-tracy,egui-wgpu"
```

## Packaging

### Windows Installer

```bash
# Build MSI installer
cargo install cargo-wix
cargo wix --install

# Build NSIS installer (requires NSIS)
cargo install cargo-wix
cargo wix --installer nsis
```

### macOS Bundle

```bash
# Create .app bundle
cargo install cargo-bundle
cargo bundle --release

# Create DMG
cargo install cargo-bundle
cargo bundle --format dmg --release
```

### Linux Packages

```bash
# Create AppImage
cargo install cargo-appimage
cargo appimage --release

# Create Debian package
cargo install cargo-deb
cargo deb --release

# Create RPM package (requires rpm tools)
cargo install cargo-rpm
cargo rpm build --release
```

## User Interface

### Main Interface Components

1. **Navigation Panel** - Library, playlists, sources
2. **Content View** - Albums, artists, tracks listing
3. **Player Controls** - Play, pause, skip, seek
4. **Queue Panel** - Current and upcoming tracks
5. **Now Playing** - Track info, artwork, lyrics

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Space` | Play/Pause |
| `Left/Right` | Seek backward/forward |
| `Up/Down` | Volume up/down |
| `Ctrl+F` | Search |
| `Ctrl+L` | Focus library |
| `Ctrl+Q` | Quit application |

### Themes

- **Dark Theme** - Easy on the eyes
- **Light Theme** - Classic appearance
- **Auto Theme** - Follow system preference
- **Custom Themes** - User-configurable colors

## Troubleshooting

### Common Issues

1. **Connection failed**: Check server URL and network connectivity
2. **Audio playback issues**: Verify audio drivers and output device
3. **High CPU usage**: Try different UI backend or disable visualizations
4. **Crashes**: Check system dependencies and graphics drivers

### Debug Information

```bash
# Enable detailed logging
RUST_LOG=moosicbox_app_native=debug moosicbox_app_native

# Audio debugging
RUST_LOG=moosicbox_audio=debug moosicbox_app_native

# Network debugging
RUST_LOG=moosicbox_http=debug moosicbox_app_native
```

### Performance Issues

```bash
# Disable GPU acceleration if issues occur
cargo run --features "egui-glow" --no-default-features

# Use lightweight FLTK backend
cargo run --features "fltk" --no-default-features
```

## See Also

- [MoosicBox Server](../server/README.md) - Backend music server
- [HyperChad Framework](../hyperchad/README.md) - UI framework
- [MoosicBox Tunnel Server](../tunnel_server/README.md) - Remote access solution
- [Tauri App](../app/tauri/README.md) - Alternative Tauri-based client
