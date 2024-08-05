#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use models::AudioZone;
use moosicbox_database::Database;
use moosicbox_json_utils::{database::DatabaseFetchError, ToValueType as _};

#[cfg(feature = "api")]
pub mod api;

pub mod models;

pub async fn zones(db: &dyn Database) -> Result<Vec<AudioZone>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .execute(db)
        .await?
        .to_value_type()?)
}
