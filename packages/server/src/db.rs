use moosicbox_config::{get_app_config_dir_path, get_profile_dir_path, AppType};

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_config_db_dir_path(app_type: AppType) -> Option<std::path::PathBuf> {
    get_app_config_dir_path(app_type).map(|x| x.join("db"))
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_config_db_path(app_type: AppType) -> Option<std::path::PathBuf> {
    get_config_db_dir_path(app_type).map(|x| x.join("config.db"))
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn make_config_db_path(app_type: AppType) -> Option<std::path::PathBuf> {
    if let Some(path) = get_config_db_path(app_type) {
        if path.is_file()
            || path
                .parent()
                .is_some_and(|x| x.is_dir() || std::fs::create_dir_all(x).is_ok())
        {
            return Some(path);
        }
    }

    None
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<std::path::PathBuf> {
    get_profile_dir_path(app_type, profile).map(|x| x.join("db"))
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn get_profile_library_db_path(app_type: AppType, profile: &str) -> Option<std::path::PathBuf> {
    get_profile_db_dir_path(app_type, profile).map(|x| x.join("library.db"))
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[must_use]
pub fn make_profile_library_db_path(
    app_type: AppType,
    profile: &str,
) -> Option<std::path::PathBuf> {
    if let Some(path) = get_profile_library_db_path(app_type, profile) {
        if path.is_file()
            || path
                .parent()
                .is_some_and(|x| x.is_dir() || std::fs::create_dir_all(x).is_ok())
        {
            return Some(path);
        }
    }

    None
}
