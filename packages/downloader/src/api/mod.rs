use std::sync::Arc;
use std::sync::OnceLock;

use crate::api::models::ApiDownloadTask;
use crate::create_download_tasks;
use crate::db::get_download_tasks;
use crate::get_create_download_tasks;
use crate::get_download_path;
use crate::queue::DownloadQueue;
use crate::queue::ProcessDownloadQueueError;
use crate::CreateDownloadTasksError;
use crate::DownloadApiSource;
use crate::GetCreateDownloadTasksError;
use crate::GetDownloadPathError;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::Db;
use moosicbox_files::files::track::TrackAudioQuality;
use moosicbox_paging::Page;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub mod models;

static DOWNLOAD_QUEUE: OnceLock<Arc<RwLock<DownloadQueue>>> = OnceLock::new();

fn get_default_download_queue(db: Db) -> Arc<RwLock<DownloadQueue>> {
    DOWNLOAD_QUEUE
        .get_or_init(|| Arc::new(RwLock::new(DownloadQueue::new(db))))
        .clone()
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
    let download_path = get_download_path(&data.db.as_ref().unwrap(), query.location_id)?;

    let tasks = get_create_download_tasks(
        &data.db.as_ref().unwrap(),
        &download_path,
        query.track_id,
        query.track_ids.clone(),
        query.album_id,
        query.album_ids.clone(),
        query.download_album_cover.unwrap_or(true),
        query.download_artist_cover.unwrap_or(true),
        query.quality,
        query.source,
    )?;

    let tasks = create_download_tasks(&data.db.as_ref().unwrap(), tasks)?;

    let db = data.db.clone().unwrap();
    let queue = get_default_download_queue(db);
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    let _ = download_queue.process();

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
    let tasks = get_download_tasks(&data.db.as_ref().unwrap().library.lock().unwrap().inner)?;

    Ok(Json(Page::WithTotal {
        offset: 0,
        limit: tasks.len() as u32,
        total: tasks.len() as u32,
        items: tasks
            .into_iter()
            .map(|task| task.into())
            .collect::<Vec<_>>(),
    }))
}
