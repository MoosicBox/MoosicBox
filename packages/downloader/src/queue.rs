use std::{sync::Arc, time::Duration};

use lazy_static::lazy_static;
use moosicbox_core::{app::Db, sqlite::db::DbError};
use moosicbox_database::{rusqlite::RusqliteDatabase, Database, DatabaseError, DatabaseValue, Row};
use thiserror::Error;
use tokio::{
    sync::{Mutex, RwLock},
    task::{JoinError, JoinHandle},
};

use crate::{
    db::models::{DownloadItem, DownloadTask, DownloadTaskState},
    DownloadAlbumError, DownloadTrackError, Downloader, MoosicboxDownloader,
};

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
    static ref TIMEOUT_DURATION: Option<Duration> = Some(Duration::from_secs(30));
}

#[derive(Debug, Error)]
pub enum UpdateTaskError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
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
}

#[derive(Debug, Clone, PartialEq)]
struct ProcessDownloadTaskResponse {}

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
        self.tasks.retain(|x| {
            !(task.file_path == x.file_path && task.item == x.item && task.item == x.item)
        });
    }
}

#[derive(Clone)]
pub struct DownloadQueue {
    db: Db,
    database: Arc<Box<dyn Database + Send + Sync>>,
    downloader: Arc<Box<dyn Downloader + Send + Sync>>,
    state: Arc<RwLock<DownloadQueueState>>,
    join_handle: Arc<Mutex<Option<JoinHandle<Result<(), ProcessDownloadQueueError>>>>>,
}

