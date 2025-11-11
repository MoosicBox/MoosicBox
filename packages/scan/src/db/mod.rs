//! Database operations for scan locations and origins.
//!
//! This module provides functions to manage scan locations in the database,
//! including adding/removing scan paths, enabling/disabling scan origins,
//! and querying configured scan locations.

use moosicbox_json_utils::{ToValueType, database::DatabaseFetchError};
use switchy_database::{profiles::LibraryDatabase, query::FilterableQuery};

use crate::ScanOrigin;

use self::models::ScanLocation;

/// Data structures for scan location database records.
pub mod models;

/// Adds a local filesystem path to the database as a scan location.
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn add_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    db.upsert("scan_locations")
        .where_eq("origin", ScanOrigin::Local.to_string())
        .where_eq("path", path)
        .value("origin", ScanOrigin::Local.to_string())
        .value("path", path)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Removes a local filesystem path from the database scan locations.
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    db.delete("scan_locations")
        .where_eq("origin", ScanOrigin::Local.to_string())
        .where_eq("path", path)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Enables a scan origin in the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn enable_scan_origin(
    db: &LibraryDatabase,
    origin: &ScanOrigin,
) -> Result<(), DatabaseFetchError> {
    db.upsert("scan_locations")
        .where_eq("origin", origin.to_string())
        .value("origin", origin.to_string())
        .execute(&**db)
        .await?;

    Ok(())
}

/// Disables a scan origin in the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn disable_scan_origin(
    db: &LibraryDatabase,
    origin: &ScanOrigin,
) -> Result<(), DatabaseFetchError> {
    db.delete("scan_locations")
        .where_eq("origin", origin.to_string())
        .execute(&**db)
        .await?;

    Ok(())
}

/// Retrieves all enabled scan origins from the database.
///
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
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves all scan locations from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_locations(
    db: &LibraryDatabase,
) -> Result<Vec<ScanLocation>, DatabaseFetchError> {
    Ok(db
        .select("scan_locations")
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves scan locations for a specific origin from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_locations_for_origin(
    db: &LibraryDatabase,
    origin: ScanOrigin,
) -> Result<Vec<ScanLocation>, DatabaseFetchError> {
    Ok(db
        .select("scan_locations")
        .where_eq("origin", origin.to_string())
        .execute(&**db)
        .await?
        .to_value_type()?)
}
