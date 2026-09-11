//! Read-only user directory resolution. No directories are created.
//!
//! `simulator` takes precedence over native backends, including inside real-file
//! I/O scopes. Locations follow the selected simulated filesystem, never the host
//! environment. Native lookup uses platform conventions and returns `None` when
//! unavailable. These APIs do not apply application-specific overrides.

use std::path::PathBuf;

/// Caller-owned simulated user locations. Defaults are all unavailable.
///
/// Paths are supplied verbatim; they need not exist. Resetting filesystem contents
/// does not change these locations. Async tasks must use `scope_filesystem` to
/// select the same filesystem during polling and cancellation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirectoryLocations {
    /// User home directory.
    pub home: Option<PathBuf>,
    /// Roaming configuration directory on Windows, XDG config on Linux.
    pub config: Option<PathBuf>,
    /// Roaming application data on Windows, XDG data on Linux.
    pub data: Option<PathBuf>,
    /// Local (non-roaming) application data on Windows.
    pub data_local: Option<PathBuf>,
}

#[cfg(all(feature = "native-directories", not(feature = "simulator")))]
mod native;

/// Resolve the user's home, or `None` if unavailable.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(feature = "simulator")]
    {
        crate::simulator::directory_locations().home
    }
    #[cfg(not(feature = "simulator"))]
    {
        native::home_dir()
    }
}

/// Resolve configuration storage, or `None` if unavailable.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(feature = "simulator")]
    {
        crate::simulator::directory_locations().config
    }
    #[cfg(not(feature = "simulator"))]
    {
        native::config_dir()
    }
}

/// Resolve roaming application data, or `None` if unavailable.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(feature = "simulator")]
    {
        crate::simulator::directory_locations().data
    }
    #[cfg(not(feature = "simulator"))]
    {
        native::data_dir()
    }
}

/// Resolve local application data, or `None` if unavailable.
#[must_use]
pub fn data_local_dir() -> Option<PathBuf> {
    #[cfg(feature = "simulator")]
    {
        crate::simulator::directory_locations().data_local
    }
    #[cfg(not(feature = "simulator"))]
    {
        native::data_local_dir()
    }
}
