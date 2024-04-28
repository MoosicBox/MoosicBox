use std::collections::HashSet;
use std::sync::Arc;

use crate::api::models::to_api_download_task;
use crate::api::models::ApiDownloadTask;
use crate::api::models::ApiDownloadTaskState;
use crate::create_download_tasks;
use crate::db::get_download_tasks;
use crate::db::models::DownloadItem;
use crate::db::models::DownloadTaskState;
use crate::get_create_download_tasks;
use crate::get_download_path;
use crate::queue::{DownloadQueue, ProcessDownloadQueueError, ProgressListenerRef};
use crate::CreateDownloadTasksError;
use crate::DownloadApiSource;
use crate::GetCreateDownloadTasksError;
use crate::GetDownloadPathError;
use crate::MoosicboxDownloader;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::sqlite::db::{get_artists, get_tracks};
use moosicbox_core::sqlite::menu::get_albums;
use moosicbox_database::Database;
use moosicbox_files::files::track::TrackAudioQuality;
use moosicbox_paging::Page;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub mod models;

static DOWNLOAD_QUEUE: Lazy<Arc<RwLock<DownloadQueue>>> =
    Lazy::new(|| Arc::new(RwLock::new(DownloadQueue::new())));

pub async fn add_progress_listener_to_download_queue(listener: ProgressListenerRef) {
    DOWNLOAD_QUEUE.write().await.add_progress_listener(listener);
}

async fn get_default_download_queue(db: Arc<Box<dyn Database>>) -> Arc<RwLock<DownloadQueue>> {
    let queue = { DOWNLOAD_QUEUE.read().await.clone() };

    if !queue.has_database() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        let output = queue.with_database(db.clone());
        *queue = output.clone();
    }
    if !queue.has_downloader() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        let output = queue.with_downloader(Box::new(MoosicboxDownloader::new(db)));
        *queue = output.clone();
    }

    DOWNLOAD_QUEUE.clone()
}

impl From<GetDownloadPathError> for actix_web::Error {
    fn from(err: GetDownloadPathError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<GetCreateDownloadTasksError> for actix_web::Error {
    fn from(err: GetCreateDownloadTasksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<CreateDownloadTasksError> for actix_web::Error {
    fn from(err: CreateDownloadTasksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<ProcessDownloadQueueError> for actix_web::Error {
    fn from(err: ProcessDownloadQueueError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    location_id: Option<u64>,
    track_id: Option<u64>,
    track_ids: Option<String>,
    album_id: Option<u64>,
    album_ids: Option<String>,
    download_album_cover: Option<bool>,
    download_artist_cover: Option<bool>,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
}

#[route("/download", method = "POST")]
pub async fn download_endpoint(
    query: web::Query<DownloadQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let download_path = get_download_path(&**data.database, query.location_id).await?;

    let tasks = get_create_download_tasks(
        &**data.database,
        &download_path,
        query.track_id,
        query.track_ids.clone(),
        query.album_id,
        query.album_ids.clone(),
        query.download_album_cover.unwrap_or(true),
        query.download_artist_cover.unwrap_or(true),
        query.quality,
        query.source,
    )
    .await?;

    let tasks = create_download_tasks(&**data.database, tasks).await?;

    let queue = get_default_download_queue(data.database.clone()).await;
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    download_queue.process();

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadTasks {}

#[route("/download-tasks", method = "GET")]
pub async fn download_tasks_endpoint(
    _query: web::Query<GetDownloadTasks>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiDownloadTask>>> {
    let tasks = get_download_tasks(&**data.database).await?;
    let (mut current, mut history): (Vec<_>, Vec<_>) =
        tasks.into_iter().partition(|task| match task.state {
            DownloadTaskState::Pending | DownloadTaskState::Paused | DownloadTaskState::Started => {
                true
            }
            DownloadTaskState::Cancelled
            | DownloadTaskState::Finished
            | DownloadTaskState::Error => false,
        });

    current.sort_by(|a, b| a.id.cmp(&b.id));
    history.sort_by(|a, b| b.id.cmp(&a.id));

    let tasks = [current, history].concat();

    let track_ids = tasks
        .iter()
        .filter_map(|task| {
            if let DownloadItem::Track { track_id, .. } = task.item {
                Some(track_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let tracks = get_tracks(&**data.database, Some(&track_ids)).await?;

    let album_ids = tasks
        .iter()
        .filter_map(|task| {
            if let DownloadItem::AlbumCover(album_id) = task.item {
                Some(album_id as i32)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let albums = get_albums(&**data.database)
        .await?
        .iter()
        .filter(|album| album_ids.contains(&album.id))
        .cloned()
        .collect::<Vec<_>>();

    let artist_ids = albums
        .iter()
        .map(|album| album.artist_id)
        .collect::<HashSet<_>>();

    let artists = get_artists(&**data.database)
        .await?
        .iter()
        .filter(|artist| artist_ids.contains(&artist.id))
        .cloned()
        .collect::<Vec<_>>();

    let len = tasks.len() as u32;

    let mut items = tasks
        .into_iter()
        .map(|task| to_api_download_task(task, &tracks, &albums, &artists))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            ErrorInternalServerError(format!("Failed to get download tasks: {err:?}"))
        })?;

    for item in items.iter_mut() {
        if item.state == ApiDownloadTaskState::Started {
            item.speed
                .replace(DOWNLOAD_QUEUE.read().await.speed().unwrap_or(0.0) as u64);
        }
    }

    Ok(Json(Page::WithTotal {
        offset: 0,
        items,
        limit: len,
        total: len,
    }))
}
