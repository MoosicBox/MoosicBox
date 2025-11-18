//! Mobile implementation of the player plugin.
//!
//! This module provides the mobile player interface for iOS and Android platforms.
//! It bridges Rust code with native mobile implementations (Swift for iOS, Kotlin for Android)
//! that integrate with platform-specific media session APIs.

use serde::de::DeserializeOwned;
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

use crate::models::*;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.moosicbox.playerplugin";

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_player);

/// Initializes the mobile player plugin.
///
/// Registers the native mobile plugin (Swift for iOS, Kotlin for Android) and
/// creates a new [`Player`] instance that bridges to the native implementation.
///
/// # Errors
///
/// Returns an error if:
/// * Plugin registration fails on the native side
/// * The native plugin class cannot be found
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: &PluginApi<R, C>,
) -> crate::Result<Player<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "PlayerPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_player)?;
    Ok(Player(handle))
}

/// Access to the player APIs.
pub struct Player<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Player<R> {
    /// Updates the player state on mobile platforms.
    ///
    /// Invokes the native mobile plugin to update the player state, including
    /// playback status, position, seek location, volume, and playlist. This
    /// integrates with platform-specific media session APIs.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The mobile plugin invocation fails
    /// * The native implementation encounters an error
    /// * Communication with the native side fails
    pub fn update_state(&self, payload: UpdateState) -> crate::Result<StateResponse> {
        self.0
            .run_mobile_plugin("updateState", payload)
            .map_err(Into::into)
    }

    /// Initializes a communication channel on mobile platforms.
    ///
    /// Sets up an IPC channel for receiving media control events from the
    /// platform's media session API (play/pause, next/previous track, etc.).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The mobile plugin invocation fails
    /// * Channel initialization on the native side fails
    /// * Communication with the native side fails
    pub fn init_channel(&self, payload: InitChannel) -> crate::Result<InitChannelResponse> {
        self.0
            .run_mobile_plugin("initChannel", payload)
            .map_err(Into::into)
    }
}
