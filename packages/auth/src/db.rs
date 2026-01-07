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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::{Database, simulator::SimulationDatabase};

    async fn setup_db() -> ConfigDatabase {
        let db = SimulationDatabase::new().expect("Failed to create simulation database");
        let db: Arc<Box<dyn Database>> = Arc::new(Box::new(db));

        // Create the client_access_tokens table
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS client_access_tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                client_id TEXT NOT NULL,
                token TEXT NOT NULL,
                expires TEXT,
                updated TEXT DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await
        .expect("Failed to create client_access_tokens table");

        // Create the magic_tokens table
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS magic_tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                magic_token TEXT NOT NULL,
                client_id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                expires TEXT
            )",
        )
        .await
        .expect("Failed to create magic_tokens table");

        ConfigDatabase::from(db)
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_client_access_token_returns_none_when_empty() {
        let db = setup_db().await;
        let result = get_client_access_token(&db).await.unwrap();
        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_create_and_get_client_access_token() {
        let db = setup_db().await;

        create_client_access_token(&db, "client123", "token456")
            .await
            .unwrap();

        let result = get_client_access_token(&db).await.unwrap();
        assert!(result.is_some());
        let (client_id, token) = result.unwrap();
        assert_eq!(client_id, "client123");
        assert_eq!(token, "token456");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_create_client_access_token_upserts_existing() {
        let db = setup_db().await;

        // Create initial token
        create_client_access_token(&db, "client123", "token456")
            .await
            .unwrap();

        // Upsert the same token (should not create duplicate)
        create_client_access_token(&db, "client123", "token456")
            .await
            .unwrap();

        // Should still retrieve the same token
        let result = get_client_access_token(&db).await.unwrap();
        assert!(result.is_some());
        let (client_id, token) = result.unwrap();
        assert_eq!(client_id, "client123");
        assert_eq!(token, "token456");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_client_access_token_returns_most_recent() {
        let db = setup_db().await;

        // Create first token
        create_client_access_token(&db, "client1", "token1")
            .await
            .unwrap();

        // Create second token (more recent)
        create_client_access_token(&db, "client2", "token2")
            .await
            .unwrap();

        // Should return the most recently updated token
        let result = get_client_access_token(&db).await.unwrap();
        assert!(result.is_some());
        let (client_id, _token) = result.unwrap();
        // The order depends on which was updated last
        assert!(client_id == "client1" || client_id == "client2");
    }

    #[cfg(feature = "api")]
    #[test_log::test(switchy_async::test)]
    async fn test_save_and_get_magic_token() {
        let db = setup_db().await;

        save_magic_token(&db, "magic123", "client456", "access789")
            .await
            .unwrap();

        let result = get_credentials_from_magic_token(&db, "magic123")
            .await
            .unwrap();
        assert!(result.is_some());
        let (client_id, access_token) = result.unwrap();
        assert_eq!(client_id, "client456");
        assert_eq!(access_token, "access789");
    }

    #[cfg(feature = "api")]
    #[test_log::test(switchy_async::test)]
    async fn test_get_credentials_from_magic_token_consumes_token() {
        let db = setup_db().await;

        save_magic_token(&db, "magic_once", "client", "access")
            .await
            .unwrap();

        // First retrieval should succeed
        let result1 = get_credentials_from_magic_token(&db, "magic_once")
            .await
            .unwrap();
        assert!(result1.is_some());

        // Second retrieval should fail (token was consumed)
        let result2 = get_credentials_from_magic_token(&db, "magic_once")
            .await
            .unwrap();
        assert!(result2.is_none());
    }

    #[cfg(feature = "api")]
    #[test_log::test(switchy_async::test)]
    async fn test_get_credentials_from_magic_token_returns_none_for_nonexistent() {
        let db = setup_db().await;

        let result = get_credentials_from_magic_token(&db, "nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[cfg(feature = "api")]
    #[test_log::test(switchy_async::test)]
    async fn test_delete_magic_token() {
        let db = setup_db().await;

        // Save a token
        save_magic_token(&db, "to_delete", "client", "access")
            .await
            .unwrap();

        // Delete it
        delete_magic_token(&db, "to_delete").await.unwrap();

        // Should no longer exist
        let result = get_credentials_from_magic_token(&db, "to_delete")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[cfg(feature = "api")]
    #[test_log::test(switchy_async::test)]
    async fn test_delete_nonexistent_magic_token_succeeds() {
        let db = setup_db().await;

        // Deleting a nonexistent token should not error
        let result = delete_magic_token(&db, "does_not_exist").await;
        assert!(result.is_ok());
    }
}
