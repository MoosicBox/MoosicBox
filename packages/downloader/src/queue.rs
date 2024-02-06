use std::{sync::Arc, time::Duration};

use lazy_static::lazy_static;
use moosicbox_core::app::Db;
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
pub enum ProcessDownloadQueueError {
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
            !(task.file_path == x.file_path && task.item == x.item && task.source == x.source)
        });
    }
}

#[derive(Clone)]
pub struct DownloadQueue {
    db: Db,
    downloader: Arc<Box<dyn Downloader + Send + Sync>>,
    state: Arc<RwLock<DownloadQueueState>>,
    join_handle: Arc<Mutex<Option<JoinHandle<Result<(), ProcessDownloadQueueError>>>>>,
}

impl DownloadQueue {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            downloader: Arc::new(Box::new(MoosicboxDownloader {})),
            state: Arc::new(RwLock::new(DownloadQueueState::new())),
            join_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_downloader(self, downloader: Box<dyn Downloader + Send + Sync>) -> Self {
        Self {
            db: self.db.clone(),
            downloader: Arc::new(downloader),
            state: self.state.clone(),
            join_handle: self.join_handle.clone(),
        }
    }

    pub async fn add_task_to_queue(&mut self, task: DownloadTask) {
        self.state.write().await.add_task_to_queue(task);
    }

    pub async fn add_tasks_to_queue(&mut self, tasks: Vec<DownloadTask>) {
        self.state.write().await.add_tasks_to_queue(tasks);
    }

    pub async fn process(&mut self) -> Result<(), ProcessDownloadQueueError> {
        let mut handle = self.join_handle.lock().await;
        if handle.as_ref().is_none() {
            let db = self.db.clone();
            let downloader = self.downloader.clone();
            let state = self.state.clone();
            *handle =
                Some(RT.spawn(async move { Self::process_inner(&db, downloader, state).await }));
        }

        Ok(())
    }

    #[allow(unused)]
    async fn shutdown(&mut self) -> Result<(), ProcessDownloadQueueError> {
        let mut handle = self.join_handle.lock().await;
        if let Some(handle) = handle.as_mut() {
            Ok(handle.await??)
        } else {
            Ok(())
        }
    }

    async fn process_inner(
        db: &Db,
        downloader: Arc<Box<dyn Downloader + Send + Sync>>,
        state: Arc<RwLock<DownloadQueueState>>,
    ) -> Result<(), ProcessDownloadQueueError> {
        while let Some(mut task) = {
            let state = state.as_ref().read().await;
            state.tasks.first().cloned()
        } {
            let result = Self::process_task(db, downloader.clone(), &mut task).await;

            let mut state = state.write().await;

            if let Err(ref err) = result {
                log::error!("Encountered error when processing task in DownloadQueue: {err:?}");
            }

            state.results.push(result);
            state.finish_task(&task);
        }

        Ok(())
    }

    async fn process_task(
        db: &Db,
        downloader: Arc<Box<dyn Downloader + Send + Sync>>,
        task: &mut DownloadTask,
    ) -> Result<ProcessDownloadTaskResponse, ProcessDownloadQueueError> {
        log::debug!("Processing task {task:?}");

        task.state = DownloadTaskState::Started;

        match task.item {
            DownloadItem::Track(track_id) => {
                downloader
                    .download_track_id(
                        db,
                        &task.file_path,
                        track_id,
                        task.quality,
                        task.source,
                        *TIMEOUT_DURATION,
                    )
                    .await?
            }
            DownloadItem::AlbumCover(album_id) => {
                downloader
                    .download_album_cover(db, &task.file_path, album_id)
                    .await?;
            }
            DownloadItem::ArtistCover(album_id) => {
                downloader
                    .download_artist_cover(db, &task.file_path, album_id)
                    .await?;
            }
        }

        task.state = DownloadTaskState::Finished;

        Ok(ProcessDownloadTaskResponse {})
    }
}

