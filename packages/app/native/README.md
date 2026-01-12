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
- **Download Support**: Download music for offline playback

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

Server connections are configured through the in-app settings interface:

- Navigate to Settings → Connections
- Add a new connection with your server URL
- Select the connection to use

### UI Backend Selection

The default build includes multiple UI backends. You can enable specific backends with cargo features:

#### Egui (GPU Accelerated - Default)

```bash
cargo run --features "egui-wgpu"
```

Additional Egui variants:

- `egui-glow` - OpenGL backend
- `egui-v1` - Legacy HyperChad egui renderer implementation (original, overcomplicated)
- `egui-v2` - New HyperChad egui renderer implementation (work in progress, simplified)

#### FLTK (Lightweight)

```bash
cargo run --features "fltk" --no-default-features
```

#### Web Interface (HTML + JavaScript)

```bash
cargo run --features "html,vanilla-js" --no-default-features
```

The web interface supports additional deployment options:

- `actix` - Deploy with Actix web server
- `lambda` - Deploy as AWS Lambda function

## Configuration

### Environment Variables

| Variable        | Description                                                  | Default  |
| --------------- | ------------------------------------------------------------ | -------- |
| `WINDOW_WIDTH`  | Initial window width                                         | `1000.0` |
| `WINDOW_HEIGHT` | Initial window height                                        | `600.0`  |
| `WINDOW_X`      | Initial window X position                                    | -        |
| `WINDOW_Y`      | Initial window Y position                                    | -        |
| `MAX_THREADS`   | Maximum blocking threads                                     | `64`     |
| `TOKIO_CONSOLE` | Enable tokio console (requires `console-subscriber` feature) | -        |
| `RUST_LOG`      | Logging configuration                                        | -        |

Note: Server connection settings are configured through the in-app settings interface rather than environment variables.

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

- **Audio Visualization** - Real-time waveform display
- **Volume Control** - Adjust playback volume
- **Seek Control** - Navigate within tracks
- **Queue Management** - Control playback order

#### Planned Features

- Gapless playback
- Crossfade transitions
- Replay Gain normalization
- Equalizer controls
- Lyrics display

### Interface Features

- **Library Browser** - Navigate your music collection
- **Albums View** - Browse and filter albums by source
- **Artist View** - View artist details and albums
- **Audio Zones** - Manage playback zones
- **Playback Sessions** - View and control active sessions
- **Settings** - Configure connections, downloads, and music API sources
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

- **Standalone Operation** - Embedded server runs in the same process
- **Auto-Configuration** - Automatic server startup and initialization
- **Performance** - Direct in-process communication with the server

## Development

### Local Development

```bash
# Start with debug features
cargo run --features "dev,console-subscriber,debug"
```

Note: The `dev` feature enables asset serving and static route handling for development.

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

# Profile with tracing
cargo run --features "profiling-tracing,egui-wgpu"

# Profile with Tracy
cargo run --features "profiling-tracy,egui-wgpu"
```

## Packaging

### Windows Installer

```bash
# Build MSI installer
cargo install cargo-wix
cargo wix --install
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

The user interface is built using the HyperChad framework and includes:

1. **Home View** - Main dashboard with library access
2. **Albums View** - Browse and filter albums by source
3. **Artists View** - View artist details and albums
4. **Player Controls** - Play, pause, skip, seek, volume control
5. **Audio Visualization** - Real-time waveform display
6. **Settings** - Configure connections, downloads, music API sources, and scan settings
7. **Audio Zones & Sessions** - Manage playback zones and sessions

The interface and keyboard shortcuts are currently determined by the UI backend being used (Egui, FLTK, or Web). **Note**: Standardized keybindings across all UI backends is a planned future enhancement.

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

## Architecture

The MoosicBox Native App is structured as follows:

- **Main Application** (`moosicbox_app_native`) - Application entry point and routing
- **UI Components** (`moosicbox_app_native_ui`) - HyperChad-based UI components
- **Bundled Server** (`moosicbox_app_native_bundled`) - Optional embedded server mode
- **App State** (`moosicbox_app_state`) - Application state management
- **Player** (`moosicbox_player`) - Audio playback functionality
- **Music API** (`moosicbox_music_api`) - Multi-source music integration

## See Also

- [MoosicBox App Native UI](ui/README.md) - UI component library
- [MoosicBox App Native Bundled](bundled/README.md) - Bundled server mode
