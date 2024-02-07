use moosicbox_core::sqlite::db::{delete, select, select_distinct, upsert, DbError, SqliteValue};
use moosicbox_database::DbConnection;

use crate::ScanOrigin;

use self::models::ScanLocation;

pub mod models;

#[cfg(feature = "local")]
pub fn add_scan_path(db: &DbConnection, path: &str) -> Result<(), DbError> {
    upsert::<ScanLocation>(
        &db.inner,
        "scan_locations",
        vec![
            (
                "origin",
                SqliteValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            ("path", SqliteValue::String(path.to_string())),
        ],
        vec![
            (
                "origin",
                SqliteValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            ("path", SqliteValue::String(path.to_string())),
        ],
    )?;

    Ok(())
}

#[cfg(feature = "local")]
pub fn remove_scan_path(db: &DbConnection, path: &str) -> Result<(), DbError> {
    delete::<ScanLocation>(
        &db.inner,
        "scan_locations",
        &vec![
            (
                "origin",
                SqliteValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            ("path", SqliteValue::String(path.to_string())),
        ],
    )?;

    Ok(())
}

pub fn enable_scan_origin(db: &DbConnection, origin: ScanOrigin) -> Result<(), DbError> {
    upsert::<ScanLocation>(
        &db.inner,
        "scan_locations",
        vec![
            ("origin", SqliteValue::String(origin.as_ref().to_string())),
            ("path", SqliteValue::StringOpt(None)),
        ],
        vec![
            ("origin", SqliteValue::String(origin.as_ref().to_string())),
            ("path", SqliteValue::StringOpt(None)),
        ],
    )?;

    Ok(())
}

pub fn disable_scan_origin(db: &DbConnection, origin: ScanOrigin) -> Result<(), DbError> {
    delete::<ScanLocation>(
        &db.inner,
        "scan_locations",
        &vec![
            ("origin", SqliteValue::String(origin.as_ref().to_string())),
            ("path", SqliteValue::StringOpt(None)),
        ],
    )?;

    Ok(())
}

pub fn get_enabled_scan_origins(db: &DbConnection) -> Result<Vec<ScanOrigin>, DbError> {
    Ok(
        select_distinct::<ScanOrigin>(&db.inner, "scan_locations", &vec![], &["origin"])?
            .into_iter()
            .collect::<Vec<_>>(),
    )
}

pub fn get_scan_locations(db: &DbConnection) -> Result<Vec<ScanLocation>, DbError> {
    Ok(
        select::<ScanLocation>(&db.inner, "scan_locations", &vec![], &["*"])?
            .into_iter()
            .collect::<Vec<_>>(),
    )
}
pub fn get_scan_locations_for_origin(
    db: &DbConnection,
    origin: ScanOrigin,
) -> Result<Vec<ScanLocation>, DbError> {
    Ok(select::<ScanLocation>(
        &db.inner,
        "scan_locations",
        &vec![("origin", SqliteValue::String(origin.as_ref().to_string()))],
        &["*"],
    )?
    .into_iter()
    .collect::<Vec<_>>())
}
