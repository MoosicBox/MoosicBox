//! Database operations for authentication token management.
//!
//! This module provides internal database functions for managing client access tokens
//! and magic tokens. All functions interact with the `ConfigDatabase` to persist and
//! retrieve authentication credentials.

use moosicbox_json_utils::{ParseError, ToValueType, database::DatabaseFetchError};
use switchy_database::{
    DatabaseValue, boxed,
    config::ConfigDatabase,
    query::{FilterableQuery, SortDirection, where_eq, where_gt},
};

/// Retrieves the most recent valid client access token from the database.
///
/// Returns the client ID and access token if a valid (non-expired) token exists.
/// Tokens are sorted by update time, with the most recent returned first.
///
/// # Errors
///
/// * Database query fails
/// * Token data cannot be parsed to the expected type
pub async fn get_client_access_token(
    db: &ConfigDatabase,
) -> Result<Option<(String, String)>, DatabaseFetchError> {
    Ok(db
        .select("client_access_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .sort("updated", SortDirection::Desc)
        .execute_first(&**db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?)
}

/// Creates or updates a client access token in the database.
///
/// Stores the client ID and access token pair, creating a new record or updating
/// an existing one if a matching token and client ID already exist.
///
/// # Errors
///
/// * Database upsert operation fails
pub async fn create_client_access_token(
    db: &ConfigDatabase,
    client_id: &str,
    token: &str,
) -> Result<(), DatabaseFetchError> {
    db.upsert("client_access_tokens")
        .where_eq("token", token)
        .where_eq("client_id", client_id)
        .value("token", token)
        .value("client_id", client_id)
        .execute_first(&**db)
        .await?;

    Ok(())
}

/// Deletes a magic token from the database.
///
/// Removes the magic token record after it has been consumed or expired.
///
/// # Errors
///
/// * Database delete operation fails
#[cfg(feature = "api")]
pub async fn delete_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
) -> Result<(), DatabaseFetchError> {
    db.delete("magic_tokens")
        .where_eq("magic_token", magic_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Retrieves and consumes credentials from a magic token.
///
/// Looks up the client ID and access token associated with a valid (non-expired)
/// magic token. If found, the magic token is deleted from the database and the
/// credentials are returned.
///
/// # Errors
///
/// * Database query fails
/// * Token deletion fails
/// * Credential data cannot be parsed to the expected type
#[cfg(feature = "api")]
pub async fn get_credentials_from_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
) -> Result<Option<(String, String)>, DatabaseFetchError> {
    if let Some((client_id, access_token)) = db
        .select("magic_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .where_eq("magic_token", magic_token)
        .execute_first(&**db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("access_token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?
    {
        delete_magic_token(db, magic_token).await?;

        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

/// Saves a magic token to the database with associated credentials.
///
/// Creates or updates a magic token record with the provided client ID and access token.
/// The token is set to expire in 24 hours from creation.
///
/// # Errors
///
/// * Database upsert operation fails
#[cfg(feature = "api")]
pub async fn save_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
    client_id: &str,
    access_token: &str,
) -> Result<(), DatabaseFetchError> {
    db.upsert("magic_tokens")
        .where_eq("magic_token", magic_token)
        .where_eq("access_token", access_token)
        .where_eq("client_id", client_id)
        .value("magic_token", magic_token)
        .value("access_token", access_token)
        .value("client_id", client_id)
        .value("expires", DatabaseValue::now().plus_days(1))
        .execute_first(&**db)
        .await?;

    Ok(())
}
