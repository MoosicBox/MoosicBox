#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use models::{AudioZone, AudioZoneWithSession, CreateAudioZone, UpdateAudioZone};
use moosicbox_database::{Database, TryIntoDb};
use moosicbox_json_utils::database::DatabaseFetchError;

#[cfg(feature = "api")]
pub mod api;

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
    crate::db::create_audio_zone(db, zone)
        .await?
        .try_into_db(db)
        .await
}

pub async fn update_audio_zone(
    db: &dyn Database,
    update: UpdateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    crate::db::update_audio_zone(db, update)
        .await?
        .try_into_db(db)
        .await
}

pub async fn delete_audio_zone(
    db: &dyn Database,
    id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    Ok(if let Some(zone) = get_zone(db, id).await? {
        crate::db::delete_audio_zone(db, id).await?;

        Some(zone)
    } else {
        None
    })
}
