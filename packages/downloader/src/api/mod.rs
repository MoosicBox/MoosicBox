//! HTTP API endpoints for the download service.
//!
//! Provides REST API endpoints for managing downloads, download tasks, and download
//! locations via HTTP requests. Available when the `api` feature is enabled.

#![allow(clippy::needless_for_each)]

use std::{path::PathBuf, str::FromStr as _, sync::LazyLock};

use crate::{
    CreateDownloadTasksError, DOWNLOAD_QUEUE, DownloadApiSource, DownloadError, DownloadRequest,
    GetCreateDownloadTasksError, GetDownloadPathError, MoosicboxDownloader,
    api::models::{ApiDownloadLocation, ApiDownloadTask, ApiDownloadTaskState},
    db::{
        create_download_location, delete_download_location, delete_download_task,
        get_download_locations, get_download_tasks, models::DownloadTaskState,
    },
    download, get_download_path,
    queue::{DownloadQueue, ProcessDownloadQueueError, ProgressListenerRef},
};
use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
};
use moosicbox_auth::NonTunnelRequestAuthorized;
use moosicbox_music_api::{MusicApis, models::TrackAudioQuality};
use moosicbox_music_models::{
    ApiSource,
    id::{Id, parse_id_ranges},
};
use moosicbox_paging::Page;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json::Value;
use switchy_database::profiles::LibraryDatabase;

pub mod models;

/// Binds download API service endpoints to an actix-web scope.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(download_endpoint)
        .service(retry_download_endpoint)
        .service(delete_download_endpoint)
        .service(download_tasks_endpoint)
        .service(get_download_locations_endpoint)
        .service(add_download_location_endpoint)
        .service(remove_download_location_endpoint)
}

/// `OpenAPI` specification for the downloader API endpoints.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Downloader")),
    paths(
        download_endpoint,
        retry_download_endpoint,
        delete_download_endpoint,
        download_tasks_endpoint,
        get_download_locations_endpoint,
        add_download_location_endpoint,
        remove_download_location_endpoint
    ),
    components(schemas(
        DownloadApiSource,
        moosicbox_music_models::id::Id,
        TrackAudioQuality
    ))
)]
pub struct Api;

/// Adds a progress listener to the global download queue.
pub async fn add_progress_listener_to_download_queue(listener: ProgressListenerRef) {
    let mut queue = DOWNLOAD_QUEUE.write().await;
    *queue = queue.clone().add_progress_listener(listener);
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

/// Query parameters for the download endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    /// Download location ID
    location_id: Option<u64>,
    /// Single track ID to download
    track_id: Option<String>,
    /// Multiple track IDs to download (comma-separated)
    track_ids: Option<String>,
    /// Single album ID to download
    album_id: Option<String>,
    /// Multiple album IDs to download (comma-separated)
    album_ids: Option<String>,
    /// Whether to download album cover
    download_album_cover: Option<bool>,
    /// Whether to download artist cover
    download_artist_cover: Option<bool>,
    /// Audio quality for tracks
    quality: Option<TrackAudioQuality>,
    /// API source identifier
    source: String,
    /// `MoosicBox` server URL
    url: Option<String>,
}

impl From<DownloadError> for actix_web::Error {
    fn from(err: DownloadError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
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
            ("source" = String, Query, description = "The API source to download the track from"),
            ("url" = Option<String>, Query, description = "The MoosicBox URL to download the audio from"),
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

    let api_source = match query.source.as_str() {
        "MoosicBox" | "MOOSIC_BOX" => ApiSource::library(),
        api => api.try_into().map_err(ErrorBadRequest)?,
    };

    let track_id = if let Some(track_id) = &query.track_id {
        Some(Id::try_from_str(track_id, &api_source).map_err(ErrorBadRequest)?)
    } else {
        None
    };

    let album_id = if let Some(album_id) = &query.album_id {
        Some(Id::try_from_str(album_id, &api_source).map_err(ErrorBadRequest)?)
    } else {
        None
    };

    let track_ids = if let Some(track_ids) = &query.track_ids {
        Some(parse_id_ranges(track_ids, &api_source).map_err(ErrorBadRequest)?)
    } else {
        None
    };

    let album_ids = if let Some(album_ids) = &query.album_ids {
        Some(parse_id_ranges(album_ids, &api_source).map_err(ErrorBadRequest)?)
    } else {
        None
    };

    let source = match query.source.as_str() {
        "MoosicBox" | "MOOSIC_BOX" => DownloadApiSource::MoosicBox(
            query
                .url
                .clone()
                .ok_or_else(|| ErrorBadRequest("Missing MoosicBox url"))?,
        ),
        api => DownloadApiSource::Api(api.try_into().map_err(ErrorBadRequest)?),
    };

    let quality = query.quality;

    log::debug!(
        "\
        GET /download: \
        download_path={download_path} \
        source={source:?} \
        quality={quality:?} \
        track_id={track_id:?} \
        track_ids={track_ids:?} \
        album_id={album_id:?} \
        album_ids={album_ids:?}\
        ",
        download_path = download_path.display(),
    );

    let request = DownloadRequest {
        directory: download_path,
        track_id,
        track_ids,
        album_id,
        album_ids,
        download_album_cover: query.download_album_cover,
        download_artist_cover: query.download_artist_cover,
        quality,
        source,
    };

    download(request, db, music_apis).await?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Query parameters for the retry download endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryDownloadQuery {
    /// Task ID to retry
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

/// Query parameters for the delete download task endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDownloadTaskQuery {
    /// Task ID to delete
    task_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/delete-download",
        description = "Delete a specific download task",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("taskId" = u64, Query, description = "The task ID to delete"),
        ),
        responses(
            (
                status = 200,
                description = "The deleted download task",
                body = ApiDownloadTask
            )
        )
    )
)]
#[route("/delete-download", method = "POST")]
pub async fn delete_download_endpoint(
    query: web::Query<DeleteDownloadTaskQuery>,
    db: LibraryDatabase,
) -> Result<Json<ApiDownloadTask>> {
    Ok(Json(
        delete_download_task(&db, query.task_id)
            .await
            .map_err(ErrorInternalServerError)?
            .ok_or_else(|| ErrorNotFound(format!("Download task not found: {}", query.task_id)))?
            .into(),
    ))
}

