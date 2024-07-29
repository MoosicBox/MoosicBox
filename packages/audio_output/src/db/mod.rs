use moosicbox_database::{query::*, Database, DatabaseError};
use moosicbox_json_utils::{ParseError, ToValueType as _};
use thiserror::Error;

use self::models::AudioOutputModel;

pub mod models;

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub async fn create_download_location(
    db: &dyn Database,
    model: AudioOutputModel,
) -> Result<(), DbError> {
    db.upsert("audio_outputs")
        .where_eq("id", &model.id)
        .value("id", model.id)
        .value("name", model.name)
        .value("spec_rate", model.spec_rate)
        .value("spec_channels", model.spec_channels)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_audio_outputs(db: &dyn Database) -> Result<Vec<AudioOutputModel>, DbError> {
    Ok(db
        .select("audio_outputs")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_audio_output(
    db: &dyn Database,
    id: String,
) -> Result<Option<AudioOutputModel>, DbError> {
    Ok(db
        .select("audio_outputs")
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}
