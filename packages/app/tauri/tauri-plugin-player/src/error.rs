//! Error types for the player plugin.
//!
//! This module defines error types that can occur during player plugin operations,
//! including I/O errors and mobile plugin invocation failures.

use serde::{Serialize, ser::Serializer};

/// Result type for player plugin operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in player plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Mobile plugin invocation error.
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
}

/// Serializes errors as strings for transmission over IPC boundaries.
///
/// This implementation converts errors to their display string representation,
/// making them compatible with Tauri's IPC serialization requirements.
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