impl DownloadQueue {
    pub fn new(db: Db) -> Self {
        Self {
            db: db.clone(),
            database: Arc::new(Box::new(RusqliteDatabase::new(db.library))),
            downloader: Arc::new(Box::new(MoosicboxDownloader::new())),
            state: Arc::new(RwLock::new(DownloadQueueState::new())),
            join_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_database(self, database: Box<dyn Database + Send + Sync>) -> Self {
        Self {
            db: self.db.clone(),
            database: Arc::new(database),
            downloader: self.downloader.clone(),
            state: self.state.clone(),
            join_handle: self.join_handle.clone(),
        }
    }

    pub fn with_downloader(self, downloader: Box<dyn Downloader + Send + Sync>) -> Self {
        Self {
            db: self.db.clone(),
            database: self.database.clone(),
            downloader: Arc::new(downloader),
            state: self.state.clone(),
            join_handle: self.join_handle.clone(),
        }
    }

    pub fn speed(&self) -> f64 {
        self.downloader.speed()
    }

    pub async fn add_task_to_queue(&mut self, task: DownloadTask) {
        self.state.write().await.add_task_to_queue(task);
    }

    pub async fn add_tasks_to_queue(&mut self, tasks: Vec<DownloadTask>) {
        self.state.write().await.add_tasks_to_queue(tasks);
    }

    pub fn process(&mut self) -> JoinHandle<Result<(), ProcessDownloadQueueError>> {
        let join_handle = self.join_handle.clone();
        let this = self.clone();

        tokio::task::spawn(async move {
            let mut handle = join_handle.lock().await;

            if let Some(handle) = handle.as_mut() {
                if !handle.is_finished() {
                    handle.await??;
                }
            }

            handle.replace(RT.spawn(async move {
                this.process_inner().await?;
                Ok(())
            }));

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

    async fn process_inner(&self) -> Result<(), ProcessDownloadQueueError> {
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

        self.update_task(
            task.id,
            &[(
                "state",
                DatabaseValue::String(task.state.as_ref().to_string()),
            )],
        )
        .await
    }

    async fn update_task(
        &self,
        task_id: u64,
        values: &[(&str, DatabaseValue)],
    ) -> Result<Row, UpdateTaskError> {
        Ok(self
            .database
            .update_and_get_row("download_tasks", DatabaseValue::UNumber(task_id), values)
            .await?
            .ok_or(UpdateTaskError::NoRow)?)
    }

    async fn process_task(
        &self,
        task: &mut DownloadTask,
    ) -> Result<ProcessDownloadTaskResponse, ProcessDownloadQueueError> {
        log::debug!("Processing task {task:?}");

        self.update_task_state(task, DownloadTaskState::Started)
            .await?;

        let mut task_size = None;
        let database = self.database.clone();
        let task_id = task.id;
        let on_size = Box::new(move |size| {
            log::debug!("Got size of task: {size:?}");
            if let Some(size) = size {
                task_size.replace(size);
                let database = database.clone();
                tokio::task::spawn(async move {
                    if let Err(err) = database
                        .update_and_get_row(
                            "download_tasks",
                            DatabaseValue::UNumber(task_id),
                            &[("total_bytes", DatabaseValue::UNumber(size))],
                        )
                        .await
                    {
                        log::error!("Failed to set DownloadTask total_bytes: {err:?}");
                    }
                });
            }
        });

        match task.item {
            DownloadItem::Track {
                track_id,
                quality,
                source,
            } => {
                self.downloader
                    .download_track_id(
                        &self.db,
                        &task.file_path,
                        track_id,
                        quality,
                        source,
                        on_size,
                        *TIMEOUT_DURATION,
                    )
                    .await?
            }
            DownloadItem::AlbumCover(album_id) => {
                self.downloader
                    .download_album_cover(&self.db, &task.file_path, album_id, on_size)
                    .await?;
            }
            DownloadItem::ArtistCover(album_id) => {
                self.downloader
                    .download_artist_cover(&self.db, &task.file_path, album_id, on_size)
                    .await?;
            }
        }

        if let Some(size) = task_size {
            task.total_bytes.replace(size);
        }

        self.update_task_state(task, DownloadTaskState::Finished)
            .await?;

        Ok(ProcessDownloadTaskResponse {})
    }
}

impl Drop for DownloadQueue {
    fn drop(&mut self) {
        let handle = self.join_handle.clone();

        tokio::task::spawn(async move {
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
    use moosicbox_database::{DbConnection, Row};
    use moosicbox_files::files::track::TrackAudioQuality;
    use pretty_assertions::assert_eq;

    use crate::db::models::{DownloadApiSource, DownloadItem, DownloadTaskState};

    use super::*;

    struct TestDownloader {}

    #[async_trait]
    impl Downloader for TestDownloader {
        async fn download_track_id(
            &self,
            _db: &Db,
            _path: &str,
            _track_id: u64,
            _quality: moosicbox_files::files::track::TrackAudioQuality,
            _source: crate::db::models::DownloadApiSource,
            _on_size: Box<dyn FnMut(Option<u64>) + Send + Sync>,
            _timeout_duration: Option<Duration>,
        ) -> Result<(), DownloadTrackError> {
            Ok(())
        }

        async fn download_album_cover(
            &self,
            _db: &Db,
            _path: &str,
            _album_id: u64,
            _on_size: Box<dyn FnMut(Option<u64>) + Send + Sync>,
        ) -> Result<(), DownloadAlbumError> {
            Ok(())
        }

        async fn download_artist_cover(
            &self,
            _db: &Db,
            _path: &str,
            _album_id: u64,
            _on_size: Box<dyn FnMut(Option<u64>) + Send + Sync>,
        ) -> Result<(), DownloadAlbumError> {
            Ok(())
        }
    }

    struct TestDatabase {}

    #[async_trait]
    impl Database for TestDatabase {
        async fn update_and_get_row<'a>(
            &self,
            _table_name: &str,
            _id: DatabaseValue,
            _values: &[(&'a str, DatabaseValue)],
        ) -> Result<Option<Row>, DatabaseError> {
            Ok(Some(Row { columns: vec![] }))
        }
    }

    fn new_queue() -> DownloadQueue {
        let library = ::rusqlite::Connection::open_in_memory().unwrap();
        let db = Db {
            library: Arc::new(std::sync::Mutex::new(DbConnection { inner: library })),
        };

        DownloadQueue::new(db)
            .with_database(Box::new(TestDatabase {}))
            .with_downloader(Box::new(TestDownloader {}))
    }

    #[test_log::test(tokio::test)]
    async fn test_can_process_single_track_download_task() {
        let mut queue = new_queue();

        queue
            .add_task_to_queue(DownloadTask {
                id: 1,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 1,
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                },
                file_path: "".into(),
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

        assert_eq!(responses, vec![Some(ProcessDownloadTaskResponse {})]);
    }

    #[test_log::test(tokio::test)]
    async fn test_can_process_multiple_track_download_tasks() {
        let mut queue = new_queue();

        queue
            .add_tasks_to_queue(vec![
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1,
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                    },
                    file_path: "".into(),
                    created: "".into(),
                    updated: "".into(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 2,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 2,
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                    },
                    file_path: "".into(),
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
                Some(ProcessDownloadTaskResponse {}),
                Some(ProcessDownloadTaskResponse {})
            ]
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_can_process_duplicate_track_download_tasks() {
        let mut queue = new_queue();

        queue
            .add_tasks_to_queue(vec![
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1,
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                    },
                    file_path: "".into(),
                    created: "".into(),
                    updated: "".into(),
                    total_bytes: None,
                },
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track {
                        track_id: 1,
                        source: DownloadApiSource::Tidal,
                        quality: TrackAudioQuality::FlacHighestRes,
                    },
                    file_path: "".into(),
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

        assert_eq!(responses, vec![Some(ProcessDownloadTaskResponse {}),]);
    }

    #[test_log::test(tokio::test)]
    async fn test_can_process_another_track_download_task_after_processing_has_already_started() {
        let mut queue = new_queue();

        queue
            .add_task_to_queue(DownloadTask {
                id: 1,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 1,
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                },
                file_path: "".into(),
                created: "".into(),
                updated: "".into(),
                total_bytes: None,
            })
            .await;

        queue.process().await.unwrap().unwrap();

        queue
            .add_task_to_queue(DownloadTask {
                id: 2,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track {
                    track_id: 2,
                    source: DownloadApiSource::Tidal,
                    quality: TrackAudioQuality::FlacHighestRes,
                },
                file_path: "".into(),
                created: "".into(),
                updated: "".into(),
                total_bytes: None,
            })
            .await;

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
                Some(ProcessDownloadTaskResponse {}),
                Some(ProcessDownloadTaskResponse {})
            ]
        );
    }
}
