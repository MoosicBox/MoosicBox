use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{query::*, Database};
use moosicbox_json_utils::ToValueType;

use crate::ScanOrigin;

use self::models::ScanLocation;

pub mod models;

#[cfg(feature = "local")]
pub async fn add_scan_path(db: &dyn Database, path: &str) -> Result<(), DbError> {
    db.upsert("scan_locations")
        .where_eq("origin", ScanOrigin::Local.as_ref())
        .where_eq("path", path)
        .value("origin", ScanOrigin::Local.as_ref())
        .value("path", path)
        .execute(db)
        .await?;

    Ok(())
}

#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &dyn Database, path: &str) -> Result<(), DbError> {
    db.delete("scan_locations")
        .where_eq("origin", ScanOrigin::Local.as_ref())
        .where_eq("path", path)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn enable_scan_origin(db: &dyn Database, origin: ScanOrigin) -> Result<(), DbError> {
    db.upsert("scan_locations")
        .where_eq("origin", origin.as_ref())
        .value("origin", origin.as_ref())
        .execute(db)
        .await?;

    Ok(())
}

pub async fn disable_scan_origin(db: &dyn Database, origin: ScanOrigin) -> Result<(), DbError> {
    db.delete("scan_locations")
        .where_eq("origin", origin.as_ref())
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_enabled_scan_origins(db: &dyn Database) -> Result<Vec<ScanOrigin>, DbError> {
    Ok(db
        .select("scan_locations")
        .distinct()
        .columns(&["origin"])
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_scan_locations(db: &dyn Database) -> Result<Vec<ScanLocation>, DbError> {
    Ok(db
        .select("scan_locations")
        .execute(db)
        .await?
        .to_value_type()?)
}
pub async fn get_scan_locations_for_origin(
    db: &dyn Database,
    origin: ScanOrigin,
) -> Result<Vec<ScanLocation>, DbError> {
    Ok(db
        .select("scan_locations")
        .where_eq("origin", origin.as_ref())
        .execute(db)
        .await?
        .to_value_type()?)
}
