#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::{path::PathBuf, sync::atomic::AtomicUsize};

use db::get_enabled_scan_origins;
use event::{ProgressEvent, ScanTask, PROGRESS_LISTENERS};
use moosicbox_config::get_cache_dir_path;
use moosicbox_core::sqlite::{
    db::DbError,
    models::{ApiSource, TrackApiSource},
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_music_api::{MusicApi, MusicApis, MusicApisError, SourceToMusicApi as _};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "local")]
pub mod local;

pub mod db;
pub mod event;
pub mod music_api;
pub mod output;

static CACHE_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| get_cache_dir_path().expect("Could not get cache directory"));

static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

pub fn cancel() {
    CANCELLATION_TOKEN.cancel();
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ScanOrigin {
    #[cfg(feature = "local")]
    Local,
    #[cfg(feature = "tidal")]
    Tidal,
    #[cfg(feature = "qobuz")]
    Qobuz,
    #[cfg(feature = "yt")]
    Yt,
}

impl From<ScanOrigin> for ApiSource {
    fn from(value: ScanOrigin) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanOrigin::Local => {
                moosicbox_assert::die_or_panic!("Local ScanOrigin cant map to ApiSource")
            }
            #[cfg(feature = "tidal")]
            ScanOrigin::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            ScanOrigin::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            ScanOrigin::Yt => Self::Yt,
        }
    }
}

impl From<ApiSource> for ScanOrigin {
    fn from(value: ApiSource) -> Self {
        match value {
            ApiSource::Library => {
                moosicbox_assert::die_or_panic!("Library ApiSource cant map to ScanOrigin")
            }
            #[cfg(feature = "tidal")]
            ApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            ApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            ApiSource::Yt => Self::Yt,
        }
    }
}

impl From<TrackApiSource> for ScanOrigin {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => {
                #[cfg(feature = "local")]
                {
                    Self::Local
                }
                #[cfg(not(feature = "local"))]
                {
                    moosicbox_assert::die_or_panic!("Local TrackApiSource cant map to ScanOrigin")
                }
            }
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            TrackApiSource::Yt => Self::Yt,
        }
    }
}

impl From<ScanOrigin> for TrackApiSource {
    fn from(value: ScanOrigin) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanOrigin::Local => Self::Local,
            #[cfg(feature = "tidal")]
            ScanOrigin::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            ScanOrigin::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            ScanOrigin::Yt => Self::Yt,
        }
    }
}

