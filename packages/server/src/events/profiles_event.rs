use std::{collections::BTreeMap, sync::Arc};

use moosicbox_config::AppType;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::ApiSource;
use moosicbox_profiles::events::{
    BoxErrorSend, on_profiles_updated_event, trigger_profiles_updated_event,
};
use switchy_database::{Database, config::ConfigDatabase};

/// Errors that can occur when adding a new profile.
#[derive(Debug, thiserror::Error)]
pub enum AddProfileError {
    /// Database query error.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Tidal music service configuration error.
    #[cfg(feature = "tidal")]
    #[error(transparent)]
    TidalConfig(#[from] moosicbox_tidal::TidalConfigError),
    /// Qobuz music service configuration error.
    #[cfg(feature = "qobuz")]
    #[error(transparent)]
    QobuzConfig(#[from] moosicbox_qobuz::QobuzConfigError),
    /// `YouTube` Music service configuration error.
    #[cfg(feature = "yt")]
    #[error(transparent)]
    YtConfig(#[from] moosicbox_yt::YtConfigError),
}

/// Adds a new profile with its database connection and music API instances.
///
/// This function creates a profile-specific database, runs migrations, and initializes music
/// APIs for all enabled services (Library, Tidal, Qobuz, `YouTube` Music).
///
/// # Errors
///
/// * [`AddProfileError::DatabaseFetch`] - If database operations fail
/// * [`AddProfileError::TidalConfig`] - If Tidal API initialization fails (with `tidal` feature)
/// * [`AddProfileError::QobuzConfig`] - If Qobuz API initialization fails (with `qobuz` feature)
/// * [`AddProfileError::YtConfig`] - If `YouTube` Music API initialization fails (with `yt` feature)
///
/// # Panics
///
/// * If the profile library database path cannot be created (with `sqlite` feature, non-simulator mode)
/// * If database initialization fails
/// * If database migration fails (with `sqlite` or `postgres` features)
async fn add_profile(
    #[allow(unused)] app_type: AppType,
    profile: &str,
) -> Result<(), AddProfileError> {
    log::debug!("add_profile: app_type={app_type} profile={profile}");

    #[cfg(feature = "sqlite")]
    let library_db_profile_path = {
        if cfg!(feature = "simulator") {
            None
        } else {
            Some(
                crate::db::make_profile_library_db_path(app_type, profile)
                    .expect("Failed to get DB profile path"),
            )
        }
    };

    let library_db = switchy_database_connection::init(
        #[cfg(feature = "sqlite")]
        library_db_profile_path.as_deref(),
        None,
    )
    .await
    .expect("Failed to initialize database");

    #[cfg(any(feature = "sqlite", feature = "postgres"))]
    if let Err(e) = moosicbox_schema::migrate_library(&*library_db).await {
        moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
    }

    let library_database: Arc<Box<dyn Database>> = Arc::new(library_db);

    #[allow(unused)]
    let library_database =
        switchy_database::profiles::PROFILES.add_fetch(profile, library_database.clone());

    #[allow(clippy::redundant_clone)]
    #[cfg(feature = "library")]
    let library_music_api =
        moosicbox_library_music_api::LibraryMusicApi::new(library_database.clone());

    #[allow(unused_mut)]
    let mut apis_map: BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>> = BTreeMap::new();
    #[cfg(feature = "library")]
    apis_map.insert(
        ApiSource::library(),
        Arc::new(Box::new(library_music_api.cached())),
    );
    #[cfg(feature = "tidal")]
    apis_map.insert(
        moosicbox_tidal::API_SOURCE.clone(),
        Arc::new(Box::new(
            #[allow(clippy::redundant_clone)]
            moosicbox_tidal::TidalMusicApi::builder()
                .with_db(library_database.clone())
                .build()
                .await?
                .cached(),
        )),
    );
    #[cfg(feature = "qobuz")]
    apis_map.insert(
        moosicbox_qobuz::API_SOURCE.clone(),
        Arc::new(Box::new(
            #[allow(clippy::redundant_clone)]
            moosicbox_qobuz::QobuzMusicApi::builder()
                .with_db(library_database.clone())
                .build()
                .await?
                .cached(),
        )),
    );
    #[cfg(feature = "yt")]
    apis_map.insert(
        moosicbox_yt::API_SOURCE.clone(),
        Arc::new(Box::new(
            #[allow(clippy::redundant_clone)]
            moosicbox_yt::YtMusicApi::builder()
                .with_db(library_database.clone())
                .build()
                .await?
                .cached(),
        )),
    );
    moosicbox_music_api::profiles::PROFILES.add(profile.to_string(), Arc::new(apis_map));

    #[cfg(feature = "library")]
    moosicbox_library_music_api::profiles::PROFILES.add(profile.to_string(), library_database);

    Ok(())
}

/// Removes a profile and cleans up its resources.
///
/// This function removes the profile from all registries (database profiles, music API profiles,
/// library profiles) and optionally deletes the profile directory from disk.
///
/// # Errors
///
/// * If the profile directory deletion fails (with `sqlite` and without `postgres` features)
#[allow(clippy::unused_async)]
async fn remove_profile(
    #[allow(unused)] app_type: AppType,
    profile: &str,
) -> Result<(), std::io::Error> {
    log::debug!("remove_profile: app_type={app_type} profile={profile}");

    switchy_database::profiles::PROFILES.remove(profile);
    moosicbox_music_api::profiles::PROFILES.remove(profile);
    #[cfg(feature = "library")]
    moosicbox_library_music_api::profiles::PROFILES.remove(profile);

    #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
    if let Some(path) = moosicbox_config::get_profile_dir_path(app_type, profile) {
        tokio::fs::remove_dir_all(path).await?;
    }

    Ok(())
}

/// Initializes profile management and loads existing profiles.
///
/// This function sets up event handlers for profile additions and removals, then loads all
/// existing profiles from the database. Each profile gets its own database connection and
/// music API instances for enabled services (Tidal, Qobuz, `YouTube` Music, etc.).
///
/// # Errors
///
/// * If profile retrieval from the database fails
/// * If profile database initialization fails
/// * If music API initialization fails for any enabled service
pub async fn init(
    #[allow(unused)] app_type: AppType,
    config_db: ConfigDatabase,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    on_profiles_updated_event(move |added, removed| {
        let added = added.to_vec();
        let removed = removed.to_vec();

        Box::pin(async move {
            for profile in &removed {
                remove_profile(app_type, profile)
                    .await
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
            }

            for profile in &added {
                add_profile(app_type, profile)
                    .await
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
            }

            Ok(())
        })
    })
    .await;

    let profiles = moosicbox_config::get_profiles(&config_db)
        .await
        .map_err(|e| vec![Box::new(e) as BoxErrorSend])?;

    #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
    for profile in &profiles {
        if crate::db::get_profile_library_db_path(app_type, &profile.name)
            .is_none_or(|x| !x.is_file())
        {
            moosicbox_config::delete_profile(&config_db, &profile.name)
                .await
                .map_err(|e| vec![Box::new(e) as BoxErrorSend])?;
        }
    }

    trigger_profiles_updated_event(
        profiles.iter().map(|x| x.name.clone()).collect::<Vec<_>>(),
        vec![],
    )
    .await?;

    Ok(())
}
