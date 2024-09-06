#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::path::PathBuf;

#[cfg(feature = "db")]
pub mod db;

pub fn get_config_dir_path() -> Option<PathBuf> {
    home::home_dir().map(|home| home.join(".local").join("moosicbox"))
}

pub fn get_cache_dir_path() -> Option<PathBuf> {
    get_config_dir_path().map(|config| config.join("cache"))
}

#[cfg(feature = "test")]
pub fn get_tests_dir_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "moosicbox_tests_{}",
        rand::Rng::gen::<usize>(&mut rand::thread_rng())
    ))
}
