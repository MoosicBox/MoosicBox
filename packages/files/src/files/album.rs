use std::{collections::HashMap, path::Path, sync::RwLock};

use async_recursion::async_recursion;
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album, DbError, SqliteValue},
        models::{Album, AlbumId},
    },
};
use moosicbox_qobuz::QobuzAlbum;
use moosicbox_tidal::TidalAlbum;
use once_cell::sync::Lazy;
use thiserror::Error;

use crate::{
    fetch_and_save_bytes_from_remote_url, sanitize_filename, search_for_cover,
    FetchAndSaveBytesFromRemoteUrlError,
};

pub enum AlbumCoverSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum FetchAlbumCoverError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

async fn get_or_fetch_album_cover_from_remote_url(
    url: &str,
    source: &str,
    album_id: &str,
    artist_name: &str,
    album_name: &str,
) -> Result<String, FetchAlbumCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name))
        .join(sanitize_filename(album_name));

    let filename = format!("album_{album_id}.jpg");
    let file_path = path.join(filename);

    if Path::exists(&file_path) {
        Ok(file_path.to_str().unwrap().to_string())
    } else {
        Ok(
            fetch_and_save_bytes_from_remote_url(&IMAGE_CLIENT, &file_path, url)
                .await?
                .to_str()
                .unwrap()
                .to_string(),
        )
    }
}

#[derive(Debug, Error)]
pub enum FetchLocalAlbumCoverError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No Album Cover")]
    NoAlbumCover,
}

fn fetch_local_album_cover(
    db: &Db,
    cover: Option<String>,
    album_id: i32,
    directory: Option<String>,
) -> Result<String, FetchLocalAlbumCoverError> {
    let cover = cover.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::exists(&cover_path) {
        return Ok(cover_path.to_str().unwrap().to_string());
    }

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None)? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!("Updating Album {album_id} artwork file from '{cover}' to '{artwork}'");

        moosicbox_core::sqlite::db::update_and_get_row::<Album>(
            &db.library.lock().as_ref().unwrap().inner,
            "albums",
            SqliteValue::Number(album_id as i64),
            &[("artwork", SqliteValue::String(artwork))],
        )?;

        return Ok(path.to_str().unwrap().to_string());
    }

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

#[derive(Debug, Error)]
pub enum AlbumCoverError {
    #[error("Album cover not found for album: {0:?}")]
    NotFound(AlbumId),
    #[error(transparent)]
    FetchAlbumCover(#[from] FetchAlbumCoverError),
    #[error(transparent)]
    FetchLocalAlbumCover(#[from] FetchLocalAlbumCoverError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TidalAlbum(#[from] moosicbox_tidal::TidalAlbumError),
    #[error(transparent)]
    QobuzAlbum(#[from] moosicbox_qobuz::QobuzAlbumError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

fn copy_streaming_cover_to_local(
    db: &Db,
    album_id: i32,
    cover: String,
) -> Result<String, AlbumCoverError> {
    log::debug!("Updating Album {album_id} cover file to '{cover}'");

    moosicbox_core::sqlite::db::update_and_get_row::<Album>(
        &db.library.lock().as_ref().unwrap().inner,
        "albums",
        SqliteValue::Number(album_id as i64),
        &[("artwork", SqliteValue::String(cover.to_string()))],
    )?;

    Ok(cover)
}

#[async_recursion]
pub async fn get_album_cover(
    album_id: AlbumId,
    db: &Db,
) -> Result<AlbumCoverSource, AlbumCoverError> {
    let path = match &album_id {
        AlbumId::Library(library_album_id) => {
            let album = get_album(
                &db.library.lock().as_ref().unwrap().inner,
                *library_album_id,
            )?
            .ok_or(AlbumCoverError::NotFound(album_id.clone()))?;

            if let Ok(cover) = fetch_local_album_cover(db, album.artwork, album.id, album.directory)
            {
                return Ok(AlbumCoverSource::LocalFilePath(cover));
            }

            if let Some(tidal_id) = album.tidal_id {
                if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
                    get_album_cover(AlbumId::Tidal(tidal_id), db).await
                {
                    return Ok(AlbumCoverSource::LocalFilePath(
                        copy_streaming_cover_to_local(db, album.id, cover)?,
                    ));
                }
            }

            if let Some(qobuz_id) = album.qobuz_id {
                if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
                    get_album_cover(AlbumId::Qobuz(qobuz_id), db).await
                {
                    return Ok(AlbumCoverSource::LocalFilePath(
                        copy_streaming_cover_to_local(db, album.id, cover)?,
                    ));
                }
            }

            return Err(AlbumCoverError::NotFound(album_id));
        }
        AlbumId::Tidal(tidal_album_id) => {
            static ALBUM_CACHE: Lazy<RwLock<HashMap<u64, TidalAlbum>>> =
                Lazy::new(|| RwLock::new(HashMap::new()));

            let album = if let Some(album) = {
                let binding = ALBUM_CACHE.read().unwrap();
                binding.get(tidal_album_id).cloned()
            } {
                album
            } else {
                let album =
                    moosicbox_tidal::album(db, *tidal_album_id, None, None, None, None).await?;
                ALBUM_CACHE
                    .write()
                    .as_mut()
                    .unwrap()
                    .insert(*tidal_album_id, album.clone());
                album
            };

            get_or_fetch_album_cover_from_remote_url(
                &album.cover_url(1280),
                "tidal",
                &album.id.to_string(),
                &album.artist,
                &album.title,
            )
            .await?
        }
        AlbumId::Qobuz(qobuz_album_id) => {
            static ALBUM_CACHE: Lazy<RwLock<HashMap<String, QobuzAlbum>>> =
                Lazy::new(|| RwLock::new(HashMap::new()));

            let album = if let Some(album) = {
                let binding = ALBUM_CACHE.read().unwrap();
                binding.get(qobuz_album_id).cloned()
            } {
                album
            } else {
                let album = moosicbox_qobuz::album(db, qobuz_album_id, None, None).await?;
                ALBUM_CACHE
                    .write()
                    .as_mut()
                    .unwrap()
                    .insert(qobuz_album_id.to_string(), album.clone());
                album
            };

            let cover = album
                .cover_url()
                .ok_or(AlbumCoverError::NotFound(album_id.clone()))?;

            get_or_fetch_album_cover_from_remote_url(
                &cover,
                "qobuz",
                &album.id,
                &album.artist,
                &album.title,
            )
            .await?
        }
    };

    Ok(AlbumCoverSource::LocalFilePath(path))
}
