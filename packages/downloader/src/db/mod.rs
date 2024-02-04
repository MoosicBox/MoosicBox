use moosicbox_core::sqlite::db::{select, upsert, DbError, SqliteValue};
use rusqlite::Connection;

pub mod models;

use self::models::{DownloadLocation, DownloadTask};

pub fn create_download_location(db: &Connection, path: &str) -> Result<(), DbError> {
    upsert::<DownloadLocation>(
        db,
        "download_locations",
        vec![("path", SqliteValue::String(path.to_string()))],
        vec![("path", SqliteValue::String(path.to_string()))],
    )?;

    Ok(())
}

pub fn get_download_location(
    db: &Connection,
    id: u64,
) -> Result<Option<DownloadLocation>, DbError> {
    Ok(select::<DownloadLocation>(
        db,
        "download_locations",
        &vec![("id", SqliteValue::Number(id as i64))],
        &["*"],
    )?
    .into_iter()
    .next())
}

pub fn get_download_locations(db: &Connection) -> Result<Vec<DownloadLocation>, DbError> {
    let download_locations = select::<DownloadLocation>(db, "download_locations", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(download_locations)
}

pub fn create_download_task(db: &Connection, path: &str) -> Result<(), DbError> {
    upsert::<DownloadTask>(
        db,
        "download_tasks",
        vec![("path", SqliteValue::String(path.to_string()))],
        vec![("path", SqliteValue::String(path.to_string()))],
    )?;

    Ok(())
}

pub fn get_download_tasks(db: &Connection) -> Result<Vec<DownloadTask>, DbError> {
    let download_tasks = select::<DownloadTask>(db, "download_tasks", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(download_tasks)
}
