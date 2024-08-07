#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use db::models::AudioZoneModel;
use models::{AudioZone, CreateAudioZone};
use moosicbox_database::{Database, TryIntoDb};
use moosicbox_json_utils::database::DatabaseFetchError;

#[cfg(feature = "api")]
pub mod api;

pub mod db;
pub mod models;

pub async fn zones(db: &dyn Database) -> Result<Vec<AudioZone>, DatabaseFetchError> {
    crate::db::get_zones(db).await?.try_into_db(db).await
}

pub async fn create_audio_zone(
    db: &dyn Database,
    zone: &CreateAudioZone,
) -> Result<AudioZoneModel, DatabaseFetchError> {
    crate::db::create_audio_zone(db, zone).await
}
