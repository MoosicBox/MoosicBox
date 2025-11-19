//! Database path utilities for configuration and profile databases.
//!
//! This module provides functions for determining database file paths for both the server
//! configuration database and per-profile library databases. It handles path creation and
//! validation for SQLite database files.

use moosicbox_config::{AppType, get_app_config_dir_path, get_profile_dir_path};

/// Returns the directory path for the configuration database.
///
/// This is the parent directory that will contain the `config.db` file.
///
/// # Returns
///
/// * `Some(PathBuf)` - The database directory path if it can be determined
/// * `None` - If the application config directory cannot be determined
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_config_db_dir_path(app_type: AppType) -> Option<std::path::PathBuf> {
    get_app_config_dir_path(app_type).map(|x| x.join("db"))
}

/// Returns the file path for the configuration database.
///
/// The database file is named `config.db` and resides in the database directory.
///
/// # Returns
///
/// * `Some(PathBuf)` - The database file path if the database directory can be determined
/// * `None` - If the database directory cannot be determined
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_config_db_path(app_type: AppType) -> Option<std::path::PathBuf> {
    get_config_db_dir_path(app_type).map(|x| x.join("config.db"))
}

/// Creates the configuration database directory if needed and returns the database path.
///
/// This function ensures that the parent directory exists (creating it if necessary) before
/// returning the database file path. It's suitable for use before opening or creating the
/// database file.
///
/// # Returns
///
/// * `Some(PathBuf)` - The database file path if the directory exists or was created successfully
/// * `None` - If the path cannot be determined or the directory cannot be created
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn make_config_db_path(app_type: AppType) -> Option<std::path::PathBuf> {
    if let Some(path) = get_config_db_path(app_type)
        && (path.is_file()
            || path
                .parent()
                .is_some_and(|x| x.is_dir() || std::fs::create_dir_all(x).is_ok()))
    {
        return Some(path);
    }

    None
}

/// Returns the directory path for a profile's database files.
///
/// Each profile has its own directory for storing profile-specific data including the library
/// database.
///
/// # Returns
///
/// * `Some(PathBuf)` - The profile database directory path if it can be determined
/// * `None` - If the profile directory cannot be determined
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<std::path::PathBuf> {
    get_profile_dir_path(app_type, profile).map(|x| x.join("db"))
}

/// Returns the file path for a profile's library database.
///
/// The library database file is named `library.db` and contains music library data specific to
/// the profile.
///
/// # Returns
///
/// * `Some(PathBuf)` - The library database file path if the profile database directory can be determined
/// * `None` - If the profile database directory cannot be determined
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_profile_library_db_path(app_type: AppType, profile: &str) -> Option<std::path::PathBuf> {
    get_profile_db_dir_path(app_type, profile).map(|x| x.join("library.db"))
}

/// Creates the profile library database directory if needed and returns the database path.
///
/// This function ensures that the parent directory exists (creating it if necessary) before
/// returning the library database file path. It's suitable for use before opening or creating
/// the library database file.
///
/// # Returns
///
/// * `Some(PathBuf)` - The library database file path if the directory exists or was created successfully
/// * `None` - If the path cannot be determined or the directory cannot be created
#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn make_profile_library_db_path(
    app_type: AppType,
    profile: &str,
) -> Option<std::path::PathBuf> {
    if let Some(path) = get_profile_library_db_path(app_type, profile)
        && (path.is_file()
            || path
                .parent()
                .is_some_and(|x| x.is_dir() || std::fs::create_dir_all(x).is_ok()))
    {
        return Some(path);
    }

    None
}
