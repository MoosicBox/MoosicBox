use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{query::*, Database};
use moosicbox_json_utils::ToValueType as _;

pub mod models;

use self::models::{CreateDownloadTask, DownloadItem, DownloadLocation, DownloadTask};

pub async fn create_download_location(db: &Box<dyn Database>, path: &str) -> Result<(), DbError> {
    db.upsert("download_locations")
        .filter(where_eq("path", path))
        .value("path", path)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_download_location(
    db: &Box<dyn Database>,
    id: u64,
) -> Result<Option<DownloadLocation>, DbError> {
    Ok(db
        .select("download_locations")
        .filter(where_eq("id", id))
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_download_locations(
    db: &Box<dyn Database>,
) -> Result<Vec<DownloadLocation>, DbError> {
    Ok(db
        .select("download_locations")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn create_download_task(
    db: &Box<dyn Database>,
    task: &CreateDownloadTask,
) -> Result<DownloadTask, DbError> {
    let track_id = if let DownloadItem::Track { track_id, .. } = task.item {
        Some(track_id)
    } else {
        None
    };
    let source = if let DownloadItem::Track { source, .. } = task.item {
        Some(source.as_ref().to_string())
    } else {
        None
    };
    let quality = if let DownloadItem::Track { quality, .. } = task.item {
        Some(quality.as_ref().to_string())
    } else {
        None
    };
    let album_id = if let DownloadItem::AlbumCover(album_id) = task.item {
        Some(album_id)
    } else if let DownloadItem::ArtistCover(album_id) = task.item {
        Some(album_id)
    } else {
        None
    };

    Ok(db
        .upsert("download_tasks")
        .filter(where_eq("file_path", task.file_path.clone()))
        .filter(where_eq("type", task.item.as_ref()))
        .filter_some(track_id.map(|x| where_eq("track_id", x)))
        .filter_some(source.clone().map(|x| where_eq("source", x)))
        .filter_some(quality.clone().map(|x| where_eq("quality", x)))
        .filter_some(album_id.map(|x| where_eq("album_id", x)))
        .value("file_path", task.file_path.clone())
        .value("type", task.item.as_ref())
        .value("track_id", track_id)
        .value("source", source)
        .value("quality", quality)
        .value("album_id", album_id)
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn get_download_tasks(db: &Box<dyn Database>) -> Result<Vec<DownloadTask>, DbError> {
    Ok(db
        .select("download_tasks")
        .sort("id", SortDirection::Desc)
        .execute(db)
        .await?
        .to_value_type()?)
}