#[allow(unused)]
async fn get_origins_or_default(
    db: &LibraryDatabase,
    origins: Option<Vec<ScanOrigin>>,
) -> Result<Vec<ScanOrigin>, DbError> {
    let enabled_origins = get_enabled_scan_origins(db).await?;

    Ok(if let Some(origins) = origins {
        origins
            .iter()
            .filter(|o| enabled_origins.iter().any(|enabled| enabled == *o))
            .copied()
            .collect::<Vec<_>>()
    } else {
        enabled_origins
    })
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

#[derive(Clone)]
pub struct Scanner {
    scanned: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    task: Arc<ScanTask>,
}

impl Scanner {
    /// # Panics
    ///
    /// * If the scan location path is missing
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    #[allow(unused, clippy::unused_async)]
    pub async fn from_origin(db: &LibraryDatabase, origin: ScanOrigin) -> Result<Self, DbError> {
        let task = match origin {
            #[cfg(feature = "local")]
            ScanOrigin::Local => {
                use crate::db::get_scan_locations_for_origin;

                let locations = get_scan_locations_for_origin(db, origin).await?;
                let paths = locations
                    .iter()
                    .map(|location| {
                        location
                            .path
                            .as_ref()
                            .expect("Local ScanLocation is missing path")
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                ScanTask::Local { paths }
            }
            _ => ScanTask::Api { origin },
        };

        Ok(Self::new(task))
    }

    #[must_use]
    pub fn new(task: ScanTask) -> Self {
        Self {
            scanned: Arc::new(AtomicUsize::new(0)),
            total: Arc::new(AtomicUsize::new(0)),
            task: Arc::new(task),
        }
    }

    #[allow(unused)]
    async fn increase_total(&self, count: usize) {
        let total = self.total.load(std::sync::atomic::Ordering::SeqCst) + count;
        self.on_total_updated(total).await;
    }

    #[allow(unused)]
    async fn on_total_updated(&self, total: usize) {
        let scanned = self.scanned.load(std::sync::atomic::Ordering::SeqCst);
        self.total.store(total, std::sync::atomic::Ordering::SeqCst);

        let event = ProgressEvent::ScanCountUpdated {
            scanned,
            total,
            task: self.task.deref().clone(),
        };

        let mut listeners = PROGRESS_LISTENERS.read().await;
        #[allow(unreachable_code)]
        for listener in listeners.iter() {
            listener(&event).await;
        }
    }

    #[allow(unused)]
    async fn on_scanned_track(&self) {
        let total = self.total.load(std::sync::atomic::Ordering::SeqCst);
        let scanned = self
            .scanned
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let event = ProgressEvent::ItemScanned {
            scanned,
            total,
            task: self.task.deref().clone(),
        };

        let mut listeners = PROGRESS_LISTENERS.read().await;
        #[allow(unreachable_code)]
        for listener in listeners.iter() {
            listener(&event).await;
        }
    }

    #[allow(unused)]
    pub async fn on_scan_finished(&self) {
        let scanned = self.scanned.load(std::sync::atomic::Ordering::SeqCst);
        let total = self.total.load(std::sync::atomic::Ordering::SeqCst);

        let event = ProgressEvent::ScanFinished {
            scanned,
            total,
            task: self.task.deref().clone(),
        };

        let mut listeners = PROGRESS_LISTENERS.read().await;
        #[allow(unreachable_code)]
        for listener in listeners.iter() {
            listener(&event).await;
        }
    }

    /// # Errors
    ///
    /// * If the scan fails
    /// * If a tokio task fails to join
    #[allow(clippy::uninhabited_references)]
    pub async fn scan(&self, music_apis: MusicApis, db: &LibraryDatabase) -> Result<(), ScanError> {
        self.scanned.store(0, std::sync::atomic::Ordering::SeqCst);
        self.total.store(0, std::sync::atomic::Ordering::SeqCst);

        match &*self.task {
            #[cfg(feature = "local")]
            ScanTask::Local { paths } => self.scan_local(db, paths).await?,
            ScanTask::Api { origin } => {
                self.scan_music_api(&**music_apis.get((*origin).into())?, db)
                    .await?;
            }
        }

        self.on_scan_finished().await;

        Ok(())
    }

    /// # Errors
    ///
    /// * If the scan fails
    /// * If a tokio task fails to join
    #[cfg(feature = "local")]
    pub async fn scan_local(
        &self,
        db: &LibraryDatabase,
        paths: &[String],
    ) -> Result<(), local::ScanError> {
        let handles = paths.iter().map(|path| {
            let db = db.clone();
            let scanner = self.clone();
            let path = path.to_owned();

            moosicbox_task::spawn(&format!("scan_local: scan '{path}'"), async move {
                local::scan(&path, &db, CANCELLATION_TOKEN.clone(), scanner).await
            })
        });

        for resp in futures::future::join_all(handles).await {
            resp??;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If the scan fails
    /// * If tokio task fails to join
    pub async fn scan_music_api(
        &self,
        api: &dyn MusicApi,
        db: &LibraryDatabase,
    ) -> Result<(), music_api::ScanError> {
        let enabled_origins = get_enabled_scan_origins(db).await?;
        let enabled = enabled_origins
            .into_iter()
            .any(|origin| origin == api.source().into());
        let scanner = self.clone();

        if !enabled {
            log::debug!(
                "scan_music_api: scan origin is not enabled: {}",
                api.source()
            );
            return Ok(());
        }

        music_api::scan(api, db, CANCELLATION_TOKEN.clone(), Some(scanner)).await?;

        Ok(())
    }
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_origins(db: &LibraryDatabase) -> Result<Vec<ScanOrigin>, DbError> {
    get_enabled_scan_origins(db).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn enable_scan_origin(db: &LibraryDatabase, origin: ScanOrigin) -> Result<(), DbError> {
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

/// # Errors
///
/// * If a database error occurs
pub async fn disable_scan_origin(db: &LibraryDatabase, origin: ScanOrigin) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations.iter().all(|location| location.origin != origin) {
        return Ok(());
    }

    db::disable_scan_origin(db, origin).await
}

/// # Errors
///
/// * If a database error occurs
/// * If the scan fails
pub async fn run_scan(
    origins: Option<Vec<ScanOrigin>>,
    db: &LibraryDatabase,
    music_apis: MusicApis,
) -> Result<(), ScanError> {
    log::debug!("run_scan: origins={origins:?}");

    let origins = get_origins_or_default(db, origins).await?;
    log::debug!("run_scan: get_origins_or_default={origins:?}");

    for origin in origins {
        Scanner::from_origin(db, origin)
            .await?
            .scan(music_apis.clone(), db)
            .await?;
    }

    Ok(())
}

/// # Panics
///
/// * If the download location path is missing
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn get_scan_paths(db: &LibraryDatabase) -> Result<Vec<String>, DbError> {
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

/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn add_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .any(|location| location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::add_scan_path(db, path).await
}

/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DbError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .all(|location| !location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::remove_scan_path(db, path).await
}
