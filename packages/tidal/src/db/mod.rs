use moosicbox_database::{where_eq, Database, DatabaseError, DatabaseValue};
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
    db.upsert(
        "tidal_config",
        &[
            ("client_id", DatabaseValue::String(client_id.to_string())),
            (
                "access_token",
                DatabaseValue::String(access_token.to_string()),
            ),
            (
                "refresh_token",
                DatabaseValue::String(refresh_token.to_string()),
            ),
            (
                "client_name",
                DatabaseValue::String(client_name.to_string()),
            ),
            ("expires_in", DatabaseValue::Number(expires_in as i64)),
            ("scope", DatabaseValue::String(scope.to_string())),
            ("token_type", DatabaseValue::String(token_type.to_string())),
            ("user", DatabaseValue::String(user.to_string())),
            ("user_id", DatabaseValue::Number(user_id as i64)),
        ],
        Some(&[where_eq(
            "refresh_token",
            DatabaseValue::String(refresh_token.to_string()),
        )]),
        None,
    )
    .await?;

    Ok(())
}

pub async fn delete_tidal_config(
    db: &Box<dyn Database>,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete(
        "tidal_config",
        Some(&[where_eq(
            "refresh_token",
            DatabaseValue::String(refresh_token.to_string()),
        )]),
        None,
    )
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
        .select("tidal_config", &["*"], None, None, None)
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
