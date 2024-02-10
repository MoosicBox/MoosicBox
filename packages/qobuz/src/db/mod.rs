use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{query::*, Database, DatabaseValue};
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
    db.upsert(
        "qobuz_bundle_secrets",
        &[
            (
                "qobuz_bundle_id",
                DatabaseValue::Number(qobuz_bundle_id as i64),
            ),
            ("timezone", DatabaseValue::String(timezone.to_string())),
        ],
        Some(&[
            where_eq(
                "qobuz_bundle_id",
                DatabaseValue::Number(qobuz_bundle_id as i64),
            ),
            where_eq("timezone", DatabaseValue::String(timezone.to_string())),
            where_eq("secret", DatabaseValue::String(secret.to_string())),
        ]),
    )
    .await?;

    Ok(())
}

pub async fn create_qobuz_app_config(
    db: &Box<dyn Database>,
    bundle_version: &str,
    app_id: &str,
) -> Result<QobuzAppConfig, DbError> {
    Ok(db
        .upsert(
            "qobuz_bundles",
            &[
                (
                    "bundle_version",
                    DatabaseValue::String(bundle_version.to_string()),
                ),
                ("app_id", DatabaseValue::String(app_id.to_string())),
            ],
            Some(&[where_eq(
                "bundle_version",
                DatabaseValue::String(bundle_version.to_string()),
            )]),
        )
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
    db.upsert(
        "qobuz_config",
        &[("user_id", DatabaseValue::Number(user_id as i64))],
        Some(&[
            where_eq(
                "access_token",
                DatabaseValue::String(access_token.to_string()),
            ),
            where_eq("user_id", DatabaseValue::Number(user_id as i64)),
            where_eq("user_email", DatabaseValue::String(user_email.to_string())),
            where_eq(
                "user_public_id",
                DatabaseValue::String(user_public_id.to_string()),
            ),
        ]),
    )
    .await?;

    Ok(())
}

pub async fn get_qobuz_app_secrets(db: &Box<dyn Database>) -> Result<Vec<QobuzAppSecret>, DbError> {
    Ok(db
        .select("qobuz_bundle_secrets", &["*"], None, None, None)
        .await?
        .to_value_type()?)
}

pub async fn get_qobuz_app_config(
    db: &Box<dyn Database>,
) -> Result<Option<QobuzAppConfig>, DbError> {
    let app_configs = db
        .select("qobuz_bundles", &["*"], None, None, None)
        .await?
        .to_value_type()?;

    Ok(app_configs.last().cloned())
}

pub async fn get_qobuz_config(db: &Box<dyn Database>) -> Result<Option<QobuzConfig>, DbError> {
    let configs = db
        .select("qobuz_config", &["*"], None, None, None)
        .await?
        .to_value_type()?;

    Ok(configs.last().cloned())
}

pub async fn get_qobuz_access_token(db: &Box<dyn Database>) -> Result<Option<String>, DbError> {
    Ok(get_qobuz_config(db).await?.map(|c| c.access_token.clone()))
}
