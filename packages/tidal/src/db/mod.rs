use moosicbox_database::{query::*, Database, DatabaseError};
use moosicbox_json_utils::ToValueType;
use thiserror::Error;

pub mod models;

use crate::db::models::TidalConfig;

#[allow(clippy::too_many_arguments)]
pub async fn create_tidal_config(
    db: &Box<dyn Database>,
    client_id: &str,
    access_token: &str,
    refresh_token: &str,
    client_name: &str,
    expires_in: u32,
    scope: &str,
    token_type: &str,
    user: &str,
    user_id: u32,
) -> Result<(), DatabaseError> {
    db.upsert("tidal_config")
        .value("client_id", client_id)
        .value("access_token", access_token)
        .value("refresh_token", refresh_token)
        .value("client_name", client_name)
        .value("expires_in", expires_in)
        .value("scope", scope)
        .value("token_type", token_type)
        .value("user", user)
        .value("user_id", user_id)
        .filter(where_eq("refresh_token", refresh_token))
        .execute(db)
        .await?;

    Ok(())
}

pub async fn delete_tidal_config(
    db: &Box<dyn Database>,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete("tidal_config")
        .filter(where_eq("refresh_token", refresh_token))
        .execute(db)
        .await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum TidalConfigError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
    #[error("No configs available")]
    NoConfigsAvailable,
}

pub async fn get_tidal_config(
    db: &Box<dyn Database>,
) -> Result<Option<TidalConfig>, TidalConfigError> {
    let mut configs = db
        .select("tidal_config")
        .execute(db)
        .await?
        .to_value_type()?;

    if configs.is_empty() {
        return Err(TidalConfigError::NoConfigsAvailable);
    }

    configs.sort_by(|a: &TidalConfig, b: &TidalConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

pub async fn get_tidal_access_tokens(
    db: &Box<dyn Database>,
) -> Result<Option<(String, String)>, TidalConfigError> {
    Ok(get_tidal_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token.clone())))
}

pub async fn get_tidal_access_token(
    db: &Box<dyn Database>,
) -> Result<Option<String>, TidalConfigError> {
    Ok(get_tidal_access_tokens(db).await?.map(|c| c.0))
}
