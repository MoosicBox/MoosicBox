use moosicbox_database::{profiles::LibraryDatabase, query::FilterableQuery};
use moosicbox_json_utils::{database::DatabaseFetchError, ToValueType};

pub mod models;

use crate::db::models::QobuzConfig;

use self::models::{QobuzAppConfig, QobuzAppSecret};

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
        .execute(db)
        .await?;

    Ok(())
}

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
        .execute_first(db)
        .await?
        .to_value_type()?)
}

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
        .execute(db)
        .await?;

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_app_secrets(
    db: &LibraryDatabase,
) -> Result<Vec<QobuzAppSecret>, DatabaseFetchError> {
    Ok(db
        .select("qobuz_bundle_secrets")
        .execute(db)
        .await?
        .to_value_type()?)
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_app_config(
    db: &LibraryDatabase,
) -> Result<Option<QobuzAppConfig>, DatabaseFetchError> {
    let app_configs = db
        .select("qobuz_bundles")
        .execute(db)
        .await?
        .to_value_type()?;

    Ok(app_configs.last().cloned())
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_config(
    db: &LibraryDatabase,
) -> Result<Option<QobuzConfig>, DatabaseFetchError> {
    let configs = db
        .select("qobuz_config")
        .execute(db)
        .await?
        .to_value_type()?;

    Ok(configs.last().cloned())
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_qobuz_access_token(
    db: &LibraryDatabase,
) -> Result<Option<String>, DatabaseFetchError> {
    Ok(get_qobuz_config(db).await?.map(|c| c.access_token))
}
