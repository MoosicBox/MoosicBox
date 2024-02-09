use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{sort, where_eq, Database, DatabaseValue, SortDirection};
use moosicbox_json_utils::ToValueType as _;

pub mod models;

use self::models::{CreateDownloadTask, DownloadItem, DownloadLocation, DownloadTask};

pub async fn create_download_location(db: &Box<dyn Database>, path: &str) -> Result<(), DbError> {
    db.upsert(
        "download_locations",
        &[("path", DatabaseValue::String(path.to_string()))],
        Some(&[where_eq("path", DatabaseValue::String(path.to_string()))]),
    )
    .await?;

    Ok(())
}

pub async fn get_download_location(
    db: &Box<dyn Database>,
    id: u64,
) -> Result<Option<DownloadLocation>, DbError> {
    Ok(db
        .select_first(
            "download_locations",
            &["*"],
            Some(&[where_eq("id", DatabaseValue::Number(id as i64))]),
            None,
            None,
        )
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_download_locations(
    db: &Box<dyn Database>,
) -> Result<Vec<DownloadLocation>, DbError> {
    Ok(db
        .select("download_locations", &["*"], None, None, None)
        .await?
        .to_value_type()?)
}

pub async fn create_download_task(
    db: &Box<dyn Database>,
    task: &CreateDownloadTask,
) -> Result<DownloadTask, DbError> {
    let values = vec![
        ("file_path", DatabaseValue::String(task.file_path.clone())),
        (
            "type",
            DatabaseValue::String(task.item.as_ref().to_string()),
        ),
        (
            "track_id",
            DatabaseValue::NumberOpt(if let DownloadItem::Track { track_id, .. } = task.item {
                Some(track_id as i64)
            } else {
                None
            }),
        ),
        (
            "source",
            DatabaseValue::StringOpt(if let DownloadItem::Track { source, .. } = task.item {
                Some(source.as_ref().to_string())
            } else {
                None
            }),
        ),
        (
            "quality",
            DatabaseValue::StringOpt(if let DownloadItem::Track { quality, .. } = task.item {
                Some(quality.as_ref().to_string())
            } else {
                None
            }),
        ),
        (
            "album_id",
            DatabaseValue::NumberOpt(if let DownloadItem::AlbumCover(album_id) = task.item {
                Some(album_id as i64)
            } else if let DownloadItem::ArtistCover(album_id) = task.item {
                Some(album_id as i64)
            } else {
                None
            }),
        ),
    ];

    Ok(db
        .upsert(
            "download_tasks",
            values.clone().as_slice(),
            Some(
                values
                    .into_iter()
                    .map(|(key, value)| where_eq(key, value))
                    .collect::<Vec<_>>()
                    .as_slice(),
            ),
        )
        .await?
        .to_value_type()?)
}

pub async fn get_download_tasks(db: &Box<dyn Database>) -> Result<Vec<DownloadTask>, DbError> {
    Ok(db
        .select(
            "download_tasks",
            &["*"],
            None,
            None,
            Some(&[sort("id", SortDirection::Desc)]),
        )
        .await?
        .to_value_type()?)
}
