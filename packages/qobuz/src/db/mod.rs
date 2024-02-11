use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{query::*, Database};
use moosicbox_json_utils::ToValueType;

pub mod models;

use crate::db::models::QobuzConfig;

use self::models::{QobuzAppConfig, QobuzAppSecret};

pub async fn create_qobuz_app_secret(
    db: &Box<dyn Database>,
    qobuz_bundle_id: u32,
    timezone: &str,
    secret: &str,
) -> Result<(), DbError> {
    db.upsert("qobuz_bundle_secrets")
        .filter(where_eq("qobuz_bundle_id", qobuz_bundle_id))
        .filter(where_eq("timezone", timezone))
        .value("qobuz_bundle_id", qobuz_bundle_id)
        .value("timezone", timezone)
        .value("secret", secret)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn create_qobuz_app_config(
    db: &Box<dyn Database>,
    bundle_version: &str,
    app_id: &str,
) -> Result<QobuzAppConfig, DbError> {
    Ok(db
        .upsert("qobuz_bundles")
        .value("bundle_version", bundle_version)
        .value("app_id", app_id)
        .filter(where_eq("bundle_version", bundle_version))
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn create_qobuz_config(
    db: &Box<dyn Database>,
    access_token: &str,
    user_id: u64,
    user_email: &str,
    user_public_id: &str,
) -> Result<(), DbError> {
    db.upsert("qobuz_config")
        .filter(where_eq("user_id", user_id))
        .value("access_token", access_token)
        .value("user_id", user_id)
        .value("user_email", user_email)
        .value("user_public_id", user_public_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_qobuz_app_secrets(db: &Box<dyn Database>) -> Result<Vec<QobuzAppSecret>, DbError> {
    Ok(db
        .select("qobuz_bundle_secrets")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_qobuz_app_config(
    db: &Box<dyn Database>,
) -> Result<Option<QobuzAppConfig>, DbError> {
    let app_configs = db
        .select("qobuz_bundles")
        .execute(db)
        .await?
        .to_value_type()?;

    Ok(app_configs.last().cloned())
}

pub async fn get_qobuz_config(db: &Box<dyn Database>) -> Result<Option<QobuzConfig>, DbError> {
    let configs = db
        .select("qobuz_config")
        .execute(db)
        .await?
        .to_value_type()?;

    Ok(configs.last().cloned())
}

pub async fn get_qobuz_access_token(db: &Box<dyn Database>) -> Result<Option<String>, DbError> {
    Ok(get_qobuz_config(db).await?.map(|c| c.access_token.clone()))
}
