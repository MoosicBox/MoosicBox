//! Download queue management and progress tracking.
//!
//! Provides a queue system for managing sequential download tasks with progress
//! tracking and state management. The queue processes tasks one at a time and
//! notifies listeners of download progress events.

#![allow(clippy::module_name_repetitions)]

use std::{
    path::PathBuf,
    pin::Pin,
    str::FromStr as _,
    sync::{Arc, LazyLock},
    time::Duration,
};

use futures::Future;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_scan::local::ScanItem;
use switchy_async::sync::{Mutex, RwLock};
use switchy_async::task::{JoinError, JoinHandle};
use switchy_async::util::CancellationToken;
use switchy_database::{
    DatabaseError, DatabaseValue, Row, profiles::LibraryDatabase, query::FilterableQuery,
};
use thiserror::Error;

use crate::{
    DownloadAlbumError, DownloadTrackError, Downloader,
    db::models::{DownloadItem, DownloadTask, DownloadTaskState},
};

static TIMEOUT_DURATION: LazyLock<Duration> = LazyLock::new(|| Duration::from_secs(30));

/// Error updating a download task in the database.
#[derive(Debug, Error)]
pub enum UpdateTaskError {
    /// Database operation failed
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// No database connection available
    #[error("No database")]
    NoDatabase,
    /// Task row not found in database
    #[error("No row")]
    NoRow,
}

