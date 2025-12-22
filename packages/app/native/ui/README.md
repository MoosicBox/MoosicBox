# MoosicBox Native App UI

User interface components and layouts for MoosicBox native applications.

## Overview

The MoosicBox Native App UI package provides:

- **UI Components**: Complete user interface component library
- **Layout Management**: Responsive layouts and navigation
- **Music Controls**: Audio playback and control interfaces
- **State Integration**: UI components integrated with application state
- **HyperChad Integration**: Built on HyperChad UI framework

## Components

### Core UI

- **Navigation**: Sidebar navigation and routing
- **Player**: Audio player controls and visualization
- **Footer**: Bottom player interface
- **Modal**: Modal dialog components

### Music UI

- **Albums**: Album browsing and display
- **Artists**: Artist listing and navigation
- **Search**: Music search interface
- **Play Queue**: Playback queue management

### Settings

- **Audio Zones**: Multi-zone audio configuration
- **Downloads**: Download management interface
- **Playback Sessions**: Session management UI
- **Settings**: Application configuration interface

## Features

### Responsive Design

- **Adaptive Layouts**: Responsive to different screen sizes
- **Flexible Components**: Configurable component sizing
- **Modern Styling**: Contemporary UI design patterns

### Music Integration

- **Real-time Updates**: Live playback state updates
- **Album Art**: Cover art display and management
- **Progress Tracking**: Playback progress visualization
- **Volume Control**: Audio volume management

### Action System

- **Custom Actions**: Comprehensive action system for user interactions
- **Playback Control**: Play, pause, skip, seek functionality
- **Queue Management**: Add to queue, play album, track selection
- **Filter Controls**: Album filtering and sorting

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_native_ui = { path = "../app/native/ui" }
```

## Usage

### Basic UI Components

```rust
use moosicbox_app_native_ui::{page, sidebar_navigation, footer};
use hyperchad::template::container;

// Create sidebar navigation
let nav = sidebar_navigation();

// Create page with content
let content = container! {
    div { "Page content here" }
};

let ui = page(&state, &content);

// Create footer with player
let footer_ui = footer(&state);
```

### Player Interface

```rust
use moosicbox_app_native_ui::player;

// Create player interface
let player_ui = player(&state);
```

### Action Handling

```rust
use moosicbox_app_native_ui::Action;
use moosicbox_music_models::{ApiSource, TrackApiSource, AlbumSort};

// Play an album
let action = Action::PlayAlbum {
    album_id: 123.into(),
    api_source: ApiSource::library(),
    version_source: None,
    sample_rate: Some(44100),
    bit_depth: Some(16),
};

// Other available actions:
// - Action::RefreshVisualization
// - Action::TogglePlayback
// - Action::PreviousTrack
// - Action::NextTrack
// - Action::SetVolume
// - Action::SeekCurrentTrackPercent
// - Action::FilterAlbums { filtered_sources, sort }
// - Action::AddAlbumToQueue { album_id, api_source, version_source, sample_rate, bit_depth }
// - Action::PlayAlbumStartingAtTrackId { album_id, start_track_id, api_source, version_source, sample_rate, bit_depth }
// - Action::PlayTracks { track_ids, api_source }
```

## Constants

### Layout Constants

- `FOOTER_HEIGHT`: Footer component height (calculated from 100 + VIZ_HEIGHT + VIZ_PADDING \* 2 + FOOTER_BORDER_SIZE)
- `FOOTER_ICON_SIZE`: Icon sizes in footer (25px)
- `FOOTER_BORDER_SIZE`: Footer border size (3px)
- `CURRENT_ALBUM_SIZE`: Current album artwork size (70px)
- `VIZ_HEIGHT`: Visualization component height (35px)
- `VIZ_PADDING`: Visualization padding (5px)

### Color Constants

- `DARK_BACKGROUND`: Dark theme background color (`#080a0b`)
- `BACKGROUND`: Standard background color (`#181a1b`)

### Element IDs

- `AUDIO_ZONES_ID`: Audio zones modal identifier
- `AUDIO_ZONES_CONTENT_ID`: Audio zones content container identifier
- `PLAYBACK_SESSIONS_ID`: Playback sessions modal identifier
- `PLAYBACK_SESSIONS_CONTENT_ID`: Playback sessions content container identifier
- `PLAY_QUEUE_ID`: Play queue modal identifier
- `VOLUME_SLIDER_CONTAINER_ID`: Volume slider container identifier
- `VOLUME_SLIDER_ID`: Volume slider identifier
- `VOLUME_SLIDER_VALUE_CONTAINER_ID`: Volume slider value container identifier
- `VOLUME_SLIDER_VALUE_ID`: Volume slider value identifier

## Modules

The package is organized into the following modules:

- `albums` - Album browsing, display, and cover art management
- `artists` - Artist listing, navigation, and cover art
- `audio_zones` - Audio zone configuration UI
- `downloads` - Download management interface with progress tracking
- `formatting` - Time and data formatting utilities
- `play_queue` - Playback queue display and management
- `playback_sessions` - Session management UI
- `search` - Music search interface
- `settings` - Application configuration interface
- `state` - Application state management

## Dependencies

- **HyperChad**: UI framework and templating with actions, actions-logic, actions-serde, color, renderer-canvas, serde, template, and transformer features
- **MoosicBox App Models**: Application-level models with music-api-api support
- **MoosicBox Audio Zone Models**: Audio zone data structures
- **MoosicBox Date Utils**: Date formatting utilities with chrono support
- **MoosicBox Downloader**: Download management with API support
- **MoosicBox Menu Models**: Menu data structures with API support
- **MoosicBox Music API Models**: Music API data structures with api-search support
- **MoosicBox Music Models**: Music data structures with API support
- **MoosicBox Paging**: Pagination utilities
- **MoosicBox Session Models**: Session management data structures
- **switchy_env**: Environment variable utilities with std support
- **bytesize**: Byte size formatting
- **rust_decimal**: Decimal number handling
- **serde**: Serialization for actions and state
