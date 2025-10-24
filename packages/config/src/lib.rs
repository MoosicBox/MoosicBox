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
/// * If the `ROOT_DIR` `Mutex` is poisoned
pub fn set_root_dir(path: PathBuf) {
    *ROOT_DIR.lock().unwrap() = Some(path);
}

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

    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn get_server_identity(db: &ConfigDatabase) -> Result<Option<String>, DatabaseError> {
        crate::db::get_server_identity(db).await
    }

    /// # Errors
    ///
    /// * If a database error occurs
    /// * If the server server identity has not been initialized
    pub async fn get_or_init_server_identity(
        db: &ConfigDatabase,
    ) -> Result<String, GetOrInitServerIdentityError> {
        crate::db::get_or_init_server_identity(db).await
    }

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

    /// # Errors
    ///
    /// * If a database error occurs
    pub async fn get_profiles(
        db: &ConfigDatabase,
    ) -> Result<Vec<models::Profile>, DatabaseFetchError> {
        crate::db::get_profiles(db).await
    }
}
