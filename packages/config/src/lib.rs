//! Configuration management for `MoosicBox` applications.
//!
//! This crate provides functionality for managing `MoosicBox` configuration, including:
//!
//! * Configuration directory paths for different application types (app, server, local)
//! * Profile management for organizing settings per user or environment
//! * File-based configuration loading with JSON5 support
//! * Database-backed configuration storage (with `db` feature)
//! * HTTP API endpoints for configuration (with `api` feature)
//!
//! # Directory Structure
//!
//! Configuration is stored in `~/.local/moosicbox` by default, organized by application type:
//!
//! ```text
//! ~/.local/moosicbox/
//! ├── server/
//! │   ├── config.json5          # Global server config
//! │   └── profiles/
//! │       ├── default/
//! │       │   └── config.json5  # Profile-specific config
//! │       └── production/
//! │           └── config.json5
//! └── cache/                     # Shared cache directory
//! ```
//!
//! # Example
//!
//! ```rust
//! use moosicbox_config::{AppType, get_app_config_dir_path, get_profile_dir_path};
//!
//! // Get the configuration directory for a server application
//! if let Some(config_dir) = get_app_config_dir_path(AppType::Server) {
//!     println!("Server config directory: {:?}", config_dir);
//! }
//!
//! // Get a specific profile's directory
//! if let Some(profile_dir) = get_profile_dir_path(AppType::Server, "production") {
//!     println!("Production profile directory: {:?}", profile_dir);
//! }
//! ```
//!
//! # Features
//!
//! * `file` - File-based configuration loading with JSON5 support
//! * `db` - Database-backed configuration and profile management
//! * `api` - HTTP API endpoints for configuration (requires `db`)
//! * `openapi` - `OpenAPI` schema generation for API endpoints

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;
#[cfg(feature = "file")]
pub mod file;

/// Represents the type of `MoosicBox` application.
///
/// Used to determine the appropriate configuration directory structure.
#[derive(Copy, Clone, Debug)]
pub enum AppType {
    /// Mobile or desktop application
    App,
    /// Server application
    Server,
    /// Local development instance
    Local,
}

impl From<AppType> for &str {
    fn from(value: AppType) -> Self {
        match value {
            AppType::App => "app",
            AppType::Server => "server",
            AppType::Local => "local",
        }
    }
}

impl std::fmt::Display for AppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).into())
    }
}

static ROOT_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

/// Sets the root directory for `MoosicBox` configuration.
///
/// By default, the root directory is `~/.local/moosicbox`. This function allows
/// overriding that default location.
///
/// # Panics
///
/// * If the `ROOT_DIR` mutex is poisoned (which can only occur if another thread panicked while holding the lock)
pub fn set_root_dir(path: PathBuf) {
    *ROOT_DIR.lock().unwrap() = Some(path);
}

/// Returns the root directory for `MoosicBox` configuration.
///
/// Defaults to `~/.local/moosicbox` unless overridden with [`set_root_dir`].
/// This is an internal helper function that caches the root directory path.
///
/// # Panics
///
/// * If the `ROOT_DIR` mutex is poisoned (which can only occur if another thread panicked while holding the lock)
#[must_use]
fn get_root_dir() -> Option<PathBuf> {
    let mut root_dir = ROOT_DIR.lock().unwrap();

    if root_dir.is_some() {
        return root_dir.clone();
    }

    *root_dir = home::home_dir().map(|home| home.join(".local").join("moosicbox"));

    root_dir.clone()
}

/// Returns the path to the `MoosicBox` configuration directory.
///
/// Defaults to `~/.local/moosicbox` unless overridden with [`set_root_dir`].
#[must_use]
pub fn get_config_dir_path() -> Option<PathBuf> {
    get_root_dir()
}

/// Returns the path to the application-specific configuration directory.
///
/// For example, for `AppType::Server`, this returns `~/.local/moosicbox/server`.
#[must_use]
pub fn get_app_config_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_config_dir_path().map(|x| x.join(app_type.to_string()))
}

/// Returns the path to the profiles directory for the specified application type.
///
/// For example, for `AppType::Server`, this returns `~/.local/moosicbox/server/profiles`.
#[must_use]
pub fn get_profiles_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_app_config_dir_path(app_type).map(|x| x.join("profiles"))
}

/// Returns the path to a specific profile's directory.
///
/// For example, for `AppType::Server` and profile name `"default"`, this returns
/// `~/.local/moosicbox/server/profiles/default`.
#[must_use]
pub fn get_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    get_profiles_dir_path(app_type).map(|x| x.join(profile))
}

/// Returns the path to the cache directory.
///
/// Defaults to `~/.local/moosicbox/cache`.
#[must_use]
pub fn get_cache_dir_path() -> Option<PathBuf> {
    get_config_dir_path().map(|config| config.join("cache"))
}

