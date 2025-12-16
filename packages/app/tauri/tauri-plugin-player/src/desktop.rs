//! Desktop implementation of the player plugin.
//!
//! This module provides a lightweight player interface for desktop platforms
//! (Windows, macOS, Linux). The desktop implementation provides stub functionality
//! as the actual player logic is typically handled in the frontend.

use serde::de::DeserializeOwned;
use tauri::{AppHandle, Runtime, plugin::PluginApi};

use crate::models::{InitChannel, InitChannelResponse, StateResponse, UpdateState};

/// Initializes the desktop player plugin.
///
/// Creates a new [`Player`] instance for desktop platforms.
#[must_use]
pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: &PluginApi<R, C>,
) -> Player<R> {
    Player(app.clone())
}

/// Desktop player implementation providing access to player APIs.
///
/// This struct provides a lightweight interface for desktop platforms (Windows, macOS,
/// Linux). The desktop implementation returns stub responses as actual player logic is
/// typically handled in the frontend JavaScript/TypeScript code.
pub struct Player<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Player<R> {
    /// Updates the player state.
    ///
    /// On desktop platforms, this method returns a success response without performing
    /// any operations, as player state management is typically handled in the frontend.
    ///
    /// # Errors
    ///
    /// This method always succeeds on desktop platforms.
    #[allow(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        clippy::unnecessary_wraps
    )]
    pub fn update_state(&self, _payload: UpdateState) -> crate::Result<StateResponse> {
        Ok(StateResponse {})
    }

    /// Initializes a communication channel.
    ///
    /// On desktop platforms, this method returns a success response without performing
    /// any operations, as channel initialization is not required for desktop.
    ///
    /// # Errors
    ///
    /// This method always succeeds on desktop platforms.
    #[allow(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        clippy::unnecessary_wraps
    )]
    pub fn init_channel(&self, _payload: InitChannel) -> crate::Result<InitChannelResponse> {
        Ok(InitChannelResponse {})
    }
}
