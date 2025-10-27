//! Player initialization and management.
//!
//! This module handles initialization of audio players including local players and UPnP/DLNA devices.
//! Players are registered with the server and made available for playback control.

pub mod local;
#[cfg(feature = "upnp")]
pub mod upnp;
