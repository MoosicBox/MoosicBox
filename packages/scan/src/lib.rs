#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::path::PathBuf;
use std::sync::Arc;

use db::get_enabled_scan_origins;
use moosicbox_config::get_cache_dir_path;
use moosicbox_core::sqlite::{
    db::DbError,
    models::{ApiSource, TrackApiSource},
};
use moosicbox_database::Database;
use moosicbox_music_api::{MusicApi, MusicApiState, MusicApisError};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "local")]
pub mod local;

pub mod db;
pub mod music_api;
pub mod output;

static CACHE_DIR: Lazy<PathBuf> =
    Lazy::new(|| get_cache_dir_path().expect("Could not get cache directory"));

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

pub fn cancel() {
    CANCELLATION_TOKEN.cancel();
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ScanOrigin {
    #[cfg(feature = "local")]
    Local,
    Tidal,
    Qobuz,
    Yt,
}

impl From<ScanOrigin> for ApiSource {
    fn from(value: ScanOrigin) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanOrigin::Local => {
                moosicbox_assert::die_or_panic!("Local ScanOrigin cant map to ApiSource")
            }
            ScanOrigin::Tidal => ApiSource::Tidal,
            ScanOrigin::Qobuz => ApiSource::Qobuz,
            ScanOrigin::Yt => ApiSource::Yt,
        }
    }
}

impl From<ApiSource> for ScanOrigin {
    fn from(value: ApiSource) -> Self {
        match value {
            ApiSource::Library => {
                moosicbox_assert::die_or_panic!("Library ApiSource cant map to ScanOrigin")
            }
            ApiSource::Tidal => ScanOrigin::Tidal,
            ApiSource::Qobuz => ScanOrigin::Qobuz,
            ApiSource::Yt => ScanOrigin::Yt,
        }
    }
}

impl From<TrackApiSource> for ScanOrigin {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => {
                #[cfg(feature = "local")]
                {
                    ScanOrigin::Local
                }
                #[cfg(not(feature = "local"))]
                {
                    moosicbox_assert::die_or_panic!("Local TrackApiSource cant map to ScanOrigin")
                }
            }
            TrackApiSource::Tidal => ScanOrigin::Tidal,
            TrackApiSource::Qobuz => ScanOrigin::Qobuz,
            TrackApiSource::Yt => ScanOrigin::Yt,
        }
    }
}

impl From<ScanOrigin> for TrackApiSource {
    fn from(value: ScanOrigin) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanOrigin::Local => TrackApiSource::Local,
            ScanOrigin::Tidal => TrackApiSource::Tidal,
            ScanOrigin::Qobuz => TrackApiSource::Qobuz,
            ScanOrigin::Yt => TrackApiSource::Yt,
        }
    }
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[cfg(feature = "local")]
    #[error(transparent)]
    Local(#[from] local::ScanError),
    #[error(transparent)]
    MusicApis(#[from] MusicApisError),
    #[error(transparent)]
    MusicApi(#[from] music_api::ScanError),
}

pub async fn scan(
    api_state: MusicApiState,
    db: Arc<Box<dyn Database>>,
    origins: Option<Vec<ScanOrigin>>,
) -> Result<(), ScanError> {
    let enabled_origins = get_enabled_scan_origins(&**db).await?;

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
            ScanOrigin::Local => scan_local(db.clone()).await?,
            _ => scan_music_api(&**api_state.apis.get(origin.into())?, &**db).await?,
        }
    }

    Ok(())
}

#[cfg(feature = "local")]
pub async fn scan_local(db: Arc<Box<dyn Database>>) -> Result<(), local::ScanError> {
    use db::get_scan_locations_for_origin;

    let locations = get_scan_locations_for_origin(&**db, ScanOrigin::Local).await?;
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
        local::scan(path, db.clone(), CANCELLATION_TOKEN.clone()).await?;
    }

    Ok(())
}

pub async fn scan_music_api(
    api: &dyn MusicApi,
    db: &dyn Database,
) -> Result<(), music_api::ScanError> {
    let enabled_origins = get_enabled_scan_origins(db).await?;
    let enabled = enabled_origins
        .into_iter()
        .any(|origin| origin == ScanOrigin::Tidal);

    if !enabled {
        return Ok(());
    }

    music_api::scan(api, db, CANCELLATION_TOKEN.clone()).await?;

    Ok(())
}

pub async fn get_scan_origins(db: &dyn Database) -> Result<Vec<ScanOrigin>, DbError> {
    get_enabled_scan_origins(db).await
}

pub async fn enable_scan_origin(db: &dyn Database, origin: ScanOrigin) -> Result<(), DbError> {
    #[cfg(feature = "local")]
    if origin == ScanOrigin::Local {
        return Ok(());
    }

    let locations = db::get_scan_locations(db).await?;

    if locations.iter().any(|location| location.origin == origin) {
        return Ok(());
    }

    db::enable_scan_origin(db, origin).await
}

pub async fn disable_scan_origin(db: &dyn Database, origin: ScanOrigin) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations.iter().all(|location| location.origin != origin) {
        return Ok(());
    }

    db::disable_scan_origin(db, origin).await
}

#[cfg(feature = "local")]
pub async fn get_scan_paths(db: &dyn Database) -> Result<Vec<String>, DbError> {
    let locations = db::get_scan_locations_for_origin(db, ScanOrigin::Local).await?;

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
pub async fn add_scan_path(db: &dyn Database, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .any(|location| location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::add_scan_path(db, path).await
}

#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &dyn Database, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .all(|location| !location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::remove_scan_path(db, path).await
}
