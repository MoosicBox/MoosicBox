use moosicbox_core::sqlite::db::{select, upsert, DbError, SqliteValue};
use rusqlite::Connection;

pub mod models;

use crate::db::models::QobuzConfig;

use self::models::{QobuzAppConfig, QobuzAppSecret};

pub fn create_qobuz_app_secret(
    db: &Connection,
    qobuz_bundle_id: u32,
    timezone: &str,
    secret: &str,
) -> Result<(), DbError> {
    upsert::<QobuzAppSecret>(
        db,
        "qobuz_bundle_secrets",
        vec![
            (
                "qobuz_bundle_id",
                SqliteValue::Number(qobuz_bundle_id as i64),
            ),
            ("timezone", SqliteValue::String(timezone.to_string())),
        ],
        vec![
            (
                "qobuz_bundle_id",
                SqliteValue::Number(qobuz_bundle_id as i64),
            ),
            ("timezone", SqliteValue::String(timezone.to_string())),
            ("secret", SqliteValue::String(secret.to_string())),
        ],
    )?;

    Ok(())
}

pub fn create_qobuz_app_config(
    db: &Connection,
    bundle_version: &str,
    app_id: &str,
) -> Result<QobuzAppConfig, DbError> {
    upsert::<QobuzAppConfig>(
        db,
        "qobuz_bundles",
        vec![(
            "bundle_version",
            SqliteValue::String(bundle_version.to_string()),
        )],
        vec![
            (
                "bundle_version",
                SqliteValue::String(bundle_version.to_string()),
            ),
            ("app_id", SqliteValue::String(app_id.to_string())),
        ],
    )
}

pub fn create_qobuz_config(
    db: &Connection,
    access_token: &str,
    user_id: u64,
    user_email: &str,
    user_public_id: &str,
) -> Result<(), DbError> {
    upsert::<QobuzConfig>(
        db,
        "qobuz_config",
        vec![("user_id", SqliteValue::Number(user_id as i64))],
        vec![
            (
                "access_token",
                SqliteValue::String(access_token.to_string()),
            ),
            ("user_id", SqliteValue::Number(user_id as i64)),
            ("user_email", SqliteValue::String(user_email.to_string())),
            (
                "user_public_id",
                SqliteValue::String(user_public_id.to_string()),
            ),
        ],
    )?;

    Ok(())
}

pub fn get_qobuz_app_secrets(db: &Connection) -> Result<Vec<QobuzAppSecret>, DbError> {
    let secrets = select::<QobuzAppSecret>(db, "qobuz_bundle_secrets", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(secrets)
}

pub fn get_qobuz_app_config(db: &Connection) -> Result<Option<QobuzAppConfig>, DbError> {
    let app_configs = select::<QobuzAppConfig>(db, "qobuz_bundles", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(app_configs.last().cloned())
}

pub fn get_qobuz_config(db: &Connection) -> Result<Option<QobuzConfig>, DbError> {
    let configs = select::<QobuzConfig>(db, "qobuz_config", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(configs.last().cloned())
}

pub fn get_qobuz_access_token(db: &Connection) -> Result<Option<String>, DbError> {
    Ok(get_qobuz_config(db)?.map(|c| c.access_token.clone()))
}
