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

// Create main page layout
let content = container! {
    div { "Page content here" }
};

let ui = page(&state, &content);
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

// Handle user actions
let action = Action::PlayAlbum {
    album_id: 123.into(),
    api_source: ApiSource::library(),
    version_source: None,
    sample_rate: Some(44100),
    bit_depth: Some(16),
};
```

## Constants

### Layout Constants
- `FOOTER_HEIGHT`: Footer component height
- `FOOTER_ICON_SIZE`: Icon sizes in footer
- `CURRENT_ALBUM_SIZE`: Current album artwork size
- `VIZ_HEIGHT`: Visualization component height

### Color Constants
- `DARK_BACKGROUND`: Dark theme background color
- `BACKGROUND`: Standard background color

## Dependencies

- **HyperChad**: UI framework and templating
- **MoosicBox Music Models**: Music data structures
- **MoosicBox Session Models**: Session management
- **Serde**: Serialization for actions and state