/// Query parameters for the get download tasks endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadTasks {
    /// Filter by task state (comma-separated)
    state: Option<String>,
    /// Page offset
    offset: Option<u32>,
    /// Page limit
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
            ("state" = Option<String>, Query, description = "The download task state to filter by"),
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
    let states = query
        .state
        .as_ref()
        .map(|x| {
            x.split(',')
                .map(ApiDownloadTaskState::from_str)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(ErrorBadRequest)?
        .map(|x| {
            x.into_iter()
                .map(DownloadTaskState::from)
                .collect::<Vec<_>>()
        });

    let tasks = get_download_tasks(&db)
        .await
        .map_err(ErrorInternalServerError)?
        .into_iter()
        .filter(|task| states.as_ref().is_none_or(|x| x.contains(&task.state)));

    let (mut current, mut history): (Vec<_>, Vec<_>) = tasks.partition(|task| match task.state {
        DownloadTaskState::Pending | DownloadTaskState::Paused | DownloadTaskState::Started => true,
        DownloadTaskState::Cancelled | DownloadTaskState::Finished | DownloadTaskState::Error => {
            false
        }
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
            let queue = DOWNLOAD_QUEUE.read().await;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            task.speed.replace(queue.speed().unwrap_or(0.0) as u64);
            if let Some(current_task) = queue.current_task().await {
                let file = switchy_fs::unsync::OpenOptions::new()
                    .read(true)
                    .open(current_task.file_path)
                    .await
                    .map_err(ErrorInternalServerError)?;
                let bytes = file
                    .metadata()
                    .await
                    .map_err(ErrorInternalServerError)?
                    .len();
                task.bytes = bytes;

                #[allow(clippy::cast_precision_loss)]
                if let Some(total_bytes) = task.total_bytes {
                    task.progress = 100.0_f64.min((bytes as f64 / total_bytes as f64) * 100.0);
                }
            }
        }
    }

    Ok(Json(Page::WithTotal {
        offset,
        items: tasks,
        limit,
        total,
    }))
}

/// Query parameters for the get download locations endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadLocations {
    /// Page offset
    offset: Option<u32>,
    /// Page limit
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

/// Query parameters for the add download location endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDownloadLocation {
    /// Filesystem path for the location
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
            .ok_or_else(|| ErrorBadRequest(format!("Invalid path: {}", path.display())))?,
    )
    .await
    .map_err(ErrorInternalServerError)?;

    Ok(Json(location.into()))
}

/// Query parameters for the delete download location endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDownloadLocation {
    /// Filesystem path to delete
    path: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        delete,
        path = "/download-locations",
        description = "Delete a download location",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("path" = String, Query, description = "The download location path"),
        ),
        responses(
            (
                status = 200,
                description = "The successfully deleted download location",
                body = Value,
            )
        )
    )
)]
#[route("/download-locations", method = "DELETE")]
#[allow(clippy::future_not_send)]
pub async fn remove_download_location_endpoint(
    query: web::Query<DeleteDownloadLocation>,
    db: LibraryDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Option<ApiDownloadLocation>>> {
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

    let location = delete_download_location(
        &db,
        path.to_str()
            .ok_or_else(|| ErrorBadRequest(format!("Invalid path: {}", path.display())))?,
    )
    .await
    .map_err(ErrorInternalServerError)?;

    Ok(Json(location.map(Into::into)))
}
