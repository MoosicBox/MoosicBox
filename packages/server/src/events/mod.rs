//! Event handling for server-wide events.
//!
//! This module provides event handlers and initialization for various server events including
//! audio zone changes, playback events, profile management, download progress, and library scanning.
//! Events are typically dispatched via WebSocket to connected clients.

pub mod audio_zone_event;
#[cfg(feature = "downloader")]
pub mod download_event;
#[cfg(feature = "player")]
pub mod playback_event;
pub mod profiles_event;
#[cfg(feature = "scan")]
pub mod scan_event;
pub mod session_event;
