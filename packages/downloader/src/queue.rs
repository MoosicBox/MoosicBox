use std::{path::PathBuf, pin::Pin, str::FromStr as _, sync::Arc, time::Duration};

use futures::Future;
use lazy_static::lazy_static;
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{profiles::LibraryDatabase, query::*, DatabaseError, DatabaseValue, Row};
use moosicbox_scan::local::ScanItem;
use thiserror::Error;
use tokio::{
    sync::{Mutex, RwLock},
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;

use crate::{
    db::models::{DownloadItem, DownloadTask, DownloadTaskState},
    DownloadAlbumError, DownloadTrackError, Downloader,
};

lazy_static! {
    static ref TIMEOUT_DURATION: Option<Duration> = Some(Duration::from_secs(30));
}

#[derive(Debug, Error)]
pub enum UpdateTaskError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("No database")]
    NoDatabase,
    #[error("No row")]
    NoRow,
}

#[derive(Debug, Error)]
pub enum ProcessDownloadQueueError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    UpdateTask(#[from] UpdateTaskError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    #[error(transparent)]
    DownloadAlbum(#[from] DownloadAlbumError),
    #[error(transparent)]
    LocalScan(#[from] moosicbox_scan::local::ScanError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("No database")]
    NoDatabase,
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
    fn new() -> Self {
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
}

#[derive(Clone)]
pub enum GenericProgressEvent {
    Size { bytes: Option<u64> },
    Speed { bytes_per_second: f64 },
    BytesRead { read: usize, total: usize },
}

#[derive(Clone)]
pub enum ProgressEvent {
    Size {
        task: DownloadTask,
        bytes: Option<u64>,
    },
    Speed {
        task: DownloadTask,
        bytes_per_second: f64,
    },
    BytesRead {
        task: DownloadTask,
        read: usize,
        total: usize,
    },
    State {
        task: DownloadTask,
        state: DownloadTaskState,
    },
}

pub type ProgressListenerFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListener =
    Box<dyn (FnMut(GenericProgressEvent) -> ProgressListenerFut) + Send + Sync>;
pub type ProgressListenerRefFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListenerRef =
    Box<dyn (Fn(&ProgressEvent) -> ProgressListenerRefFut) + Send + Sync>;

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

    pub fn has_database(&self) -> bool {
        self.database.is_some()
    }

    pub fn with_database(mut self, database: LibraryDatabase) -> Self {
        self.database.replace(database);
        self
    }

    pub fn has_downloader(&self) -> bool {
        self.downloader.is_some()
    }

    pub fn with_downloader(mut self, downloader: Box<dyn Downloader + Send + Sync>) -> Self {
        self.downloader.replace(Arc::new(downloader));
        self
    }

    pub fn add_progress_listener(mut self, listener: ProgressListenerRef) -> Self {
        self.progress_listeners.push(Arc::new(listener));
        self
    }

    pub fn with_scan(mut self, scan: bool) -> Self {
        self.scan = scan;
        self
    }

    pub fn speed(&self) -> Option<f64> {
        self.downloader
            .clone()
            .and_then(|downloader| downloader.speed())
    }

    pub async fn add_task_to_queue(&mut self, task: DownloadTask) {
        self.state.write().await.add_task_to_queue(task);
    }

    pub async fn add_tasks_to_queue(&mut self, tasks: Vec<DownloadTask>) {
        self.state.write().await.add_tasks_to_queue(tasks);
    }

    pub fn process(&mut self) -> JoinHandle<Result<(), ProcessDownloadQueueError>> {
        let join_handle = self.join_handle.clone();
        let mut this = self.clone();

        moosicbox_task::spawn("downloader: queue - process", async move {
            let mut handle = join_handle.lock().await;

            if let Some(handle) = handle.as_mut() {
                if !handle.is_finished() {
                    handle.await??;
                }
            }

            handle.replace(moosicbox_task::spawn(
                "downloader: queue - process_inner",
                async move {
                    this.process_inner().await?;
                    Ok(())
                },
            ));

            Ok::<_, ProcessDownloadQueueError>(())
        })
    }

    #[allow(unused)]
    async fn shutdown(&mut self) -> Result<(), ProcessDownloadQueueError> {
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

    async fn process_inner(&mut self) -> Result<(), ProcessDownloadQueueError> {
        while let Some(mut task) = {
            let state = self.state.as_ref().read().await;
            state.tasks.first().cloned()
        } {
            let result = self.process_task(&mut task).await;

            let mut state = self.state.write().await;

            if let Err(ref err) = result {
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

        for listener in self.progress_listeners.iter() {
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
            .execute_first(&db)
            .await?
            .ok_or(UpdateTaskError::NoRow)
    }

    #[allow(unreachable_code)]
    #[allow(unused)]
    async fn process_task(
        &mut self,
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
                                moosicbox_task::spawn(
                                    "downloader: queue - on_progress - size",
                                    async move {
                                        if let Err(err) = database
                                            .update("download_tasks")
                                            .where_eq("id", task_id)
                                            .value("total_bytes", size)
                                            .execute_first(&database)
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
                        GenericProgressEvent::Speed { .. } => {}
                        GenericProgressEvent::BytesRead { .. } => {}
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
                    for listener in listeners.iter() {
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
                Some(
                    moosicbox_scan::Scanner::new(moosicbox_scan::event::ScanTask::Local {
                        paths: vec![path.parent().unwrap().to_str().unwrap().to_string()],
                    })
                    .await,
                )
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
                        *source,
                        on_progress,
                        *TIMEOUT_DURATION,
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
                    .download_album_cover(&task.file_path, album_id, *source, on_progress)
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
                    .download_artist_cover(&task.file_path, album_id, *source, on_progress)
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

        moosicbox_task::spawn("downloader: queue - drop", async move {
            let mut handle = handle.lock().await;
            if let Some(handle) = handle.as_mut() {
                if !handle.is_finished() {
                    if let Err(err) = handle.await {
                        log::error!("Failed to drop DownloadQueue: {err:?}");
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use moosicbox_core::sqlite::models::{Album, Artist, Id, Track};
    use moosicbox_database::{query::*, Database, Row};
    use moosicbox_music_api::TrackAudioQuality;
    use pretty_assertions::assert_eq;

    use crate::db::models::{DownloadApiSource, DownloadItem, DownloadTaskState};

    use super::*;

    struct TestDownloader {}

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
            Ok(Artist {
                ..Default::default()
            })
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
    }

    fn new_queue() -> DownloadQueue {
        DownloadQueue::new()
            .with_scan(false)
            .with_database(LibraryDatabase {
                database: Arc::new(Box::new(TestDatabase {})),
            })
            .with_downloader(Box::new(TestDownloader {}))
    }

    #[test_log::test(tokio::test)]
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
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path,
                created: "".into(),
                updated: "".into(),
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

    #[test_log::test(tokio::test)]
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
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path: file_path.clone(),
                    created: "".into(),
                    updated: "".into(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 2,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 2.into(),
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path,
                    created: "".into(),
                    updated: "".into(),
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

    #[test_log::test(tokio::test)]
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
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path: file_path.clone(),
                    created: "".into(),
                    updated: "".into(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1.into(),
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                        artist_id: 1.into(),
                        artist: "artist".into(),
                        album_id: 1.into(),
                        album: "album".into(),
                        title: "title".into(),
                        contains_cover: false,
                    },
                    file_path,
                    created: "".into(),
                    updated: "".into(),
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

    #[test_log::test(tokio::test)]
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
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path: file_path.clone(),
                created: "".into(),
                updated: "".into(),
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
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                    artist_id: 1.into(),
                    artist: "artist".into(),
                    album_id: 1.into(),
                    album: "album".into(),
                    title: "title".into(),
                    contains_cover: false,
                },
                file_path,
                created: "".into(),
                updated: "".into(),
                total_bytes: None,
            })
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(0)).await;

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
}