impl Drop for DownloadQueue {
    fn drop(&mut self) {
        let handle = self.join_handle.clone();

        tokio::task::spawn(async move {
            let mut handle = handle.lock().await;
            if let Some(handle) = handle.as_mut() {
                if let Err(err) = handle.await {
                    log::error!("Failed to drop DownloadQueue: {err:?}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use moosicbox_core::app::DbConnection;
    use pretty_assertions::assert_eq;

    use crate::db::models::{DownloadItem, DownloadTaskState};

    use super::*;

    struct TestDownloader {}

    #[async_trait]
    impl Downloader for TestDownloader {
        async fn download_track_id(
            &self,
            _db: &Db,
            _path: &str,
            _track_id: u64,
            _quality: Option<moosicbox_files::files::track::TrackAudioQuality>,
            _source: Option<crate::db::models::DownloadApiSource>,
            _timeout_duration: Option<Duration>,
        ) -> Result<(), DownloadTrackError> {
            Ok(())
        }

        async fn download_album_cover(
            &self,
            _db: &Db,
            _path: &str,
            _album_id: u64,
        ) -> Result<(), DownloadAlbumError> {
            Ok(())
        }

        async fn download_artist_cover(
            &self,
            _db: &Db,
            _path: &str,
            _album_id: u64,
        ) -> Result<(), DownloadAlbumError> {
            Ok(())
        }
    }

    fn new_queue() -> DownloadQueue {
        let library = ::rusqlite::Connection::open_in_memory().unwrap();
        let db = Db {
            library: Arc::new(std::sync::Mutex::new(DbConnection { inner: library })),
        };

        DownloadQueue::new(db).with_downloader(Box::new(TestDownloader {}))
    }

    #[test_log::test(tokio::test)]
    async fn test_can_process_single_track_download_task() {
        let mut queue = new_queue();

        queue
            .add_task_to_queue(DownloadTask {
                id: 1,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track(1),
                source: None,
                quality: None,
                file_path: "".into(),
                progress: 0.0,
                bytes: 0,
                speed: None,
                created: "".into(),
                updated: "".into(),
            })
            .await;

        queue.process().await.unwrap();
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
                    item: DownloadItem::Track(1),
                    source: None,
                    quality: None,
                    file_path: "".into(),
                    progress: 0.0,
                    bytes: 0,
                    speed: None,
                    created: "".into(),
                    updated: "".into(),
                },
                DownloadTask {
                    id: 2,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track(2),
                    source: None,
                    quality: None,
                    file_path: "".into(),
                    progress: 0.0,
                    bytes: 0,
                    speed: None,
                    created: "".into(),
                    updated: "".into(),
                },
            ])
            .await;

        queue.process().await.unwrap();
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
                    item: DownloadItem::Track(1),
                    source: None,
                    quality: None,
                    file_path: "".into(),
                    progress: 0.0,
                    bytes: 0,
                    speed: None,
                    created: "".into(),
                    updated: "".into(),
                },
                DownloadTask {
                    id: 1,
                    state: DownloadTaskState::Pending,
                    item: DownloadItem::Track(1),
                    source: None,
                    quality: None,
                    file_path: "".into(),
                    progress: 0.0,
                    bytes: 0,
                    speed: None,
                    created: "".into(),
                    updated: "".into(),
                },
            ])
            .await;

        queue.process().await.unwrap();
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
                item: DownloadItem::Track(1),
                source: None,
                quality: None,
                file_path: "".into(),
                progress: 0.0,
                bytes: 0,
                speed: None,
                created: "".into(),
                updated: "".into(),
            })
            .await;

        queue.process().await.unwrap();

        queue
            .add_task_to_queue(DownloadTask {
                id: 2,
                state: DownloadTaskState::Pending,
                item: DownloadItem::Track(2),
                source: None,
                quality: None,
                file_path: "".into(),
                progress: 0.0,
                bytes: 0,
                speed: None,
                created: "".into(),
                updated: "".into(),
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