/// Error processing the download queue.
#[derive(Debug, Error)]
pub enum ProcessDownloadQueueError {
    /// Database fetch operation failed
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Database operation failed
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to update task
    #[error(transparent)]
    UpdateTask(#[from] UpdateTaskError),
    /// Task join failed
    #[error(transparent)]
    Join(#[from] JoinError),
    /// Failed to download track
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    /// Failed to download album
    #[error(transparent)]
    DownloadAlbum(#[from] DownloadAlbumError),
    /// Local scan operation failed
    #[error(transparent)]
    LocalScan(#[from] moosicbox_scan::local::ScanError),
    /// I/O operation failed
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// No database connection available
    #[error("No database")]
    NoDatabase,
    /// No downloader configured
    #[error("No downloader")]
    NoDownloader,
}

#[derive(Debug, Clone, PartialEq)]
struct ProcessDownloadTaskResponse {
    task_id: u64,
}

#[derive(Debug)]
struct DownloadQueueState {
    tasks: Vec<DownloadTask>,
    results: Vec<Result<ProcessDownloadTaskResponse, ProcessDownloadQueueError>>,
}

impl DownloadQueueState {
    const fn new() -> Self {
        Self {
            tasks: vec![],
            results: vec![],
        }
    }

    fn add_task_to_queue(&mut self, task: DownloadTask) {
        self.tasks.push(task);
    }

    fn add_tasks_to_queue(&mut self, tasks: Vec<DownloadTask>) {
        self.tasks.extend(tasks);
    }

    fn finish_task(&mut self, task: &DownloadTask) {
        self.tasks
            .retain(|x| !(task.file_path == x.file_path && task.item == x.item));
    }

    pub fn current_task(&self) -> Option<&DownloadTask> {
        self.tasks.first()
    }
}

/// Generic progress event for download operations.
#[derive(Clone)]
pub enum GenericProgressEvent {
    /// Total size of the download in bytes
    Size {
        /// Total bytes to download, if known
        bytes: Option<u64>,
    },
    /// Current download speed
    Speed {
        /// Download speed in bytes per second
        bytes_per_second: f64,
    },
    /// Bytes read progress
    BytesRead {
        /// Bytes read so far
        read: usize,
        /// Total bytes to read
        total: usize,
    },
}

/// Progress event for a specific download task.
#[derive(Clone)]
pub enum ProgressEvent {
    /// Total size of the download
    Size {
        /// The download task
        task: DownloadTask,
        /// Total bytes to download, if known
        bytes: Option<u64>,
    },
    /// Current download speed
    Speed {
        /// The download task
        task: DownloadTask,
        /// Download speed in bytes per second
        bytes_per_second: f64,
    },
    /// Bytes read progress
    BytesRead {
        /// The download task
        task: DownloadTask,
        /// Bytes read so far
        read: usize,
        /// Total bytes to read
        total: usize,
    },
    /// Task state changed
    State {
        /// The download task
        task: DownloadTask,
        /// New task state
        state: DownloadTaskState,
    },
}

/// Future returned by progress listener callbacks.
pub type ProgressListenerFut = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Progress listener callback for generic progress events.
pub type ProgressListener =
    Box<dyn (FnMut(GenericProgressEvent) -> ProgressListenerFut) + Send + Sync>;

/// Future returned by progress listener reference callbacks.
pub type ProgressListenerRefFut = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Progress listener callback for task-specific progress events.
pub type ProgressListenerRef =
    Box<dyn (Fn(&ProgressEvent) -> ProgressListenerRefFut) + Send + Sync>;

/// Queue for managing and processing download tasks.
///
/// Downloads are processed sequentially in the order they are added.
#[derive(Clone)]
pub struct DownloadQueue {
    progress_listeners: Vec<Arc<ProgressListenerRef>>,
    database: Option<LibraryDatabase>,
    downloader: Option<Arc<Box<dyn Downloader + Send + Sync>>>,
    state: Arc<RwLock<DownloadQueueState>>,
    #[allow(clippy::type_complexity)]
    join_handle: Arc<Mutex<Option<JoinHandle<Result<(), ProcessDownloadQueueError>>>>>,
    scan: bool,
}

impl DownloadQueue {
    /// Creates a new empty download queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            progress_listeners: vec![],
            database: None,
            downloader: None,
            state: Arc::new(RwLock::new(DownloadQueueState::new())),
            join_handle: Arc::new(Mutex::new(None)),
            scan: true,
        }
    }

    /// Returns whether the queue has a database connection configured.
    #[must_use]
    pub const fn has_database(&self) -> bool {
        self.database.is_some()
    }

    /// Configures the queue with a database connection.
    #[must_use]
    pub fn with_database(mut self, database: LibraryDatabase) -> Self {
        self.database.replace(database);
        self
    }

    /// Returns whether the queue has a downloader configured.
    #[must_use]
    pub fn has_downloader(&self) -> bool {
        self.downloader.is_some()
    }

    /// Configures the queue with a downloader.
    #[must_use]
    pub fn with_downloader(mut self, downloader: Box<dyn Downloader + Send + Sync>) -> Self {
        self.downloader.replace(Arc::new(downloader));
        self
    }

    /// Adds a progress listener to receive download progress events.
    #[must_use]
    pub fn add_progress_listener(mut self, listener: ProgressListenerRef) -> Self {
        self.progress_listeners.push(Arc::new(listener));
        self
    }

    /// Configures whether to scan downloaded files.
    #[must_use]
    pub const fn with_scan(mut self, scan: bool) -> Self {
        self.scan = scan;
        self
    }

    /// Returns the currently processing download task, if any.
    #[must_use]
    pub async fn current_task(&self) -> Option<DownloadTask> {
        self.state.read().await.current_task().cloned()
    }

    /// Returns the current download speed in bytes per second, if available.
    #[must_use]
    pub fn speed(&self) -> Option<f64> {
        self.downloader
            .clone()
            .and_then(|downloader| downloader.speed())
    }

    /// Adds a task to the download queue.
    pub async fn add_task_to_queue(&mut self, task: DownloadTask) {
        self.state.write().await.add_task_to_queue(task);
    }

    /// Adds multiple tasks to the download queue.
    pub async fn add_tasks_to_queue(&mut self, tasks: Vec<DownloadTask>) {
        self.state.write().await.add_tasks_to_queue(tasks);
    }

    /// Starts processing the download queue.
    ///
    /// Returns a handle to the background task processing the queue.
    pub fn process(&mut self) -> JoinHandle<Result<(), ProcessDownloadQueueError>> {
        let join_handle = self.join_handle.clone();
        let this = self.clone();

        switchy_async::runtime::Handle::current().spawn_with_name(
            "downloader: queue - process",
            async move {
                let mut handle = join_handle.lock().await;

                if let Some(handle) = handle.as_mut()
                    && !handle.is_finished()
                {
                    handle.await??;
                }

                handle.replace(switchy_async::runtime::Handle::current().spawn_with_name(
                    "downloader: queue - process_inner",
                    async move {
                        this.process_inner().await?;
                        Ok(())
                    },
                ));

                drop(handle);

                Ok::<_, ProcessDownloadQueueError>(())
            },
        )
    }

    #[allow(unused)]
    async fn shutdown(&self) -> Result<(), ProcessDownloadQueueError> {
        let mut handle = self.join_handle.lock().await;

        if let Some(handle) = handle.as_mut() {
            if handle.is_finished() {
                Ok(())
            } else {
                Ok(handle.await??)
            }
        } else {
            Ok(())
        }
    }

    async fn process_inner(&self) -> Result<(), ProcessDownloadQueueError> {
        while let Some(mut task) = {
            let state = self.state.as_ref().read().await;
            state.tasks.first().cloned()
        } {
            let result = self.process_task(&mut task).await;

            let mut state = self.state.write().await;

            if let Err(err) = &result {
                log::error!("Encountered error when processing task in DownloadQueue: {err:?}");
                self.update_task_state(&mut task, DownloadTaskState::Error)
                    .await?;
            }

            state.results.push(result);
            state.finish_task(&task);
        }

        Ok(())
    }

    async fn update_task_state(
        &self,
        task: &mut DownloadTask,
        state: DownloadTaskState,
    ) -> Result<Row, UpdateTaskError> {
        task.state = state;

        let row = self
            .update_task(
                task.id,
                &[(
                    "state",
                    DatabaseValue::String(task.state.as_ref().to_string()),
                )],
            )
            .await;

        for listener in &self.progress_listeners {
            #[allow(unreachable_code)]
            listener(&ProgressEvent::State {
                task: task.clone(),
                state,
            })
            .await;
        }

        row
    }

    async fn update_task(
        &self,
        task_id: u64,
        values: &[(&str, DatabaseValue)],
    ) -> Result<Row, UpdateTaskError> {
        let db = self.database.clone().ok_or(UpdateTaskError::NoDatabase)?;

        db.update("download_tasks")
            .where_eq("id", task_id)
            .values(values.to_vec())
            .execute_first(&*db)
            .await?
            .ok_or(UpdateTaskError::NoRow)
    }

    #[allow(
        unreachable_code,
        unused,
        clippy::too_many_lines,
        clippy::uninhabited_references,
        clippy::cognitive_complexity
    )]
    async fn process_task(
        &self,
        task: &mut DownloadTask,
    ) -> Result<ProcessDownloadTaskResponse, ProcessDownloadQueueError> {
        log::debug!("Processing task {task:?}");

        self.update_task_state(task, DownloadTaskState::Started)
            .await?;

        let mut task_size = None;
        let database = self
            .database
            .clone()
            .ok_or(ProcessDownloadQueueError::NoDatabase)?;

        let task_id = task.id;
        let listeners = self.progress_listeners.clone();
        let send_task = task.clone();

        let on_progress = Box::new({
            let database = database.clone();
            move |event: GenericProgressEvent| {
                let database = database.clone();
                let send_task = send_task.clone();
                let listeners = listeners.clone();
                Box::pin(async move {
                    match event.clone() {
                        GenericProgressEvent::Size { bytes, .. } => {
                            log::debug!("Got size of task: {bytes:?}");
                            if let Some(size) = bytes {
                                task_size.replace(size);
                                let database = database.clone();
                                switchy_async::runtime::Handle::current().spawn_with_name(
                                    "downloader: queue - on_progress - size",
                                    async move {
                                        if let Err(err) = database
                                            .update("download_tasks")
                                            .where_eq("id", task_id)
                                            .value("total_bytes", size)
                                            .execute_first(&*database)
                                            .await
                                        {
                                            log::error!(
                                                "Failed to set DownloadTask total_bytes: {err:?}"
                                            );
                                        }
                                    },
                                );
                            }
                        }
                        GenericProgressEvent::Speed { .. }
                        | GenericProgressEvent::BytesRead { .. } => {}
                    }

                    let event = match event {
                        GenericProgressEvent::Size { bytes } => ProgressEvent::Size {
                            task: send_task.clone(),
                            bytes,
                        },
                        GenericProgressEvent::Speed { bytes_per_second } => ProgressEvent::Speed {
                            task: send_task.clone(),
                            bytes_per_second,
                        },
                        GenericProgressEvent::BytesRead { read, total } => {
                            ProgressEvent::BytesRead {
                                task: send_task.clone(),
                                read,
                                total,
                            }
                        }
                    };
                    for listener in &listeners {
                        listener(&event).await;
                    }
                }) as ProgressListenerFut
            }
        });

        let downloader = self
            .downloader
            .clone()
            .ok_or(ProcessDownloadQueueError::NoDownloader)?;

        let path = PathBuf::from_str(&task.file_path).unwrap();

        let scanner = if self.scan {
            let scan_paths = moosicbox_scan::get_scan_paths(&database.clone()).await?;

            if scan_paths.iter().any(|x| path.starts_with(x)) {
                Some(moosicbox_scan::Scanner::new(
                    moosicbox_scan::event::ScanTask::Local {
                        paths: vec![path.parent().unwrap().to_str().unwrap().to_string()],
                    },
                ))
            } else {
                None
            }
        } else {
            None
        };

        match &task.item {
            DownloadItem::Track {
                track_id,
                quality,
                source,
                ..
            } => {
                let track = downloader
                    .download_track_id(
                        &task.file_path,
                        track_id,
                        *quality,
                        source.clone(),
                        on_progress,
                        Some(*TIMEOUT_DURATION),
                    )
                    .await?;

                if let Some(scanner) = scanner {
                    let metadata = tokio::fs::File::open(&path).await?.metadata().await?;

                    moosicbox_scan::local::scan_items(
                        vec![ScanItem::Track {
                            path,
                            metadata,
                            track: Some(track),
                        }],
                        &database,
                        CancellationToken::new(),
                        scanner.clone(),
                    )
                    .await?;

                    scanner.on_scan_finished().await;
                }
            }
            DownloadItem::AlbumCover {
                album_id, source, ..
            } => {
                let album = downloader
                    .download_album_cover(&task.file_path, album_id, source.clone(), on_progress)
                    .await?;

                if let Some(scanner) = scanner {
                    let metadata = tokio::fs::File::open(&path).await?.metadata().await?;

                    moosicbox_scan::local::scan_items(
                        vec![ScanItem::AlbumCover {
                            path,
                            metadata,
                            album: Some(album),
                        }],
                        &database,
                        CancellationToken::new(),
                        scanner.clone(),
                    )
                    .await?;

                    scanner.on_scan_finished().await;
                }
            }
            DownloadItem::ArtistCover {
                album_id, source, ..
            } => {
                let artist = downloader
                    .download_artist_cover(&task.file_path, album_id, source.clone(), on_progress)
                    .await?;

                if let Some(scanner) = scanner {
                    let metadata = tokio::fs::File::open(&path).await?.metadata().await?;

                    moosicbox_scan::local::scan_items(
                        vec![ScanItem::ArtistCover {
                            path,
                            metadata,
                            artist: Some(artist),
                        }],
                        &database,
                        CancellationToken::new(),
                        scanner.clone(),
                    )
                    .await?;

                    scanner.on_scan_finished().await;
                }
            }
        }

        if let Some(size) = task_size {
            task.total_bytes.replace(size);
        }

        self.update_task_state(task, DownloadTaskState::Finished)
            .await?;

        Ok(ProcessDownloadTaskResponse { task_id: task.id })
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DownloadQueue {
    fn drop(&mut self) {
        let handle = self.join_handle.clone();

        switchy_async::runtime::Handle::current().spawn_with_name(
            "downloader: queue - drop",
            async move {
                let mut handle = handle.lock().await;
                if let Some(handle) = handle.as_mut()
                    && !handle.is_finished()
                    && let Err(err) = handle.await
                {
                    log::error!("Failed to drop DownloadQueue: {err:?}");
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use moosicbox_music_api::models::TrackAudioQuality;
    use moosicbox_music_models::{Album, ApiSource, Artist, Track, id::Id};
    use pretty_assertions::assert_eq;
    use switchy_database::{
        Database, Row,
        query::*,
        schema::{
            AlterTableStatement, ColumnInfo, CreateIndexStatement, CreateTableStatement,
            DropIndexStatement, DropTableStatement, TableInfo,
        },
    };

    use crate::{
        DownloadApiSource,
        db::models::{DownloadItem, DownloadTaskState},
    };

    use super::*;

    struct TestDownloader {}

    static TIDAL_API_SOURCE: LazyLock<ApiSource> =
        LazyLock::new(|| ApiSource::register("Tidal", "Tidal"));

    #[async_trait]
    impl Downloader for TestDownloader {
        async fn download_track_id(
            &self,
            _path: &str,
            track_id: &Id,
            _quality: TrackAudioQuality,
            _source: DownloadApiSource,
            _on_size: ProgressListener,
            _timeout_duration: Option<Duration>,
        ) -> Result<Track, DownloadTrackError> {
            Ok(Track {
                id: track_id.to_owned(),
                ..Default::default()
            })
        }

        async fn download_album_cover(
            &self,
            _path: &str,
            album_id: &Id,
            _source: DownloadApiSource,
            _on_size: ProgressListener,
        ) -> Result<Album, DownloadAlbumError> {
            Ok(Album {
                id: album_id.to_owned(),
                ..Default::default()
            })
        }

        async fn download_artist_cover(
            &self,
            _path: &str,
            _album_id: &Id,
            _source: DownloadApiSource,
            _on_size: ProgressListener,
        ) -> Result<Artist, DownloadAlbumError> {
            Ok(Artist::default())
        }
    }

    #[derive(Debug)]
    struct TestDatabase {}

    #[async_trait]
    impl Database for TestDatabase {
        async fn query(&self, _query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![])
        }

        async fn query_first(
            &self,
            _query: &SelectQuery<'_>,
        ) -> Result<Option<Row>, DatabaseError> {
            Ok(None)
        }

        async fn exec_delete(
            &self,
            _statement: &DeleteStatement<'_>,
        ) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![])
        }

        async fn exec_delete_first(
            &self,
            _statement: &DeleteStatement<'_>,
        ) -> Result<Option<Row>, DatabaseError> {
            Ok(None)
        }

        async fn exec_insert(
            &self,
            _statement: &InsertStatement<'_>,
        ) -> Result<Row, DatabaseError> {
            Ok(Row { columns: vec![] })
        }

        async fn exec_update(
            &self,
            _statement: &UpdateStatement<'_>,
        ) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![Row { columns: vec![] }])
        }

        async fn exec_update_first(
            &self,
            _statement: &UpdateStatement<'_>,
        ) -> Result<Option<Row>, DatabaseError> {
            Ok(Some(Row { columns: vec![] }))
        }

        async fn exec_upsert(
            &self,
            _statement: &UpsertStatement<'_>,
        ) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![Row { columns: vec![] }])
        }

