//! Database operations for persisting Qobuz authentication and configuration.
//!
//! Provides functions to store and retrieve access tokens, app configurations,
//! secrets, and user settings required for Qobuz API authentication.

use moosicbox_json_utils::{ToValueType, database::DatabaseFetchError};
use switchy::database::{profiles::LibraryDatabase, query::FilterableQuery};

pub mod models;

use crate::db::models::QobuzConfig;

use self::models::{QobuzAppConfig, QobuzAppSecret};

/// Creates or updates a Qobuz app secret for a specific timezone.
///
/// # Errors
///
/// * If a database error occurs
pub async fn create_qobuz_app_secret(
    db: &LibraryDatabase,
    qobuz_bundle_id: u32,
    timezone: &str,
    secret: &str,
) -> Result<(), DatabaseFetchError> {
    db.upsert("qobuz_bundle_secrets")
        .where_eq("qobuz_bundle_id", qobuz_bundle_id)
        .where_eq("timezone", timezone)
        .value("qobuz_bundle_id", qobuz_bundle_id)
        .value("timezone", timezone)
        .value("secret", secret)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Creates or updates a Qobuz app configuration with bundle version and app ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn create_qobuz_app_config(
    db: &LibraryDatabase,
    bundle_version: &str,
    app_id: &str,
) -> Result<QobuzAppConfig, DatabaseFetchError> {
    Ok(db
        .upsert("qobuz_bundles")
        .value("bundle_version", bundle_version)
        .value("app_id", app_id)
        .where_eq("bundle_version", bundle_version)
        .execute_first(&**db)
        .await?
        .to_value_type()?)
}

/// Creates or updates a Qobuz user configuration with authentication credentials.
///
/// # Errors
///
/// * If a database error occurs
pub async fn create_qobuz_config(
    db: &LibraryDatabase,
    access_token: &str,
    user_id: u64,
    user_email: &str,
    user_public_id: &str,
) -> Result<(), DatabaseFetchError> {
    db.upsert("qobuz_config")
        .where_eq("user_id", user_id)
        .value("access_token", access_token)
        .value("user_id", user_id)
        .value("user_email", user_email)
        .value("user_public_id", user_public_id)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Retrieves all stored Qobuz app secrets from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_app_secrets(
    db: &LibraryDatabase,
) -> Result<Vec<QobuzAppSecret>, DatabaseFetchError> {
    Ok(db
        .select("qobuz_bundle_secrets")
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves the most recent Qobuz app configuration from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_app_config(
    db: &LibraryDatabase,
) -> Result<Option<QobuzAppConfig>, DatabaseFetchError> {
    let app_configs = db
        .select("qobuz_bundles")
        .execute(&**db)
        .await?
        .to_value_type()?;

    Ok(app_configs.last().cloned())
}

/// Retrieves the most recent Qobuz user configuration from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_config(
    db: &LibraryDatabase,
) -> Result<Option<QobuzConfig>, DatabaseFetchError> {
    let configs = db
        .select("qobuz_config")
        .execute(&**db)
        .await?
        .to_value_type()?;

    Ok(configs.last().cloned())
}

/// Retrieves the Qobuz access token from the stored user configuration.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_access_token(
    db: &LibraryDatabase,
) -> Result<Option<String>, DatabaseFetchError> {
    Ok(get_qobuz_config(db).await?.map(|c| c.access_token))
}
