#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use db::get_enabled_scan_origins;
use moosicbox_core::{
    app::{Db, DbConnection},
    sqlite::db::DbError,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "tidal")]
pub mod tidal;

pub mod db;
mod output;

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

pub fn cancel() {
    log::debug!("Cancelling scan");
    CANCELLATION_TOKEN.cancel();
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanOrigin {
    #[cfg(feature = "local")]
    Local,
    #[cfg(feature = "tidal")]
    Tidal,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[cfg(feature = "local")]
    #[error(transparent)]
    Local(#[from] local::ScanError),
    #[cfg(feature = "tidal")]
    #[error(transparent)]
    Tidal(#[from] tidal::ScanError),
}

pub async fn scan(db: &Db, origins: Option<Vec<ScanOrigin>>) -> Result<(), ScanError> {
    let enabled_origins = get_enabled_scan_origins(db.library.lock().as_ref().unwrap())?;

    let search_origins = origins
        .map(|origins| {
            origins
                .iter()
                .filter(|o| enabled_origins.iter().any(|enabled| enabled == *o))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or(enabled_origins);

    for origin in search_origins {
        match origin {
            #[cfg(feature = "local")]
            ScanOrigin::Local => scan_local(db).await?,
            #[cfg(feature = "tidal")]
            ScanOrigin::Tidal => scan_tidal(db).await?,
        }
    }

    Ok(())
}

#[cfg(feature = "local")]
pub async fn scan_local(db: &Db) -> Result<(), local::ScanError> {
    use db::get_scan_locations_for_origin;

    let locations =
        get_scan_locations_for_origin(db.library.lock().as_ref().unwrap(), ScanOrigin::Local)?;
    let paths = locations
        .iter()
        .map(|location| {
            location
                .path
                .as_ref()
                .expect("Local ScanLocation is missing path")
        })
        .collect::<Vec<_>>();

    for path in paths {
        local::scan(path, db, CANCELLATION_TOKEN.clone()).await?;
    }

    Ok(())
}

#[cfg(feature = "tidal")]
pub async fn scan_tidal(db: &Db) -> Result<(), tidal::ScanError> {
    let enabled_origins = get_enabled_scan_origins(&db.library.lock().unwrap())?;
    let enabled = enabled_origins
        .into_iter()
        .any(|origin| origin == ScanOrigin::Tidal);

    if !enabled {
        return Ok(());
    }

    tidal::scan(db, CANCELLATION_TOKEN.clone()).await?;

    Ok(())
}

pub fn get_scan_origins(db: &DbConnection) -> Result<Vec<ScanOrigin>, DbError> {
    get_enabled_scan_origins(db)
}

pub fn enable_scan_origin(db: &DbConnection, origin: ScanOrigin) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db)?;

    if locations.iter().any(|location| location.origin == origin) {
        return Ok(());
    }

    db::enable_scan_origin(db, origin)
}

pub fn disable_scan_origin(db: &DbConnection, origin: ScanOrigin) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db)?;

    if locations.iter().all(|location| location.origin != origin) {
        return Ok(());
    }

    db::disable_scan_origin(db, origin)
}

#[cfg(feature = "local")]
pub fn get_scan_paths(db: &DbConnection) -> Result<Vec<String>, DbError> {
    let locations = db::get_scan_locations(db)?;

    Ok(locations
        .iter()
        .map(|location| {
            location
                .path
                .as_ref()
                .expect("Local ScanLocation is missing path")
        })
        .cloned()
        .collect::<Vec<_>>())
}

#[cfg(feature = "local")]
pub fn add_scan_path(db: &DbConnection, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db)?;

    if locations
        .iter()
        .any(|location| location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::add_scan_path(db, path)
}

#[cfg(feature = "local")]
pub fn remove_scan_path(db: &DbConnection, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db)?;

    if locations
        .iter()
        .all(|location| !location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::remove_scan_path(db, path)
}
