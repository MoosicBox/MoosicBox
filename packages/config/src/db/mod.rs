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

/// Retrieves the server identity from the database.
///
/// Returns `None` if no server identity has been initialized.
///
/// # Errors
///
/// * If a database query error occurs
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

/// Retrieves the server identity from the database, creating it if it doesn't exist.
///
/// This function ensures a unique server identity exists by creating one if needed.
/// The identity is generated using a random nanoid.
///
/// # Errors
///
/// * If a database query or insert error occurs
/// * If the inserted identity cannot be retrieved
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

/// Creates or retrieves a profile by name.
///
/// If a profile with the given name already exists, returns it. Otherwise, creates
/// a new profile.
///
/// # Errors
///
/// * If a database query or insert error occurs
/// * If the profile data cannot be parsed
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

/// Deletes a profile by name.
///
/// Returns the list of deleted profiles. If no profile with the given name exists,
/// returns an empty list.
///
/// # Errors
///
/// * If a database query or delete error occurs
/// * If the profile data cannot be parsed
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

/// Creates a new profile with the given name.
///
/// # Errors
///
/// * If a database query or insert error occurs
/// * If the profile data cannot be parsed
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

/// Retrieves all profiles from the database.
///
/// # Errors
///
/// * If a database query error occurs
/// * If the profile data cannot be parsed
pub(crate) async fn get_profiles(
    db: &ConfigDatabase,
) -> Result<Vec<models::Profile>, DatabaseFetchError> {
    Ok(db
        .select("profiles")
        .execute(&**db)
        .await?
        .to_value_type()?)
}
