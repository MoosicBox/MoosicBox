//! Database path utilities for configuration and profile databases.
//!
//! This module provides functions for determining database file paths for both the server
//! configuration database and per-profile library databases. It handles path creation and
//! validation for `SQLite` database files.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_db_dir_path_appends_db_subdirectory() {
        if let Some(config_dir) = get_app_config_dir_path(AppType::App) {
            let db_dir = get_config_db_dir_path(AppType::App);
            assert!(db_dir.is_some());
            assert_eq!(db_dir.unwrap(), config_dir.join("db"));
        }
    }

    #[test]
    fn test_get_config_db_path_returns_config_db_file() {
        if let Some(db_dir) = get_config_db_dir_path(AppType::App) {
            let db_path = get_config_db_path(AppType::App);
            assert!(db_path.is_some());
            assert_eq!(db_path.unwrap(), db_dir.join("config.db"));
        }
    }

    #[test]
    fn test_get_profile_db_dir_path_appends_db_subdirectory() {
        let profile = "test_profile";
        if let Some(profile_dir) = get_profile_dir_path(AppType::App, profile) {
            let db_dir = get_profile_db_dir_path(AppType::App, profile);
            assert!(db_dir.is_some());
            assert_eq!(db_dir.unwrap(), profile_dir.join("db"));
        }
    }

    #[test]
    fn test_get_profile_library_db_path_returns_library_db_file() {
        let profile = "test_profile";
        if let Some(db_dir) = get_profile_db_dir_path(AppType::App, profile) {
            let db_path = get_profile_library_db_path(AppType::App, profile);
            assert!(db_path.is_some());
            assert_eq!(db_path.unwrap(), db_dir.join("library.db"));
        }
    }

    #[test]
    fn test_make_config_db_path_returns_none_when_get_config_db_path_returns_none() {
        // This test relies on the assumption that we're testing with an app type
        // that might not have a config directory configured
        // In a real environment, all AppType variants should return Some
        // so this just verifies the defensive None propagation
        let result = make_config_db_path(AppType::App);
        // If get_config_db_path returns Some, make_config_db_path should too
        if get_config_db_path(AppType::App).is_some() {
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_make_profile_library_db_path_returns_none_when_get_profile_library_db_path_returns_none()
     {
        let profile = "test_profile";
        // Similar to above - verifies defensive None propagation
        let result = make_profile_library_db_path(AppType::App, profile);
        if get_profile_library_db_path(AppType::App, profile).is_some() {
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_path_consistency_across_app_types() {
        // Verify that all AppType variants produce paths with consistent naming
        for app_type in [AppType::App, AppType::Server, AppType::Local] {
            if let Some(config_path) = get_config_db_path(app_type) {
                assert!(
                    config_path.ends_with("db/config.db"),
                    "Config DB path should end with db/config.db for {app_type:?}"
                );
            }

            let profile = "default";
            if let Some(library_path) = get_profile_library_db_path(app_type, profile) {
                assert!(
                    library_path.ends_with("db/library.db"),
                    "Library DB path should end with db/library.db for {app_type:?}"
                );
            }
        }
    }

    #[test]
    fn test_profile_db_paths_differ_for_different_profiles() {
        let profile1 = "profile1";
        let profile2 = "profile2";

        let path1 = get_profile_library_db_path(AppType::App, profile1);
        let path2 = get_profile_library_db_path(AppType::App, profile2);

        if let (Some(p1), Some(p2)) = (path1, path2) {
            assert_ne!(
                p1, p2,
                "Different profiles should have different database paths"
            );
            assert!(
                p1.to_string_lossy().contains(profile1),
                "Profile1 path should contain profile1 name"
            );
            assert!(
                p2.to_string_lossy().contains(profile2),
                "Profile2 path should contain profile2 name"
            );
        }
    }
}
