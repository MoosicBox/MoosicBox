#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use db::{audio_zone_try_from_db, audio_zone_with_session_try_from_db};
use moosicbox_audio_zone_models::{
    AudioZone, AudioZoneWithSession, CreateAudioZone, UpdateAudioZone,
};
use moosicbox_database::{config::ConfigDatabase, profiles::LibraryDatabase};
use moosicbox_json_utils::database::DatabaseFetchError;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "events")]
pub mod events;

pub mod db;
pub use moosicbox_audio_zone_models as models;

/// # Errors
///
/// * If fails to fetch `AudioZone`s from the database
pub async fn zones(db: &ConfigDatabase) -> Result<Vec<AudioZone>, DatabaseFetchError> {
    let mut results = vec![];
    let zones = crate::db::get_zones(db).await?;
    for zone in zones {
        results.push(audio_zone_try_from_db(zone, db.into()).await?);
    }
    Ok(results)
}

/// # Errors
///
/// * If fails to fetch `AudioZoneWithSession`s from the database
pub async fn zones_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Vec<AudioZoneWithSession>, DatabaseFetchError> {
    let mut results = vec![];
    let zones = crate::db::get_zone_with_sessions(config_db, library_db).await?;
    for zone in zones {
        results.push(audio_zone_with_session_try_from_db(zone, config_db.into()).await?);
    }
    Ok(results)
}

/// # Errors
///
/// * If fails to fetch the `AudioZone` from the database
pub async fn get_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    Ok(if let Some(zone) = crate::db::get_zone(db, id).await? {
        Some(audio_zone_try_from_db(zone, db.into()).await?)
    } else {
        None
    })
}

/// # Errors
///
/// * If fails to create the `AudioZone` in the database
pub async fn create_audio_zone(
    db: &ConfigDatabase,
    zone: &CreateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp =
        audio_zone_try_from_db(crate::db::create_audio_zone(db, zone).await?, db.into()).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_audio_zone updated_events", async move {
            if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(resp)
}

/// # Errors
///
/// * If fails to update the `AudioZone` in the database
pub async fn update_audio_zone(
    db: &ConfigDatabase,
    update: UpdateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp =
        audio_zone_try_from_db(crate::db::update_audio_zone(db, update).await?, db.into()).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_audio_zone updated_events", async move {
            if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(resp)
}

/// # Errors
///
/// * If fails to delete the `AudioZone` from the database
pub async fn delete_audio_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    let resp = if let Some(zone) = get_zone(db, id).await? {
        crate::db::delete_audio_zone(db, id).await?;

        Some(zone)
    } else {
        None
    };

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_audio_zone updated_events", async move {
            if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(resp)
}
