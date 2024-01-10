#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::path::PathBuf;

pub fn get_config_dir_path() -> Option<PathBuf> {
    home::home_dir().map(|home| home.join(".local").join("moosicbox"))
}

pub fn get_cache_dir_path() -> Option<PathBuf> {
    get_config_dir_path().map(|config| config.join("cache"))
}
