use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::api::models::ApiDownloadTask;
use crate::create_download_tasks;
use crate::db::get_download_location;
use crate::db::get_download_tasks;
use crate::db::models::CreateDownloadTask;
use crate::db::models::DownloadItem;
use crate::queue::DownloadQueue;
use crate::queue::ProcessDownloadQueueError;
use crate::CreateDownloadTasksError;
use crate::DownloadAlbumError;
use crate::DownloadApiSource;
use crate::DownloadTrackError;
use actix_web::error::ErrorInternalServerError;
use actix_web::error::ErrorNotFound;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_config::get_config_dir_path;
use moosicbox_core::app::Db;
use moosicbox_core::integer_range::parse_integer_ranges;
use moosicbox_core::sqlite::db::get_album_tracks;
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

impl From<DownloadTrackError> for actix_web::Error {
    fn from(err: DownloadTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<DownloadAlbumError> for actix_web::Error {
    fn from(err: DownloadAlbumError) -> Self {
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
    let path = if let Some(location_id) = query.location_id {
        PathBuf::from_str(
            &get_download_location(
                &data
                    .db
                    .as_ref()
                    .unwrap()
                    .library
                    .lock()
                    .as_ref()
                    .unwrap()
                    .inner,
                location_id,
            )?
            .ok_or(ErrorNotFound("Database Location with id not found"))?
            .path,
        )
        .unwrap()
    } else {
        get_config_dir_path()
            .ok_or(ErrorInternalServerError(
                "Failed to get moosicbox config dir",
            ))?
            .join("downloads")
    };

    let path_str = path.to_str().unwrap();

    let mut tasks = vec![];

    if let Some(track_id) = query.track_id {
        tasks.push(CreateDownloadTask {
            file_path: path_str.to_string(),
            item: DownloadItem::Track(track_id),
            source: query.source,
            quality: query.quality,
        });
    }

    if let Some(track_ids) = &query.track_ids {
        let track_ids = parse_integer_ranges(track_ids)?;

        tasks.extend(
            track_ids
                .into_iter()
                .map(|track_id| CreateDownloadTask {
                    file_path: path_str.to_string(),
                    item: DownloadItem::Track(track_id),
                    source: query.source,
                    quality: query.quality,
                })
                .collect::<Vec<_>>(),
        );
    }

    if let Some(album_id) = query.album_id {
        let tracks = get_album_tracks(
            &data
                .db
                .as_ref()
                .unwrap()
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            album_id as i32,
        )?;

        tasks.extend(
            tracks
                .into_iter()
                .map(|track| track.id as u64)
                .map(|track_id| CreateDownloadTask {
                    file_path: path_str.to_string(),
                    item: DownloadItem::Track(track_id),
                    source: query.source,
                    quality: query.quality,
                })
                .collect::<Vec<_>>(),
        );

        if query.download_album_cover.unwrap_or(true) {
            tasks.push(CreateDownloadTask {
                file_path: path_str.to_string(),
                item: DownloadItem::AlbumCover(album_id),
                source: query.source,
                quality: query.quality,
            });
        }
        if query.download_artist_cover.unwrap_or(true) {
            tasks.push(CreateDownloadTask {
                file_path: path_str.to_string(),
                item: DownloadItem::ArtistCover(album_id),
                source: query.source,
                quality: query.quality,
            });
        }
    }

    if let Some(album_ids) = &query.album_ids {
        let album_ids = parse_integer_ranges(album_ids)?;

        for album_id in album_ids {
            let tracks = get_album_tracks(
                &data
                    .db
                    .as_ref()
                    .unwrap()
                    .library
                    .lock()
                    .as_ref()
                    .unwrap()
                    .inner,
                album_id as i32,
            )?;

            tasks.extend(
                tracks
                    .into_iter()
                    .map(|track| track.id as u64)
                    .map(|track_id| CreateDownloadTask {
                        file_path: path_str.to_string(),
                        item: DownloadItem::Track(track_id),
                        source: query.source,
                        quality: query.quality,
                    })
                    .collect::<Vec<_>>(),
            );

            if query.download_album_cover.unwrap_or(true) {
                tasks.push(CreateDownloadTask {
                    file_path: path_str.to_string(),
                    item: DownloadItem::AlbumCover(album_id),
                    source: query.source,
                    quality: query.quality,
                });
            }
            if query.download_artist_cover.unwrap_or(true) {
                tasks.push(CreateDownloadTask {
                    file_path: path_str.to_string(),
                    item: DownloadItem::ArtistCover(album_id),
                    source: query.source,
                    quality: query.quality,
                });
            }
        }
    }

    let tasks = create_download_tasks(&data.db.as_ref().unwrap(), tasks)?;

    let db = data.db.clone().unwrap();
    let queue = get_default_download_queue(db);
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    download_queue.process().await?;

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
