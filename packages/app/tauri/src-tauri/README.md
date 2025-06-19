# MoosicBox Tauri Application

Native desktop application for MoosicBox music streaming platform built with Tauri.

## Overview

The MoosicBox Tauri Application provides:

- **Native Desktop App**: Cross-platform desktop application for Windows, macOS, and Linux
- **Web Integration**: Embedded web view with native API access
- **Music Streaming**: Full MoosicBox music streaming functionality
- **Player Integration**: Native media player controls and system integration
- **File Management**: Local file access and management
- **System Integration**: System notifications, tray integration, and OS-specific features
- **HTTP Proxy**: Built-in HTTP proxy for API communication
- **WebSocket Support**: Real-time communication with MoosicBox servers

## Features

### Desktop Application Features
- **Cross-platform**: Runs on Windows, macOS, and Linux
- **Native Performance**: Rust backend with web frontend
- **System Integration**: Media keys, notifications, and system tray
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

### Basic Application

```rust
use moosicbox_app_tauri::{run, TauriUpdateAppState};
use tauri::{Manager, Window};

#[tokio::main]
async fn main() {
    // Run the Tauri application
    run();
}

// Application startup
#[tauri::command]
async fn on_startup() -> Result<(), tauri::Error> {
    println!("MoosicBox application started");
    Ok(())
}

// Update application state
#[tauri::command]
async fn set_state(state: TauriUpdateAppState) -> Result<(), String> {
    // Update connection settings, API URLs, etc.
    println!("State updated: {:?}", state);
    Ok(())
}
```

### Window Management

```rust
use tauri::{AppHandle, Manager, Window};

#[tauri::command]
async fn show_main_window(window: Window) {
    window.get_webview_window("main")
        .unwrap()
        .show()
        .unwrap();
}

#[tauri::command]
async fn create_player_window(app: AppHandle) -> Result<(), String> {
    let player_window = tauri::WindowBuilder::new(
        &app,
        "player",
        tauri::WindowUrl::App("player.html".into())
    )
    .title("MoosicBox Player")
    .inner_size(400.0, 600.0)
    .resizable(false)
    .build()
    .map_err(|e| e.to_string())?;
    
    Ok(())
}
```

### Music Player Integration

```rust
use moosicbox_player::{Playback, PlayerError};
use moosicbox_music_models::{ApiTrack, PlaybackQuality};

#[tauri::command]
async fn play_track(track_id: u64) -> Result<(), String> {
    // Play a specific track
    println!("Playing track: {}", track_id);
    Ok(())
}

#[tauri::command]
async fn set_playback_quality(quality: PlaybackQuality) -> Result<(), String> {
    // Set audio quality
    println!("Quality set to: {:?}", quality);
    Ok(())
}

#[tauri::command]
async fn get_current_playback() -> Result<Option<Playback>, String> {
    // Get current playback state
    Ok(None)
}

// Handle media events from the player plugin
async fn handle_media_event(event: MediaEvent) -> Result<(), String> {
    match event {
        MediaEvent::Play => println!("Playback started"),
        MediaEvent::Pause => println!("Playback paused"),
        MediaEvent::Stop => println!("Playback stopped"),
        MediaEvent::Next => println!("Next track"),
        MediaEvent::Previous => println!("Previous track"),
    }
    Ok(())
}
```

### API Integration

```rust
use serde_json::Value;

#[tauri::command]
async fn api_proxy_get(
    url: String,
    headers: Option<Value>,
) -> Result<Value, String> {
    // Proxy GET request to MoosicBox API
    println!("GET request to: {}", url);
    Ok(serde_json::json!({"success": true}))
}

#[tauri::command]
async fn api_proxy_post(
    url: String,
    body: Option<Value>,
    headers: Option<Value>,
) -> Result<Value, String> {
    // Proxy POST request to MoosicBox API
    println!("POST request to: {}", url);
    Ok(serde_json::json!({"success": true}))
}
```

### WebSocket Communication

```rust
use moosicbox_ws::models::{InboundPayload, OutboundPayload};

#[tauri::command]
async fn propagate_ws_message(message: InboundPayload) -> Result<(), String> {
    // Handle WebSocket messages from the web interface
    println!("WebSocket message: {:?}", message);
    Ok(())
}

async fn handle_ws_message(message: OutboundPayload) {
    // Handle outbound WebSocket messages
    match message {
        OutboundPayload::SessionUpdated(session) => {
            println!("Session updated: {:?}", session);
        }
        OutboundPayload::PlaybackUpdate(update) => {
            println!("Playback update: {:?}", update);
        }
        _ => {}
    }
}
```

### File System Access

```rust
use std::path::PathBuf;

#[tauri::command]
async fn get_data_dir() -> Result<PathBuf, String> {
    // Get application data directory
    tauri::api::path::app_data_dir(&tauri::Config::default())
        .ok_or_else(|| "Failed to get data directory".to_string())
}

#[tauri::command]
async fn read_config_file() -> Result<String, String> {
    let data_dir = get_data_dir().await?;
    let config_path = data_dir.join("config.json");
    
    std::fs::read_to_string(config_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn write_config_file(content: String) -> Result<(), String> {
    let data_dir = get_data_dir().await?;
    let config_path = data_dir.join("config.json");
    
    std::fs::write(config_path, content)
        .map_err(|e| e.to_string())
}
```

