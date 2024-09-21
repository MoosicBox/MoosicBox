use std::{collections::HashMap, sync::Arc};

use moosicbox_config::AppType;
use moosicbox_core::sqlite::models::ApiSource;
use moosicbox_database::{config::ConfigDatabase, Database};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::MusicApi;
use moosicbox_profiles::events::{
    on_profiles_updated_event, trigger_profiles_updated_event, BoxErrorSend,
};

async fn add_profile(
    #[allow(unused)] app_type: AppType,
    profile: &str,
) -> Result<(), DatabaseFetchError> {
    #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
    let library_db_profile_path = {
        let path = crate::db::make_profile_library_db_path(app_type, profile)
            .expect("Failed to get DB profile path");

        let path_str = path.to_str().expect("Failed to get DB path_str");
        if let Err(e) = moosicbox_schema::migrate_library(path_str) {
            moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
        };

        path
    };

    let library_db = moosicbox_database_connection::init(
        #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
        &library_db_profile_path,
        None,
    )
    .await
    .expect("Failed to initialize database");

    let library_database: Arc<Box<dyn Database>> = Arc::new(library_db);

    #[allow(unused)]
    let library_database =
        moosicbox_database::profiles::PROFILES.fetch_add(profile, library_database.clone());

    #[cfg(feature = "library")]
    let library_music_api = moosicbox_library::LibraryMusicApi::new(library_database.clone());

    #[allow(unused_mut)]
    let mut apis_map: HashMap<ApiSource, Arc<Box<dyn MusicApi>>> = HashMap::new();
    #[cfg(feature = "library")]
    apis_map.insert(
        ApiSource::Library,
        Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
            library_music_api,
        ))),
    );
    #[cfg(feature = "tidal")]
    apis_map.insert(
        ApiSource::Tidal,
        Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_tidal::TidalMusicApi::new(library_database.clone()),
        ))),
    );
    #[cfg(feature = "qobuz")]
    apis_map.insert(
        ApiSource::Qobuz,
        Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_qobuz::QobuzMusicApi::new(library_database.clone()),
        ))),
    );
    #[cfg(feature = "yt")]
    apis_map.insert(
        ApiSource::Yt,
        Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_yt::YtMusicApi::new(library_database.clone()),
        ))),
    );
    moosicbox_music_api::profiles::PROFILES.add(profile.to_string(), Arc::new(apis_map));

    #[cfg(feature = "library")]
    moosicbox_library::profiles::PROFILES.add(profile.to_string(), library_database);

    Ok(())
}

fn remove_profile(#[allow(unused)] app_type: AppType, profile: &str) {
    moosicbox_database::profiles::PROFILES.remove(profile);
    moosicbox_music_api::profiles::PROFILES.remove(profile);
    #[cfg(feature = "library")]
    moosicbox_library::profiles::PROFILES.remove(profile);
}

pub async fn init(
    #[allow(unused)] app_type: AppType,
    config_db: ConfigDatabase,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    on_profiles_updated_event(move |added, removed| {
        let added = added.to_vec();
        let removed = removed.to_vec();

        Box::pin(async move {
            for profile in &removed {
                remove_profile(app_type, profile);
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
        if !crate::db::get_profile_library_db_path(app_type, &profile.name)
            .is_some_and(|x| x.is_file())
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