        async fn exec_upsert_first(
            &self,
            _statement: &UpsertStatement<'_>,
        ) -> Result<Row, DatabaseError> {
            Ok(Row { columns: vec![] })
        }

        async fn exec_upsert_multi(
            &self,
            _statement: &UpsertMultiStatement<'_>,
        ) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![Row { columns: vec![] }])
        }

        async fn exec_create_table(
            &self,
            _statement: &CreateTableStatement<'_>,
        ) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn exec_drop_table(
            &self,
            _statement: &DropTableStatement<'_>,
        ) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn exec_create_index(
            &self,
            _statement: &CreateIndexStatement<'_>,
        ) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn exec_drop_index(
            &self,
            _statement: &DropIndexStatement<'_>,
        ) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn exec_alter_table(
            &self,
            _statement: &AlterTableStatement<'_>,
        ) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn exec_raw(&self, _statement: &str) -> Result<(), DatabaseError> {
            Ok(())
        }

        async fn query_raw(&self, _statement: &str) -> Result<Vec<Row>, DatabaseError> {
            Ok(vec![])
        }

        async fn table_exists(&self, _table_name: &str) -> Result<bool, DatabaseError> {
            Ok(false)
        }

        async fn get_table_info(
            &self,
            _table_name: &str,
        ) -> Result<Option<TableInfo>, DatabaseError> {
            Ok(None)
        }

        async fn get_table_columns(
            &self,
            _table_name: &str,
        ) -> Result<Vec<ColumnInfo>, DatabaseError> {
            Ok(vec![])
        }

        async fn column_exists(
            &self,
            _table_name: &str,
            _column_name: &str,
        ) -> Result<bool, DatabaseError> {
            Ok(false)
        }

        async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
            Ok(vec![])
        }

        async fn begin_transaction(
            &self,
        ) -> Result<Box<dyn switchy_database::DatabaseTransaction>, DatabaseError> {
            unimplemented!("Transaction support not implemented for test database")
        }
    }

    fn new_queue() -> DownloadQueue {
        DownloadQueue::new()
            .with_scan(false)
            .with_database(LibraryDatabase {
                database: Arc::new(Box::new(TestDatabase {})),
            })
            .with_downloader(Box::new(TestDownloader {}))
    }

    #[test_log::test(switchy_async::test)]
    async fn test_can_process_single_track_download_task() {
        let mut queue = new_queue();
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("test.m4a")
            .to_str()
            .unwrap()
            .to_string();

        queue
            .add_task_to_queue(DownloadTask {
                id: 1,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 1.into(),
                    source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path,
                created: String::new(),
                updated: String::new(),
                total_bytes: None,
            })
            .await;

        queue.process().await.unwrap().unwrap();
        queue.shutdown().await.unwrap();

        let responses = queue
            .state
            .read()
            .await
            .results
            .iter()
            .map(|result| result.as_ref().ok().cloned())
            .collect::<Vec<_>>();

        assert_eq!(
            responses,
            vec![Some(ProcessDownloadTaskResponse { task_id: 1 })]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_can_process_multiple_track_download_tasks() {
        let mut queue = new_queue();
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("test.m4a")
            .to_str()
            .unwrap()
            .to_string();

        queue
            .add_tasks_to_queue(vec![
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1.into(),
                        source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path: file_path.clone(),
                    created: String::new(),
                    updated: String::new(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 2,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 2.into(),
                        source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path,
                    created: String::new(),
                    updated: String::new(),
                    total_bytes: None,
                },
            ])
            .await;

        queue.process().await.unwrap().unwrap();
        queue.shutdown().await.unwrap();

        let responses = queue
            .state
            .read()
            .await
            .results
            .iter()
            .map(|result| result.as_ref().ok().cloned())
            .collect::<Vec<_>>();

        assert_eq!(
            responses,
            vec![
                Some(ProcessDownloadTaskResponse { task_id: 1 }),
                Some(ProcessDownloadTaskResponse { task_id: 2 })
            ]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_can_process_duplicate_track_download_tasks() {
        let mut queue = new_queue();
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("test.m4a")
            .to_str()
            .unwrap()
            .to_string();

        queue
            .add_tasks_to_queue(vec![
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1.into(),
                        source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path: file_path.clone(),
                    created: String::new(),
                    updated: String::new(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1.into(),
                        source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path,
                    created: String::new(),
                    updated: String::new(),
                    total_bytes: None,
                },
            ])
            .await;

        queue.process().await.unwrap().unwrap();
        queue.shutdown().await.unwrap();

        let responses = queue
            .state
            .read()
            .await
            .results
            .iter()
            .map(|result| result.as_ref().ok().cloned())
            .collect::<Vec<_>>();

        assert_eq!(
            responses,
            vec![Some(ProcessDownloadTaskResponse { task_id: 1 })]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_can_process_another_track_download_task_after_processing_has_already_started() {
        let mut queue = new_queue();
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("test.m4a")
            .to_str()
            .unwrap()
            .to_string();

        queue
            .add_task_to_queue(DownloadTask {
                id: 1,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 1.into(),
                    source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path: file_path.clone(),
                created: String::new(),
                updated: String::new(),
                total_bytes: None,
            })
            .await;

        queue.process();

        queue
            .add_task_to_queue(DownloadTask {
                id: 2,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 2.into(),
                    source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path,
                created: String::new(),
                updated: String::new(),
                total_bytes: None,
            })
            .await;

        switchy_async::time::sleep(std::time::Duration::from_millis(0)).await;

        queue.shutdown().await.unwrap();

        let responses = queue
            .state
            .read()
            .await
            .results
            .iter()
            .map(|result| result.as_ref().ok().cloned())
            .collect::<Vec<_>>();

        assert_eq!(
            responses,
            vec![
                Some(ProcessDownloadTaskResponse { task_id: 1 }),
                Some(ProcessDownloadTaskResponse { task_id: 2 })
            ]
        );
    }

    #[test_log::test]
    fn test_download_queue_state_new_is_empty() {
        let state = DownloadQueueState::new();

        assert!(state.tasks.is_empty());
        assert!(state.results.is_empty());
        assert!(state.current_task().is_none());
    }

    #[test_log::test]
    fn test_download_queue_state_add_task_to_queue() {
        let mut state = DownloadQueueState::new();
        let task = DownloadTask {
            id: 1,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        state.add_task_to_queue(task);

        assert_eq!(state.tasks.len(), 1);
        assert!(state.current_task().is_some());
        assert_eq!(state.current_task().unwrap().id, 1);
    }

    #[test_log::test]
    fn test_download_queue_state_add_tasks_to_queue() {
        let mut state = DownloadQueueState::new();
        let task1 = DownloadTask {
            id: 1,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path1".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };
        let task2 = DownloadTask {
            id: 2,
            state: DownloadTaskState::Pending,
            item: DownloadItem::AlbumCover {
                album_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                artist_id: 1.into(),
                artist: "artist".into(),
                title: "album".into(),
                contains_cover: true,
            },
            file_path: "/test/path2".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        state.add_tasks_to_queue(vec![task1, task2]);

        assert_eq!(state.tasks.len(), 2);
        assert_eq!(state.current_task().unwrap().id, 1);
    }

    #[test_log::test]
    fn test_download_queue_state_finish_task_removes_matching_task() {
        let mut state = DownloadQueueState::new();
        let task = DownloadTask {
            id: 1,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        state.add_task_to_queue(task.clone());
        assert_eq!(state.tasks.len(), 1);

        state.finish_task(&task);
        assert!(state.tasks.is_empty());
    }

    #[test_log::test]
    fn test_download_queue_state_finish_task_does_not_remove_different_path() {
        let mut state = DownloadQueueState::new();
        let task1 = DownloadTask {
            id: 1,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path1".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };
        let task2 = DownloadTask {
            id: 2,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path2".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        state.add_task_to_queue(task1.clone());
        state.add_task_to_queue(task2);

        state.finish_task(&task1);
        assert_eq!(state.tasks.len(), 1);
        assert_eq!(state.tasks[0].file_path, "/test/path2");
    }

    #[test_log::test]
    fn test_download_queue_state_finish_task_does_not_remove_different_item_type() {
        let mut state = DownloadQueueState::new();
        let track_task = DownloadTask {
            id: 1,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };
        let cover_task = DownloadTask {
            id: 2,
            state: DownloadTaskState::Pending,
            item: DownloadItem::AlbumCover {
                album_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                artist_id: 1.into(),
                artist: "artist".into(),
                title: "album".into(),
                contains_cover: true,
            },
            file_path: "/test/path".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        state.add_task_to_queue(track_task.clone());
        state.add_task_to_queue(cover_task);

        // Finishing track_task should only remove the track, not the album cover
        state.finish_task(&track_task);
        assert_eq!(state.tasks.len(), 1);
        assert!(matches!(
            state.tasks[0].item,
            DownloadItem::AlbumCover { .. }
        ));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_has_database_false_initially() {
        let queue = DownloadQueue::new();
        assert!(!queue.has_database());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_has_database_true_after_with_database() {
        let queue = DownloadQueue::new().with_database(LibraryDatabase {
            database: Arc::new(Box::new(TestDatabase {})),
        });
        assert!(queue.has_database());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_has_downloader_false_initially() {
        let queue = DownloadQueue::new();
        assert!(!queue.has_downloader());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_has_downloader_true_after_with_downloader() {
        let queue = DownloadQueue::new().with_downloader(Box::new(TestDownloader {}));
        assert!(queue.has_downloader());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_speed_returns_none_without_downloader() {
        let queue = DownloadQueue::new();
        assert!(queue.speed().is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_current_task_returns_none_when_empty() {
        let queue = DownloadQueue::new();
        assert!(queue.current_task().await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_current_task_returns_first_task() {
        let mut queue = DownloadQueue::new();
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                track_id: 1.into(),
                source: DownloadApiSource::Api(TIDAL_API_SOURCE.clone()),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 1.into(),
                artist: "artist".into(),
                album_id: 1.into(),
                album: "album".into(),
                title: "title".into(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            created: String::new(),
            updated: String::new(),
            total_bytes: None,
        };

        queue.add_task_to_queue(task).await;

        let current = queue.current_task().await;
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, 42);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_default_creates_empty_queue() {
        let queue = DownloadQueue::default();

        assert!(!queue.has_database());
        assert!(!queue.has_downloader());
        assert!(queue.scan);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_with_scan_false() {
        let queue = DownloadQueue::new().with_scan(false);

        assert!(!queue.scan);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_download_queue_with_scan_true() {
        let queue = DownloadQueue::new().with_scan(false).with_scan(true);

        assert!(queue.scan);
    }
}
