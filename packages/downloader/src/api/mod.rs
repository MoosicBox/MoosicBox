use std::{
    path::PathBuf,
    str::FromStr as _,
    sync::{Arc, LazyLock},
};

use crate::{
    api::models::{ApiDownloadLocation, ApiDownloadTask, ApiDownloadTaskState},
    create_download_tasks,
    db::{
        create_download_location, get_download_locations, get_download_tasks,
        models::DownloadTaskState,
    },
    get_create_download_tasks, get_download_path,
    queue::{DownloadQueue, ProcessDownloadQueueError, ProgressListenerRef},
    CreateDownloadTasksError, DownloadApiSource, GetCreateDownloadTasksError, GetDownloadPathError,
    MoosicboxDownloader,
};
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_music_api::{models::TrackAudioQuality, MusicApis, SourceToMusicApi as _};
use moosicbox_paging::Page;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub mod models;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(download_endpoint)
        .service(retry_download_endpoint)
        .service(download_tasks_endpoint)
        .service(get_download_locations_endpoint)
        .service(add_download_location_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Downloader")),
    paths(
        download_endpoint,
        retry_download_endpoint,
        download_tasks_endpoint,
        get_download_locations_endpoint,
        add_download_location_endpoint
    ),
    components(schemas(
        DownloadApiSource,
        moosicbox_music_models::id::Id,
        TrackAudioQuality
    ))
)]
pub struct Api;

static DOWNLOAD_QUEUE: LazyLock<Arc<RwLock<DownloadQueue>>> =
    LazyLock::new(|| Arc::new(RwLock::new(DownloadQueue::new())));

pub async fn add_progress_listener_to_download_queue(listener: ProgressListenerRef) {
    let mut queue = DOWNLOAD_QUEUE.write().await;
    *queue = queue.clone().add_progress_listener(listener);
}

