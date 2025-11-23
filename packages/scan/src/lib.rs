//! Music library scanning and indexing for `MoosicBox`.
//!
//! This crate provides functionality for scanning music libraries from both local
//! filesystem sources and remote music API services (e.g., streaming platforms).
//! It discovers music files, extracts metadata, downloads artwork, and updates
//! the `MoosicBox` database with artists, albums, and tracks.
//!
//! # Features
//!
//! * Scan local music files with metadata extraction
//! * Scan remote music services via API integrations
//! * Progress tracking for long-running scan operations
//! * Artwork downloading and caching
//! * Database synchronization with deduplication
//!
//! # Example
//!
//! ```rust,no_run
//! # use moosicbox_scan::{Scanner, ScanOrigin};
//! # use moosicbox_music_api::MusicApis;
//! # use switchy_database::profiles::LibraryDatabase;
//! # async fn example(db: &LibraryDatabase, music_apis: MusicApis) -> Result<(), Box<dyn std::error::Error>> {
//! // Create a scanner for a specific origin
//! let scanner = Scanner::from_origin(db, ScanOrigin::Local).await?;
//!
//! // Start scanning
//! scanner.scan(music_apis, db).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::{path::PathBuf, sync::atomic::AtomicUsize};

use db::get_enabled_scan_origins;
use event::{PROGRESS_LISTENERS, ProgressEvent, ScanTask};
use moosicbox_config::get_cache_dir_path;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::{MusicApi, MusicApis, SourceToMusicApi as _};
use moosicbox_music_models::TrackApiSource;
use switchy_async::util::CancellationToken;
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "local")]
pub mod local;

/// Database operations for scan locations and origins.
pub mod db;
/// Progress event types and listener registration for scan operations.
pub mod event;
/// Music API scanning functionality for remote streaming services.
pub mod music_api;
/// Data structures and database update operations for scan results.
pub mod output;

/// Re-exports the scan models from `moosicbox_scan_models`.
pub use moosicbox_scan_models as models;

static CACHE_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| get_cache_dir_path().expect("Could not get cache directory"));

static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

/// Cancels any ongoing scan operations.
pub fn cancel() {
    CANCELLATION_TOKEN.cancel();
}

/// Type alias for track API sources used as scan origins.
pub type ScanOrigin = TrackApiSource;

#[allow(unused)]
async fn get_origins_or_default(
    db: &LibraryDatabase,
    origins: Option<Vec<ScanOrigin>>,
) -> Result<Vec<ScanOrigin>, DatabaseFetchError> {
    let enabled_origins = get_enabled_scan_origins(db).await?;

    Ok(if let Some(origins) = origins {
        origins
            .iter()
            .filter(|o| enabled_origins.iter().any(|enabled| enabled == *o))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        enabled_origins
    })
}

/// Errors that can occur during scanning operations.
#[derive(Debug, Error)]
pub enum ScanError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Local filesystem scan failed.
    #[cfg(feature = "local")]
    #[error(transparent)]
    Local(#[from] local::ScanError),
    /// Music API operation failed.
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Music API scan operation failed.
    #[error(transparent)]
    ScanMusicApi(#[from] music_api::ScanError),
    /// Scan source is invalid or not supported.
    #[error("Invalid source")]
    InvalidSource,
}

/// Manages scanning operations for music library sources.
#[derive(Clone)]
pub struct Scanner {
    scanned: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    task: Arc<ScanTask>,
}

impl Scanner {
    /// Creates a scanner from a scan origin by looking up its configured locations.
    ///
    /// # Panics
    ///
    /// * If the scan location path is missing
    ///
    /// # Errors
    ///
    /// * If a database error occurs
    #[allow(unused, clippy::unused_async)]
    pub async fn from_origin(
        db: &LibraryDatabase,
        origin: ScanOrigin,
    ) -> Result<Self, DatabaseFetchError> {
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

    /// Creates a new scanner for the given scan task.
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

    /// Notifies listeners that the scan has finished.
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

    /// Executes the scan operation for this scanner's configured task.
    ///
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
                self.scan_music_api(
                    &**music_apis
                        .get(&origin.clone().into())
                        .ok_or(ScanError::InvalidSource)?,
                    db,
                )
                .await?;
            }
        }

        self.on_scan_finished().await;

        Ok(())
    }

    /// Scans all local filesystem paths configured for this scanner.
    ///
    /// # Errors
    ///
    /// * If the scan fails
    /// * If a tokio task fails to join
    #[allow(clippy::uninhabited_references)]
    #[cfg(feature = "local")]
    pub async fn scan_all_local(&self, db: &LibraryDatabase) -> Result<(), ScanError> {
        self.scanned.store(0, std::sync::atomic::Ordering::SeqCst);
        self.total.store(0, std::sync::atomic::Ordering::SeqCst);

        match &*self.task {
            #[cfg(feature = "local")]
            ScanTask::Local { paths } => self.scan_local(db, paths).await?,
            ScanTask::Api { .. } => {}
        }

        self.on_scan_finished().await;

        Ok(())
    }

    /// Scans the specified local filesystem paths for music files.
    ///
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

            switchy_async::runtime::Handle::current()
                .spawn_with_name(&format!("scan_local: scan '{path}'"), async move {
                    local::scan(&path, &db, CANCELLATION_TOKEN.clone(), scanner).await
                })
        });

        for resp in futures::future::join_all(handles).await {
            resp??;
        }

        Ok(())
    }

    /// Checks if a scan origin is enabled in the database.
    ///
    /// # Errors
    ///
    /// * If fails to fetch the enabled scan origins
    pub async fn is_scan_origin_enabled(
        &self,
        db: &LibraryDatabase,
        origin: &ScanOrigin,
    ) -> Result<bool, music_api::ScanError> {
        is_scan_origin_enabled(db, origin).await
    }

    /// Scans a music API source for albums and tracks.
    ///
    /// # Errors
    ///
    /// * If fails to fetch the enabled scan origins
    /// * If the scan fails
    pub async fn scan_music_api(
        &self,
        api: &dyn MusicApi,
        db: &LibraryDatabase,
    ) -> Result<(), music_api::ScanError> {
        if !self
            .is_scan_origin_enabled(db, &api.source().into())
            .await?
        {
            log::debug!(
                "scan_music_api: scan origin is not enabled: {}",
                api.source()
            );
            return Ok(());
        }

        let scanner = self.clone();

        music_api::scan(api, db, CANCELLATION_TOKEN.clone(), Some(scanner)).await?;

        Ok(())
    }
}

