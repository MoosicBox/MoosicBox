//! Database operations for `YouTube` Music configuration.
//!
//! This module provides database access functions for storing and retrieving
//! `YouTube` Music OAuth tokens and user configuration.

use moosicbox_json_utils::ToValueType;
use switchy_database::{DatabaseError, profiles::LibraryDatabase, query::FilterableQuery};
use thiserror::Error;

/// Data models for `YouTube` Music database entities.
///
/// Contains the `YtConfig` type for storing `YouTube` Music authentication and configuration.
pub mod models;

use crate::db::models::YtConfig;

/// Creates or updates `YouTube` Music configuration in the database.
///
/// Stores OAuth tokens and user information for `YouTube` Music authentication.
///
/// # Errors
///
/// * `DatabaseError` - If a database operation fails
#[allow(clippy::too_many_arguments)]
pub async fn create_yt_config(
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
        .execute(&**db)
        .await?;

    Ok(())
}

/// Deletes `YouTube` Music configuration from the database.
///
/// # Errors
///
/// * `DatabaseError` - If a database operation fails
pub async fn delete_yt_config(
    db: &LibraryDatabase,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete("yt_config")
        .where_eq("refresh_token", refresh_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Errors that can occur when retrieving `YouTube` Music configuration.
#[derive(Debug, Error)]
pub enum GetYtConfigError {
    /// Database operation failed
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// JSON parsing failed
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
    /// No `YouTube` Music configuration is available in the database
    #[error("No configs available")]
    NoConfigsAvailable,
}

/// Retrieves `YouTube` Music configuration from the database.
///
/// Returns the most recent configuration based on the `issued_at` timestamp.
///
/// # Errors
///
/// * `GetYtConfigError::Database` - If a database operation fails
/// * `GetYtConfigError::Parse` - If parsing the configuration fails
/// * `GetYtConfigError::NoConfigsAvailable` - If no configuration exists
pub async fn get_yt_config(db: &LibraryDatabase) -> Result<Option<YtConfig>, GetYtConfigError> {
    let mut configs = db
        .select("yt_config")
        .execute(&**db)
        .await?
        .to_value_type()?;

    if configs.is_empty() {
        return Err(GetYtConfigError::NoConfigsAvailable);
    }

    configs.sort_by(|a: &YtConfig, b: &YtConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

/// Retrieves `YouTube` Music access and refresh tokens from the database.
///
/// Returns a tuple of `(access_token, refresh_token)` if configuration exists.
///
/// # Errors
///
/// * `GetYtConfigError::Database` - If a database operation fails
/// * `GetYtConfigError::Parse` - If parsing the configuration fails
/// * `GetYtConfigError::NoConfigsAvailable` - If no configuration exists
pub async fn get_yt_access_tokens(
    db: &LibraryDatabase,
) -> Result<Option<(String, String)>, GetYtConfigError> {
    Ok(get_yt_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token)))
}

/// Retrieves the `YouTube` Music access token from the database.
///
/// # Errors
///
/// * `GetYtConfigError::Database` - If a database operation fails
/// * `GetYtConfigError::Parse` - If parsing the configuration fails
/// * `GetYtConfigError::NoConfigsAvailable` - If no configuration exists
pub async fn get_yt_access_token(db: &LibraryDatabase) -> Result<Option<String>, GetYtConfigError> {
    Ok(get_yt_access_tokens(db).await?.map(|c| c.0))
}