async fn get_default_download_queue(
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Arc<RwLock<DownloadQueue>> {
    let queue = { DOWNLOAD_QUEUE.read().await.clone() };

    if !queue.has_database() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        *queue = queue.clone().with_database(db.clone());
    }
    if !queue.has_downloader() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        *queue = queue
            .clone()
            .with_downloader(Box::new(MoosicboxDownloader::new(db, music_apis)));
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
    track_id: Option<String>,
    track_ids: Option<String>,
    album_id: Option<String>,
    album_ids: Option<String>,
    download_album_cover: Option<bool>,
    download_artist_cover: Option<bool>,
    quality: Option<TrackAudioQuality>,
    source: DownloadApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/download",
        description = "Queue the specified tracks or albums to be downloaded",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("locationId" = Option<u64>, Query, description = "The download location to save the files to"),
            ("trackId" = Option<String>, Query, description = "A trackId to download"),
            ("trackIds" = Option<String>, Query, description = "A comma-separated list of trackIds to download"),
            ("albumId" = Option<String>, Query, description = "A albumId to download"),
            ("albumIds" = Option<String>, Query, description = "A comma-separated list of albumIds to download"),
            ("downloadAlbumCover" = Option<bool>, Query, description = "Whether or not to download the album cover, if available"),
            ("downloadArtistCover" = Option<bool>, Query, description = "Whether or not to download the artist cover, if available"),
            ("quality" = Option<TrackAudioQuality>, Query, description = "The track audio quality to download the tracks at"),
            ("source" = DownloadApiSource, Query, description = "The API source to download the track from"),
        ),
        responses(
            (
                status = 200,
                description = "The download has successfully started",
                body = Value,
            )
        )
    )
)]
#[route("/download", method = "POST")]
#[allow(clippy::future_not_send)]
pub async fn download_endpoint(
    query: web::Query<DownloadQuery>,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Json<Value>> {
    let download_path = get_download_path(&db, query.location_id).await?;

    let tasks = get_create_download_tasks(
        &**music_apis
            .get(query.source.into())
            .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
        &download_path,
        query.track_id.clone(),
        query.track_ids.clone(),
        query.album_id.clone(),
        query.album_ids.clone(),
        query.download_album_cover.unwrap_or(true),
        query.download_artist_cover.unwrap_or(true),
        query.quality,
        Some(query.source),
    )
    .await?;

    let tasks = create_download_tasks(&db, tasks).await?;

    let queue = get_default_download_queue(db.clone(), music_apis).await;
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    download_queue.process();

    drop(download_queue);

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryDownloadQuery {
    task_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/retry-download",
        description = "Retry a specific download task",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("taskId" = u64, Query, description = "The task ID to retry"),
        ),
        responses(
            (
                status = 200,
                description = "Retry a specific download task",
                body = Value,
            )
        )
    )
)]
#[route("/retry-download", method = "POST")]
pub async fn retry_download_endpoint(
    query: web::Query<RetryDownloadQuery>,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<Json<Value>> {
    let tasks = get_download_tasks(&db)
        .await
        .map_err(ErrorInternalServerError)?;
    let task = tasks
        .into_iter()
        .find(|x| x.id == query.task_id)
        .ok_or_else(|| ErrorNotFound(format!("Task not found with ID {}", query.task_id)))?;

    let mut download_queue = DownloadQueue::new()
        .with_database(db.clone())
        .with_downloader(Box::new(MoosicboxDownloader::new(db, music_apis)));
    download_queue.add_tasks_to_queue(vec![task]).await;
    download_queue.process();

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadTasks {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        get,
        path = "/download-tasks",
        description = "Get a list of the current and historical download tasks",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of download items",
                body = Value,
            )
        )
    )
)]
#[route("/download-tasks", method = "GET")]
pub async fn download_tasks_endpoint(
    query: web::Query<GetDownloadTasks>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiDownloadTask>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let tasks = get_download_tasks(&db)
        .await
        .map_err(ErrorInternalServerError)?;
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

    #[allow(clippy::tuple_array_conversions)]
    let tasks = [current, history].concat();
    let total = u32::try_from(tasks.len()).unwrap();
    let mut tasks = tasks
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect::<Vec<ApiDownloadTask>>();

    for task in &mut tasks {
        if task.state == ApiDownloadTaskState::Started {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            task.speed
                .replace(DOWNLOAD_QUEUE.read().await.speed().unwrap_or(0.0) as u64);
        }
    }

    Ok(Json(Page::WithTotal {
        offset,
        items: tasks,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadLocations {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        get,
        path = "/download-locations",
        description = "Get a list of the download locations",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of download locations",
                body = Value,
            )
        )
    )
)]
#[route("/download-locations", method = "GET")]
pub async fn get_download_locations_endpoint(
    query: web::Query<GetDownloadLocations>,
    db: LibraryDatabase,
) -> Result<Json<Page<ApiDownloadLocation>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let locations = get_download_locations(&db)
        .await
        .map_err(ErrorInternalServerError)?;
    let total = u32::try_from(locations.len()).unwrap();
    let locations = locations
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(Into::into)
        .collect::<Vec<ApiDownloadLocation>>();

    Ok(Json(Page::WithTotal {
        offset,
        items: locations,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDownloadLocation {
    path: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/download-locations",
        description = "Add a download location",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("path" = String, Query, description = "The download location path"),
        ),
        responses(
            (
                status = 200,
                description = "The successfully created download location",
                body = Value,
            )
        )
    )
)]
#[route("/download-locations", method = "POST")]
#[allow(clippy::future_not_send)]
pub async fn add_download_location_endpoint(
    query: web::Query<AddDownloadLocation>,
    db: LibraryDatabase,
) -> Result<Json<ApiDownloadLocation>> {
    static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

    let path = if std::env::consts::OS == "windows" {
        REGEX
            .replace(&query.path, |caps: &Captures| {
                format!("{}:", caps[1].to_uppercase())
            })
            .replace('/', "\\")
    } else {
        query.path.clone()
    };

    let path = PathBuf::from_str(&path)?.canonicalize()?;

    let location = create_download_location(
        &db,
        path.to_str()
            .ok_or_else(|| ErrorBadRequest(format!("Invalid path: {path:?}")))?,
    )
    .await
    .map_err(ErrorInternalServerError)?;

    Ok(Json(location.into()))
}
