#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use models::{AudioZone, AudioZoneWithSession, CreateAudioZone, UpdateAudioZone};
use moosicbox_database::{Database, TryIntoDb};
use moosicbox_json_utils::database::DatabaseFetchError;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "events")]
pub mod events;

pub mod db;
pub mod models;

pub async fn zones(db: &dyn Database) -> Result<Vec<AudioZone>, DatabaseFetchError> {
    crate::db::get_zones(db).await?.try_into_db(db).await
}

pub async fn zones_with_sessions(
    db: &dyn Database,
) -> Result<Vec<AudioZoneWithSession>, DatabaseFetchError> {
    crate::db::get_zone_with_sessions(db)
        .await?
        .try_into_db(db)
        .await
}

pub async fn get_zone(db: &dyn Database, id: u64) -> Result<Option<AudioZone>, DatabaseFetchError> {
    crate::db::get_zone(db, id).await?.try_into_db(db).await
}

pub async fn create_audio_zone(
    db: &dyn Database,
    zone: &CreateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp = crate::db::create_audio_zone(db, zone)
        .await?
        .try_into_db(db)
        .await?;

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

pub async fn update_audio_zone(
    db: &dyn Database,
    update: UpdateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp = crate::db::update_audio_zone(db, update)
        .await?
        .try_into_db(db)
        .await?;

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

pub async fn delete_audio_zone(
    db: &dyn Database,
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
