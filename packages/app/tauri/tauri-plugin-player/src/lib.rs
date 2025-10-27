//! Tauri plugin for audio player control and media session management.
//!
//! This plugin provides platform-specific audio playback control for `MoosicBox` applications,
//! supporting both desktop and mobile platforms with native media controls.
//!
//! # Features
//!
//! * Control playback state (play, pause, seek)
//! * Manage playlists and track navigation
//! * Handle platform media control events
//! * Cross-platform support (desktop and mobile)
//!
//! # Usage
//!
//! Initialize the plugin in your Tauri application:
//!
//! ```rust,ignore
//! use tauri_plugin_player::PlayerExt;
//!
//! tauri::Builder::default()
//!     .plugin(tauri_plugin_player::init())
//!     .setup(|app| {
//!         // Access player through the extension trait
//!         let player = app.player();
//!         Ok(())
//!     })
//!     .run(tauri::generate_context!())
//!     .expect("error while running tauri application");
//! ```
//!
//! # Platform Support
//!
//! The plugin adapts to the target platform:
//!
//! * **Desktop**: Provides a lightweight player interface
//! * **Mobile** (iOS/Android): Integrates with native media session APIs
//!
//! # Main Types
//!
//! * [`PlayerExt`] - Extension trait for accessing player functionality
//! * [`init()`] - Plugin initialization function
//! * [`Track`] - Music track with metadata
//! * [`Playlist`] - Collection of tracks
//! * [`UpdateState`] - Request for updating player state
//! * [`Error`] - Error types for plugin operations

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use tauri::{
    Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Player;
#[cfg(mobile)]
use mobile::Player;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the player APIs.
pub trait PlayerExt<R: Runtime> {
    /// Gets a reference to the player instance.
    fn player(&self) -> &Player<R>;
}

impl<R: Runtime, T: Manager<R>> crate::PlayerExt<R> for T {
    fn player(&self) -> &Player<R> {
        self.state::<Player<R>>().inner()
    }
}

/// Initializes the plugin.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("player")
        .invoke_handler(tauri::generate_handler![commands::update_state])
        .setup(|app, api| {
            #[cfg(mobile)]
            let player = mobile::init(app, &api)?;
            #[cfg(desktop)]
            let player = desktop::init(app, &api);
            app.manage(player);
            Ok(())
        })
        .build()
}