### Native UI Integration

```rust
// With moosicbox-app-native feature
use hyperchad_renderer_vanilla_js::VanillaJsTagRenderer;
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_template::container;

async fn handle_http_request(request: HttpRequest) -> Result<HttpResponse, String> {
    // Handle HTTP requests for native UI
    let view = container! {
        div class="app" {
            h1 { "MoosicBox Native UI" }
            
            div class="player-controls" {
                button onclick=tauri_invoke("play_track", 123) { "Play" }
                button onclick=tauri_invoke("pause_track", null) { "Pause" }
                button onclick=tauri_invoke("next_track", null) { "Next" }
            }
            
            div class="library" {
                h2 { "Music Library" }
                // Library content
            }
        }
    };
    
    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .body(view.to_string()))
}
```

### System Integration

```rust
use tauri::{SystemTray, SystemTrayMenu, SystemTrayMenuItem, SystemTrayEvent};

fn create_system_tray() -> SystemTray {
    let menu = SystemTrayMenu::new()
        .add_item(SystemTrayMenuItem::new("Show", "show"))
        .add_item(SystemTrayMenuItem::new("Hide", "hide"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(SystemTrayMenuItem::new("Play/Pause", "play_pause"))
        .add_item(SystemTrayMenuItem::new("Next", "next"))
        .add_item(SystemTrayMenuItem::new("Previous", "previous"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(SystemTrayMenuItem::new("Quit", "quit"));
    
    SystemTray::new().with_menu(menu)
}

fn handle_system_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            let window = app.get_window("main").unwrap();
            window.show().unwrap();
            window.set_focus().unwrap();
        }
        SystemTrayEvent::MenuItemClick { id, .. } => {
            match id.as_str() {
                "show" => {
                    let window = app.get_window("main").unwrap();
                    window.show().unwrap();
                }
                "hide" => {
                    let window = app.get_window("main").unwrap();
                    window.hide().unwrap();
                }
                "play_pause" => {
                    // Toggle playback
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        }
        _ => {}
    }
}
```

### Configuration

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_url: Option<String>,
    pub connection_name: Option<String>,
    pub audio_quality: PlaybackQuality,
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_url: None,
            connection_name: None,
            audio_quality: PlaybackQuality::High,
            auto_start: false,
            minimize_to_tray: true,
            theme: "dark".to_string(),
        }
    }
}

#[tauri::command]
async fn load_config() -> Result<AppConfig, String> {
    let config_path = get_data_dir().await?.join("config.json");
    
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&content)
            .map_err(|e| e.to_string())
    } else {
        Ok(AppConfig::default())
    }
}

#[tauri::command]
async fn save_config(config: AppConfig) -> Result<(), String> {
    let config_path = get_data_dir().await?.join("config.json");
    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| e.to_string())?;
    
    std::fs::write(config_path, content)
        .map_err(|e| e.to_string())
}
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
- **`bundled`**: Include bundled MoosicBox services
- **`moosicbox-app-native`**: Enable native UI components

### Audio Sources
- **`tidal`**: Tidal streaming integration
- **`qobuz`**: Qobuz streaming integration
- **`yt`**: YouTube Music integration

### Development Features
- **`fail-on-warnings`**: Treat warnings as errors
- **`debug`**: Enable debug features

## Configuration Files

### `tauri.conf.json`
```json
{
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devPath": "http://localhost:1420",
    "distDir": "../dist"
  },
  "package": {
    "productName": "MoosicBox",
    "version": "0.1.0"
  },
  "tauri": {
    "allowlist": {
      "all": false,
      "shell": {
        "all": false,
        "open": true
      },
      "fs": {
        "all": true,
        "scope": ["$APPDATA/*", "$AUDIO/*", "$DOWNLOAD/*"]
      }
    },
    "bundle": {
      "active": true,
      "targets": "all",
      "identifier": "com.moosicbox.app",
      "icon": [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/icon.icns",
        "icons/icon.ico"
      ]
    },
    "security": {
      "csp": null
    },
    "windows": [
      {
        "fullscreen": false,
        "resizable": true,
        "title": "MoosicBox",
        "width": 1200,
        "height": 800
      }
    ]
  }
}
```

## Dependencies

### Core Dependencies
- **Tauri**: Desktop application framework
- **MoosicBox Core**: Music streaming and player functionality
- **Tokio**: Async runtime
- **Serde**: Serialization framework

### Optional Dependencies
- **HyperChad**: Native UI components (with `moosicbox-app-native`)
- **HTTP Server**: Built-in HTTP server for web interface
- **WebSocket**: Real-time communication

## Platform Support

### Supported Platforms
- **Windows**: Windows 10+ (x86_64, aarch64)
- **macOS**: macOS 10.15+ (x86_64, aarch64)
- **Linux**: Ubuntu 18.04+, Debian 10+, Arch Linux, Fedora 31+

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
- **Media Keys**: System media key handling
