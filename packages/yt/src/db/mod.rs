use moosicbox_database::{query::*, Database, DatabaseError};
use moosicbox_json_utils::ToValueType;
use thiserror::Error;

pub mod models;

use crate::db::models::YtConfig;

#[allow(clippy::too_many_arguments)]
pub async fn create_yt_config(
    db: &dyn Database,
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
    db.upsert("yt_config")
        .value("client_id", client_id)
        .value("access_token", access_token)
        .value("refresh_token", refresh_token)
        .value("client_name", client_name)
        .value("expires_in", expires_in)
        .value("scope", scope)
        .value("token_type", token_type)
        .value("user", user)
        .value("user_id", user_id)
        .where_eq("refresh_token", refresh_token)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn delete_yt_config(db: &dyn Database, refresh_token: &str) -> Result<(), DatabaseError> {
    db.delete("yt_config")
        .where_eq("refresh_token", refresh_token)
        .execute(db)
        .await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum YtConfigError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
    #[error("No configs available")]
    NoConfigsAvailable,
}

pub async fn get_yt_config(db: &dyn Database) -> Result<Option<YtConfig>, YtConfigError> {
    let mut configs = db.select("yt_config").execute(db).await?.to_value_type()?;

    if configs.is_empty() {
        return Err(YtConfigError::NoConfigsAvailable);
    }

    configs.sort_by(|a: &YtConfig, b: &YtConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

pub async fn get_yt_access_tokens(
    db: &dyn Database,
) -> Result<Option<(String, String)>, YtConfigError> {
    Ok(get_yt_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token.clone())))
}

pub async fn get_yt_access_token(db: &dyn Database) -> Result<Option<String>, YtConfigError> {
    Ok(get_yt_access_tokens(db).await?.map(|c| c.0))
}
