use moosicbox_config::{AppType, get_profile_dir_path};

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
