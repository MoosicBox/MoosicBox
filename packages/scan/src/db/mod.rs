use moosicbox_database::{profiles::LibraryDatabase, query::FilterableQuery};
use moosicbox_json_utils::{ToValueType, database::DatabaseFetchError};

use crate::ScanOrigin;

use self::models::ScanLocation;

pub mod models;

/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn add_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    db.upsert("scan_locations")
        .where_eq("origin", ScanOrigin::Local.as_ref())
        .where_eq("path", path)
        .value("origin", ScanOrigin::Local.as_ref())
        .value("path", path)
        .execute(db)
        .await?;

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    db.delete("scan_locations")
        .where_eq("origin", ScanOrigin::Local.as_ref())
        .where_eq("path", path)
        .execute(db)
        .await?;

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
pub async fn enable_scan_origin(
    db: &LibraryDatabase,
    origin: ScanOrigin,
) -> Result<(), DatabaseFetchError> {
    db.upsert("scan_locations")
        .where_eq("origin", origin.as_ref())
        .value("origin", origin.as_ref())
        .execute(db)
        .await?;

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
pub async fn disable_scan_origin(
    db: &LibraryDatabase,
    origin: ScanOrigin,
) -> Result<(), DatabaseFetchError> {
    db.delete("scan_locations")
        .where_eq("origin", origin.as_ref())
        .execute(db)
        .await?;

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_enabled_scan_origins(
    db: &LibraryDatabase,
) -> Result<Vec<ScanOrigin>, DatabaseFetchError> {
    Ok(db
        .select("scan_locations")
        .distinct()
        .columns(&["origin"])
        .execute(db)
        .await?
        .to_value_type()?)
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_locations(
    db: &LibraryDatabase,
) -> Result<Vec<ScanLocation>, DatabaseFetchError> {
    Ok(db
        .select("scan_locations")
        .execute(db)
        .await?
        .to_value_type()?)
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_locations_for_origin(
    db: &LibraryDatabase,
    origin: ScanOrigin,
) -> Result<Vec<ScanLocation>, DatabaseFetchError> {
    Ok(db
        .select("scan_locations")
        .where_eq("origin", origin.as_ref())
        .execute(db)
        .await?
        .to_value_type()?)
}
