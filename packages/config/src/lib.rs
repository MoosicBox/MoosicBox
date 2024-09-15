#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::path::PathBuf;

#[cfg(feature = "db")]
pub mod db;

#[derive(Copy, Clone, Debug)]
pub enum AppType {
    App,
    Server,
    TunnelServer,
}

impl From<AppType> for &str {
    fn from(value: AppType) -> Self {
        match value {
            AppType::App => "app",
            AppType::Server => "server",
            AppType::TunnelServer => "tunnel_server",
        }
    }
}

impl std::fmt::Display for AppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).into())
    }
}

pub fn get_config_dir_path() -> Option<PathBuf> {
    home::home_dir().map(|home| home.join(".local").join("moosicbox"))
}

pub fn get_profiles_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_config_dir_path().map(|x| x.join(app_type.to_string()).join("profiles"))
}

pub fn get_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    get_profiles_dir_path(app_type).map(|x| x.join(profile))
}

pub fn get_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    get_profile_dir_path(app_type, profile).map(|x| x.join("db"))
}

pub fn get_cache_dir_path() -> Option<PathBuf> {
    get_config_dir_path().map(|config| config.join("cache"))
}

pub fn make_config_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_config_dir_path() {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

pub fn make_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    if let Some(path) = get_profile_dir_path(app_type, profile) {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

pub fn make_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    if let Some(path) = get_profile_db_dir_path(app_type, profile) {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

pub fn make_cache_dir_path() -> Option<PathBuf> {
    if let Some(path) = get_cache_dir_path() {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path);
        }
    }

    None
}

#[cfg(feature = "test")]
pub fn get_tests_dir_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "moosicbox_tests_{}",
        rand::Rng::gen::<usize>(&mut rand::thread_rng())
    ))
}
