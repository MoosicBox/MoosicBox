use moosicbox_json_utils::ToValueType;
use switchy::database::{DatabaseError, profiles::LibraryDatabase, query::FilterableQuery};
use thiserror::Error;

/// Data models for Tidal database persistence.
pub mod models;

use crate::db::models::TidalConfig;

/// Creates or updates Tidal OAuth configuration in the database.
///
/// # Errors
///
/// * If a database error occurs
#[allow(clippy::too_many_arguments)]
pub async fn create_tidal_config(
    db: &LibraryDatabase,
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
        .where_eq("refresh_token", refresh_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Deletes a Tidal OAuth configuration from the database by refresh token.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_tidal_config(
    db: &LibraryDatabase,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete("tidal_config")
        .where_eq("refresh_token", refresh_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Errors that can occur when retrieving Tidal configuration from the database.
#[derive(Debug, Error)]
pub enum GetTidalConfigError {
    /// Database query or connection error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to parse configuration data from database result.
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
}

/// Retrieves the most recent Tidal OAuth configuration from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_tidal_config(
    db: &LibraryDatabase,
) -> Result<Option<TidalConfig>, GetTidalConfigError> {
    let mut configs = db
        .select("tidal_config")
        .execute(&**db)
        .await?
        .to_value_type()?;

    configs.sort_by(|a: &TidalConfig, b: &TidalConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

/// Retrieves the access and refresh tokens from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_tidal_access_tokens(
    db: &LibraryDatabase,
) -> Result<Option<(String, String)>, GetTidalConfigError> {
    Ok(get_tidal_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token)))
}

/// Retrieves only the access token from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_tidal_access_token(
    db: &LibraryDatabase,
) -> Result<Option<String>, GetTidalConfigError> {
    Ok(get_tidal_access_tokens(db).await?.map(|c| c.0))
}
