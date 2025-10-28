//! Tauri command handlers for player plugin operations.
//!
//! This module provides the command handlers that are exposed to the frontend
//! JavaScript/TypeScript code through Tauri's IPC mechanism.

use tauri::{AppHandle, Runtime, command};

use crate::PlayerExt;
use crate::Result;
use crate::models::{StateResponse, UpdateState};

/// Updates the player state based on the provided payload.
///
/// This command allows the frontend to control various aspects of the player including
/// playback state, position, seek location, volume, and playlist.
///
/// # Errors
///
/// Returns an error if:
/// * The player backend fails to update the state
/// * The mobile plugin invocation fails (on mobile platforms)
#[command]
pub async fn update_state<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateState,
) -> Result<StateResponse> {
    app.player().update_state(payload)
}
