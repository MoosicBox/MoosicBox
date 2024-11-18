#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::path::PathBuf;

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "db")]
pub mod db;

#[derive(Copy, Clone, Debug)]
pub enum AppType {
    App,
    Server,
}

impl From<AppType> for &str {
    fn from(value: AppType) -> Self {
        match value {
            AppType::App => "app",
            AppType::Server => "server",
        }
    }
}

impl std::fmt::Display for AppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).into())
    }
}

#[must_use]
pub fn get_config_dir_path() -> Option<PathBuf> {
    home::home_dir().map(|home| home.join(".local").join("moosicbox"))
}

#[must_use]
pub fn get_app_config_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_config_dir_path().map(|x| x.join(app_type.to_string()))
}

#[must_use]
pub fn get_profiles_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_app_config_dir_path(app_type).map(|x| x.join("profiles"))
}

#[must_use]
pub fn get_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    get_profiles_dir_path(app_type).map(|x| x.join(profile))
}

#[must_use]
pub fn get_cache_dir_path() -> Option<PathBuf> {
    get_config_dir_path().map(|config| config.join("cache"))
}

#[must_use]
pub fn make_config_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_config_dir_path() {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

#[must_use]
pub fn make_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    if let Some(path) = get_profile_dir_path(app_type, profile) {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

#[must_use]
pub fn make_cache_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_cache_dir_path() {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

#[cfg(feature = "test")]
#[must_use]
pub fn get_tests_dir_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "moosicbox_tests_{}",
        rand::Rng::gen::<usize>(&mut rand::thread_rng())
    ))
}

#[cfg(feature = "db")]
pub use db_impl::*;

#[cfg(feature = "db")]
mod db_impl {
    use moosicbox_database::{config::ConfigDatabase, DatabaseError};
    use moosicbox_json_utils::database::DatabaseFetchError;

    use crate::db::{models, GetOrInitServerIdentityError};

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