/// Returns the path to the configuration directory, creating it if it doesn't exist.
///
/// Returns `None` if the directory cannot be created or the path cannot be determined.
#[must_use]
pub fn make_config_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_config_dir_path()
        && (path.is_dir() || std::fs::create_dir_all(&path).is_ok())
    {
        return Some(path);
    }

    None
}

/// Returns the path to a profile's directory, creating it if it doesn't exist.
///
/// Returns `None` if the directory cannot be created or the path cannot be determined.
#[must_use]
pub fn make_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    if let Some(path) = get_profile_dir_path(app_type, profile)
        && (path.is_dir() || std::fs::create_dir_all(&path).is_ok())
    {
        return Some(path);
    }

    None
}

/// Returns the path to the cache directory, creating it if it doesn't exist.
///
/// Returns `None` if the directory cannot be created or the path cannot be determined.
#[must_use]
pub fn make_cache_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_cache_dir_path()
        && (path.is_dir() || std::fs::create_dir_all(&path).is_ok())
    {
        return Some(path);
    }

    None
}

/// Returns a unique temporary directory path for test isolation.
///
/// Each call generates a unique directory name based on process ID and timestamp
/// to prevent test interference. The directory is created in the system's temp directory.
#[must_use]
pub fn get_tests_dir_path() -> PathBuf {
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let pid = std::process::id();

    std::env::temp_dir().join(format!("moosicbox_tests_{pid}_{timestamp}"))
}

#[cfg(feature = "db")]
pub use db_impl::*;

#[cfg(feature = "db")]
mod db_impl {
    use moosicbox_json_utils::database::DatabaseFetchError;
    use switchy_database::{DatabaseError, config::ConfigDatabase};

    use crate::db::{GetOrInitServerIdentityError, models};

    /// Retrieves the server identity from the database.
    ///
    /// Returns `None` if no server identity has been initialized.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn get_server_identity(db: &ConfigDatabase) -> Result<Option<String>, DatabaseError> {
        crate::db::get_server_identity(db).await
    }

    /// Retrieves the server identity from the database, creating it if it doesn't exist.
    ///
    /// This function ensures a unique server identity exists by creating one if needed.
    /// The identity is generated using a random nanoid.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    /// * If the server identity cannot be created or retrieved
    pub async fn get_or_init_server_identity(
        db: &ConfigDatabase,
    ) -> Result<String, GetOrInitServerIdentityError> {
        crate::db::get_or_init_server_identity(db).await
    }

    /// Creates or retrieves a profile by name.
    ///
    /// If a profile with the given name already exists, returns it. Otherwise, creates
    /// a new profile and triggers a profile update event.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn upsert_profile(
        db: &ConfigDatabase,
        name: &str,
    ) -> Result<models::Profile, DatabaseFetchError> {
        let profiles = crate::db::get_profiles(db).await?;

        if let Some(profile) = profiles.into_iter().find(|x| x.name == name) {
            return Ok(profile);
        }

        if let Err(e) = moosicbox_profiles::events::trigger_profiles_updated_event(
            vec![name.to_string()],
            vec![],
        )
        .await
        {
            moosicbox_assert::die_or_error!("Failed to trigger profiles updated event: {e:?}");
        }

        create_profile(db, name).await
    }

    /// Deletes a profile by name.
    ///
    /// Returns the list of deleted profiles and triggers a profile update event.
    /// If no profile with the given name exists, returns an empty list.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn delete_profile(
        db: &ConfigDatabase,
        name: &str,
    ) -> Result<Vec<models::Profile>, DatabaseFetchError> {
        let profiles = crate::db::delete_profile(db, name).await?;

        if profiles.is_empty() {
            return Ok(profiles);
        }

        if let Err(e) = moosicbox_profiles::events::trigger_profiles_updated_event(
            vec![],
            vec![name.to_owned()],
        )
        .await
        {
            moosicbox_assert::die_or_error!("Failed to trigger profiles updated event: {e:?}");
        }

        Ok(profiles)
    }

    /// Creates a new profile with the given name.
    ///
    /// After creation, triggers a profile update event to notify other components.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn create_profile(
        db: &ConfigDatabase,
        name: &str,
    ) -> Result<models::Profile, DatabaseFetchError> {
        let profile = crate::db::create_profile(db, name).await?;

        if let Err(e) = moosicbox_profiles::events::trigger_profiles_updated_event(
            vec![profile.name.clone()],
            vec![],
        )
        .await
        {
            moosicbox_assert::die_or_error!("Failed to trigger profiles updated event: {e:?}");
        }

        Ok(profile)
    }

    /// Retrieves all profiles from the database.
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn get_profiles(
        db: &ConfigDatabase,
    ) -> Result<Vec<models::Profile>, DatabaseFetchError> {
        crate::db::get_profiles(db).await
    }
}
