use std::sync::LazyLock;

use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{query::*, Database};
use moosicbox_json_utils::ToValueType;
use regex::{Captures, Regex};

pub mod models;

use self::models::{CreateDownloadTask, DownloadLocation, DownloadTask};

pub async fn create_download_location(
    db: &dyn Database,
    path: &str,
) -> Result<DownloadLocation, DbError> {
    static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

    let path = if std::env::consts::OS == "windows" {
        REGEX
            .replace(path, |caps: &Captures| {
                format!("{}:", caps[1].to_uppercase())
            })
            .replace('/', "\\")
    } else {
        path.to_owned()
    };

    Ok(db
        .upsert("download_locations")
        .where_eq("path", &path)
        .value("path", &path)
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn get_download_location(
    db: &dyn Database,
    id: u64,
) -> Result<Option<DownloadLocation>, DbError> {
    Ok(db
        .select("download_locations")
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_download_locations(db: &dyn Database) -> Result<Vec<DownloadLocation>, DbError> {
    Ok(db
        .select("download_locations")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn create_download_task(
    db: &dyn Database,
    task: &CreateDownloadTask,
) -> Result<DownloadTask, DbError> {
    let source = task.item.source().as_ref();
    let quality = task.item.quality().map(|x| x.as_ref());
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
        .where_eq("source", source)
        .filter_if_some(track_id.map(|x| where_eq("track_id", x)))
        .filter_if_some(quality.map(|x| where_eq("quality", x)))
        .filter_if_some(album_id.map(|x| where_eq("album_id", x)))
        .filter_if_some(artist_id.map(|x| where_eq("artist_id", x)))
        .value("file_path", task.file_path.clone())
        .value("type", task.item.as_ref())
        .value("track", track)
        .value("source", source)
        .value("quality", quality)
        .value("track_id", track_id)
        .value("album", album)
        .value("album_id", album_id)
        .value("artist", artist)
        .value("artist_id", artist_id)
        .value("contains_cover", contains_cover)
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn get_download_tasks(db: &dyn Database) -> Result<Vec<DownloadTask>, DbError> {
    Ok(db
        .select("download_tasks")
        .sort("id", SortDirection::Desc)
        .execute(db)
        .await?
        .to_value_type()?)
}