/// Checks if a scan origin is enabled in the database.
///
/// # Errors
///
/// * If fails to fetch the enabled scan origins
pub async fn is_scan_origin_enabled(
    db: &LibraryDatabase,
    origin: &ScanOrigin,
) -> Result<bool, music_api::ScanError> {
    Ok(get_enabled_scan_origins(db).await?.contains(origin))
}

/// Retrieves all enabled scan origins from the database.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_scan_origins(db: &LibraryDatabase) -> Result<Vec<ScanOrigin>, DatabaseFetchError> {
    get_enabled_scan_origins(db).await
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
    #[cfg(feature = "local")]
    if origin == &ScanOrigin::Local {
        return Ok(());
    }

    let locations = db::get_scan_locations(db).await?;

    if locations.iter().any(|location| &location.origin == origin) {
        return Ok(());
    }

    db::enable_scan_origin(db, origin).await
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
    let locations = db::get_scan_locations(db).await?;

    if locations.iter().all(|location| &location.origin != origin) {
        return Ok(());
    }

    db::disable_scan_origin(db, origin).await
}

/// Runs a scan for the specified origins (or all enabled origins if none specified).
///
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

/// Retrieves all configured local scan paths from the database.
///
/// # Panics
///
/// * If the download location path is missing
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn get_scan_paths(db: &LibraryDatabase) -> Result<Vec<String>, DatabaseFetchError> {
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

/// Adds a local filesystem path to the scan locations.
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn add_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .any(|location| location.path.as_ref().is_some_and(|p| p.as_str() == path))
    {
        return Ok(());
    }

    db::add_scan_path(db, path).await
}

/// Removes a local filesystem path from the scan locations.
///
/// # Errors
///
/// * If a database error occurs
#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &LibraryDatabase, path: &str) -> Result<(), DatabaseFetchError> {
    let locations = db::get_scan_locations(db).await?;

    if locations
        .iter()
        .all(|location| location.path.as_ref().is_none_or(|p| p.as_str() != path))
    {
        return Ok(());
    }

    db::remove_scan_path(db, path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "local")]
    use moosicbox_schema::get_sqlite_library_migrations;
    #[cfg(feature = "local")]
    use std::sync::Arc;
    #[cfg(feature = "local")]
    use switchy_schema_test_utils::MigrationTestBuilder;

    #[test_log::test]
    fn test_scanner_new_initializes_with_zero_counters() {
        let tidal = moosicbox_music_models::ApiSource::register("Tidal", "Tidal");
        let task = ScanTask::Api {
            origin: ScanOrigin::Api(tidal),
        };
        let scanner = Scanner::new(task.clone());

        assert_eq!(scanner.scanned.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(scanner.total.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(*scanner.task, task);
    }

    #[cfg(feature = "local")]
    #[test_log::test(switchy_async::test)]
    async fn test_enable_scan_origin_returns_ok_for_local() {
        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        // Enabling Local should be a no-op
        let result = enable_scan_origin(&db, &ScanOrigin::Local).await;
        assert!(result.is_ok());

        // Verify no scan location was added for Local
        let locations = db::get_scan_locations(&db).await.unwrap();
        assert_eq!(locations.len(), 0);
    }

    #[cfg(feature = "local")]
    #[test_log::test(switchy_async::test)]
    async fn test_add_scan_path_adds_new_path() {
        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let path = "/music/library";
        let result = add_scan_path(&db, path).await;
        assert!(result.is_ok());

        let paths = get_scan_paths(&db).await.unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], path);
    }

    #[cfg(feature = "local")]
    #[test_log::test(switchy_async::test)]
    async fn test_add_scan_path_skips_duplicate_path() {
        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let path = "/music/library";
        add_scan_path(&db, path).await.unwrap();

        // Try to add the same path again
        let result = add_scan_path(&db, path).await;
        assert!(result.is_ok());

        // Should still only have one path
        let paths = get_scan_paths(&db).await.unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[cfg(feature = "local")]
    #[test_log::test(switchy_async::test)]
    async fn test_remove_scan_path_removes_existing_path() {
        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let path = "/music/library";
        add_scan_path(&db, path).await.unwrap();

        let result = remove_scan_path(&db, path).await;
        assert!(result.is_ok());

        let paths = get_scan_paths(&db).await.unwrap();
        assert_eq!(paths.len(), 0);
    }

    #[cfg(feature = "local")]
    #[test_log::test(switchy_async::test)]
    async fn test_remove_scan_path_is_noop_when_path_not_exists() {
        let db = switchy::database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = LibraryDatabase {
            database: Arc::new(db),
        };

        MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
            .with_table_name("__moosicbox_schema_migrations")
            .run(&*db)
            .await
            .unwrap();

        let result = remove_scan_path(&db, "/nonexistent/path").await;
        assert!(result.is_ok());

        let paths = get_scan_paths(&db).await.unwrap();
        assert_eq!(paths.len(), 0);
    }
}
