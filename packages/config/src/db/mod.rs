//! Database operations for `MoosicBox` configuration.
//!
//! This module provides database-backed storage for configuration data,
//! including server identity and profile management.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "db")]
//! # async fn example(db: &switchy_database::config::ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_config::get_or_init_server_identity;
//!
//! // Get or create a unique server identity
//! let identity = get_or_init_server_identity(db).await?;
//! println!("Server identity: {}", identity);
//! # Ok(())
//! # }
//! ```

use moosicbox_json_utils::{ToValueType as _, database::DatabaseFetchError};
use nanoid::nanoid;
use switchy_database::{DatabaseError, config::ConfigDatabase, query::FilterableQuery as _};
use thiserror::Error;

pub mod models;

/// Error type for server identity operations.
#[derive(Debug, Error)]
pub enum GetOrInitServerIdentityError {
    /// Database operation failed
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to retrieve or initialize server identity
    #[error("Failed to get server identity")]
    Failed,
}

pub(crate) async fn get_server_identity(
    db: &ConfigDatabase,
) -> Result<Option<String>, DatabaseError> {
    Ok(db
        .select("identity")
        .execute_first(&**db)
        .await?
        .and_then(|x| {
            x.get("id")
                .and_then(|x| x.as_str().map(std::string::ToString::to_string))
        }))
}

pub(crate) async fn get_or_init_server_identity(
    db: &ConfigDatabase,
) -> Result<String, GetOrInitServerIdentityError> {
    if let Some(identity) = get_server_identity(db).await? {
        Ok(identity)
    } else {
        let id = nanoid!();

        db.insert("identity")
            .value("id", id)
            .execute(&**db)
            .await?
            .get("id")
            .and_then(|x| x.as_str().map(std::string::ToString::to_string))
            .ok_or(GetOrInitServerIdentityError::Failed)
    }
}

#[allow(unused)]
pub(crate) async fn upsert_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<models::Profile, DatabaseFetchError> {
    Ok(db
        .upsert("profiles")
        .where_eq("name", name)
        .value("name", name)
        .execute_first(&**db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn delete_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<Vec<models::Profile>, DatabaseFetchError> {
    Ok(db
        .delete("profiles")
        .where_eq("name", name)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn create_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<models::Profile, DatabaseFetchError> {
    Ok(db
        .insert("profiles")
        .value("name", name)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn get_profiles(
    db: &ConfigDatabase,
) -> Result<Vec<models::Profile>, DatabaseFetchError> {
    Ok(db
        .select("profiles")
        .execute(&**db)
        .await?
        .to_value_type()?)
}
