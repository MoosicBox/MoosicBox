//! Database operations for download management.
//!
//! Provides functions for creating, retrieving, and managing download tasks and
//! download locations in the database.

use moosicbox_json_utils::{ToValueType, database::DatabaseFetchError};
use switchy_database::{
    profiles::LibraryDatabase,
    query::{FilterableQuery, where_eq},
};

pub mod models;

use self::models::{CreateDownloadTask, DownloadLocation, DownloadTask};

/// # Errors
///
/// * If there was a database error
pub async fn create_download_location(
    db: &LibraryDatabase,
    path: &str,
) -> Result<DownloadLocation, DatabaseFetchError> {
    Ok(db
        .upsert("download_locations")
        .where_eq("path", path)
        .value("path", path)
        .execute_first(&**db)
        .await?
        .to_value_type()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn delete_download_location(
    db: &LibraryDatabase,
    path: &str,
) -> Result<Option<DownloadLocation>, DatabaseFetchError> {
    Ok(db
        .delete("download_locations")
        .where_eq("path", path)
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn get_download_location(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<DownloadLocation>, DatabaseFetchError> {
    Ok(db
        .select("download_locations")
        .where_eq("id", id)
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn get_download_locations(
    db: &LibraryDatabase,
) -> Result<Vec<DownloadLocation>, DatabaseFetchError> {
    Ok(db
        .select("download_locations")
        .execute(&**db)
        .await?
        .to_value_type()?)
}

pub async fn create_download_task(
    db: &LibraryDatabase,
    task: &CreateDownloadTask,
) -> Result<DownloadTask, DatabaseFetchError> {
    let source = serde_json::to_string(task.item.source()).unwrap();
    let quality = task.item.quality().map(AsRef::as_ref);
    let track_id = task.item.track_id();
    let track = task.item.track();
    let album_id = task.item.album_id();
    let album = task.item.album();
    let artist_id = task.item.artist_id();
    let artist = task.item.artist();
    let contains_cover = task.item.contains_cover();

    Ok(db
        .upsert("download_tasks")
        .where_eq("file_path", task.file_path.clone())
        .where_eq("type", task.item.as_ref())
        .where_eq("source", &source)
        .where_eq("album_id", album_id)
        .where_eq("artist_id", artist_id)
        .filter_if_some(track_id.map(|x| where_eq("track_id", x)))
        .filter_if_some(quality.map(|x| where_eq("quality", x)))
        .value("file_path", task.file_path.clone())
        .value("type", task.item.as_ref())
        .value("track", track)
        .value("source", &source)
        .value("quality", quality)
        .value("track_id", track_id)
        .value("album", album)
        .value("album_id", album_id)
        .value("artist", artist)
        .value("artist_id", artist_id)
        .value("contains_cover", contains_cover)
        .execute_first(&**db)
        .await?
        .to_value_type()?)
}

#[cfg(feature = "api")]
pub async fn get_download_tasks(
    db: &LibraryDatabase,
) -> Result<Vec<DownloadTask>, DatabaseFetchError> {
    Ok(db
        .select("download_tasks")
        .sort("id", switchy_database::query::SortDirection::Desc)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

#[cfg(feature = "api")]
pub async fn delete_download_task(
    db: &LibraryDatabase,
    task_id: u64,
) -> Result<Option<DownloadTask>, DatabaseFetchError> {
    Ok(db
        .delete("download_tasks")
        .where_eq("id", task_id)
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}
