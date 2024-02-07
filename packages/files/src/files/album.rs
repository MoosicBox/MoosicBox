use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::RwLock,
};

use async_recursion::async_recursion;
use bytes::BytesMut;
use futures::{StreamExt, TryFutureExt, TryStreamExt};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album, DbError, SqliteValue},
        models::{
            qobuz::{QobuzAlbum, QobuzImageSize},
            tidal::{TidalAlbum, TidalAlbumImageSize},
            AlbumId, LibraryAlbum,
        },
    },
};
use moosicbox_qobuz::QobuzAlbumError;
use moosicbox_tidal::TidalAlbumError;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    get_or_fetch_cover_bytes_from_remote_url, get_or_fetch_cover_from_remote_url,
    sanitize_filename, search_for_cover, BytesStream, FetchCoverError,
};

pub enum AlbumCoverSource {
    LocalFilePath(String),
}

fn get_album_cover_path(
    size: &str,
    source: &str,
    album_id: &str,
    artist_name: &str,
    album_name: &str,
) -> PathBuf {
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name))
        .join(sanitize_filename(album_name));

    let filename = format!("album_{size}_{album_id}.jpg");

    path.join(filename)
}

#[derive(Debug, Error)]
pub enum FetchLocalAlbumCoverError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
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

    if Path::is_file(&cover_path) {
        return Ok(cover_path.to_str().unwrap().to_string());
    }

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None)? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!("Updating Album {album_id} artwork file from '{cover}' to '{artwork}'");

        moosicbox_core::sqlite::db::update_and_get_row::<LibraryAlbum>(
            &db.library.lock().as_ref().unwrap().inner,
            "albums",
            SqliteValue::Number(album_id as i64),
            &[("artwork", SqliteValue::String(artwork))],
        )?;

        return Ok(path.to_str().unwrap().to_string());
    }

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

fn fetch_local_album_cover_bytes(
    db: &Db,
    cover: Option<String>,
    album_id: i32,
    directory: Option<String>,
) -> Result<BytesStream, FetchLocalAlbumCoverError> {
    let cover = cover.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::is_file(&cover_path) {
        return Ok(tokio::fs::File::open(cover_path.to_path_buf())
            .map_ok(|file| FramedRead::new(file, BytesCodec::new()).map_ok(BytesMut::freeze))
            .try_flatten_stream()
            .boxed());
    }

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None)? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!("Updating Album {album_id} artwork file from '{cover}' to '{artwork}'");

        moosicbox_core::sqlite::db::update_and_get_row::<LibraryAlbum>(
            &db.library.lock().as_ref().unwrap().inner,
            "albums",
            SqliteValue::Number(album_id as i64),
            &[("artwork", SqliteValue::String(artwork))],
        )?;

        return Ok(tokio::fs::File::open(path)
            .map_ok(|file| FramedRead::new(file, BytesCodec::new()).map_ok(BytesMut::freeze))
            .try_flatten_stream()
            .boxed());
    }

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

#[derive(Debug, Error)]
pub enum AlbumCoverError {
    #[error("Album cover not found for album: {0:?}")]
    NotFound(AlbumId),
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    #[error(transparent)]
    FetchLocalAlbumCover(#[from] FetchLocalAlbumCoverError),
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
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

    moosicbox_core::sqlite::db::update_and_get_row::<LibraryAlbum>(
        &db.library.lock().as_ref().unwrap().inner,
        "albums",
        SqliteValue::Number(album_id as i64),
        &[("artwork", SqliteValue::String(cover.to_string()))],
    )?;

    Ok(cover)
}

#[async_recursion]
pub async fn get_album_cover_bytes(
    album_id: AlbumId,
    db: &Db,
    size: Option<u32>,
) -> Result<BytesStream, AlbumCoverError> {
    Ok(match &album_id {
        AlbumId::Library(library_album_id) => {
            get_library_album_cover_bytes(*library_album_id, db).await?
        }
        AlbumId::Tidal(tidal_album_id) => {
            get_tidal_album_cover_bytes(*tidal_album_id, db, size).await?
        }
        AlbumId::Qobuz(qobuz_album_id) => {
            get_qobuz_album_cover_bytes(qobuz_album_id, db, size).await?
        }
    })
}

#[async_recursion]
pub async fn get_album_cover(
    album_id: AlbumId,
    db: &Db,
    size: Option<u32>,
) -> Result<AlbumCoverSource, AlbumCoverError> {
    let path = match &album_id {
        AlbumId::Library(library_album_id) => {
            get_library_album_cover(*library_album_id, db).await?
        }
        AlbumId::Tidal(tidal_album_id) => get_tidal_album_cover(*tidal_album_id, db, size).await?,
        AlbumId::Qobuz(qobuz_album_id) => get_qobuz_album_cover(qobuz_album_id, db, size).await?,
    };

    Ok(AlbumCoverSource::LocalFilePath(path))
}

pub async fn get_library_album_cover(
    library_album_id: i32,
    db: &Db,
) -> Result<String, AlbumCoverError> {
    let album = get_album(&db.library.lock().as_ref().unwrap().inner, library_album_id)?.ok_or(
        AlbumCoverError::NotFound(AlbumId::Library(library_album_id)),
    )?;

    if let Ok(cover) = fetch_local_album_cover(db, album.artwork, album.id, album.directory) {
        return Ok(cover);
    }

    if let Some(tidal_id) = album.tidal_id {
        if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
            get_album_cover(AlbumId::Tidal(tidal_id), db, None).await
        {
            return Ok(copy_streaming_cover_to_local(db, album.id, cover)?);
        }
    }

    if let Some(qobuz_id) = album.qobuz_id {
        if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
            get_album_cover(AlbumId::Qobuz(qobuz_id), db, None).await
        {
            return Ok(copy_streaming_cover_to_local(db, album.id, cover)?);
        }
    }

    return Err(AlbumCoverError::NotFound(AlbumId::Library(
        library_album_id,
    )));
}

pub async fn get_library_album_cover_bytes(
    library_album_id: i32,
    db: &Db,
) -> Result<BytesStream, AlbumCoverError> {
    let album = get_album(&db.library.lock().as_ref().unwrap().inner, library_album_id)?.ok_or(
        AlbumCoverError::NotFound(AlbumId::Library(library_album_id)),
    )?;

    if let Ok(bytes) = fetch_local_album_cover_bytes(db, album.artwork, album.id, album.directory) {
        return Ok(bytes);
    }

    if let Some(tidal_id) = album.tidal_id {
        if let Ok(bytes) = get_album_cover_bytes(AlbumId::Tidal(tidal_id), db, None).await {
            return Ok(bytes);
        }
    }

    if let Some(qobuz_id) = album.qobuz_id {
        if let Ok(bytes) = get_album_cover_bytes(AlbumId::Qobuz(qobuz_id), db, None).await {
            return Ok(bytes);
        }
    }

    return Err(AlbumCoverError::NotFound(AlbumId::Library(
        library_album_id,
    )));
}

async fn get_tidal_album_cover_request(
    tidal_album_id: u64,
    db: &Db,
    size: Option<u32>,
) -> Result<AlbumCoverRequest, AlbumCoverError> {
    static ALBUM_CACHE: Lazy<RwLock<HashMap<u64, Option<TidalAlbum>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    let album = if let Some(album) = {
        let binding = ALBUM_CACHE.read().unwrap();
        binding.get(&tidal_album_id).cloned()
    } {
        album
    } else {
        use moosicbox_tidal::AuthenticatedRequestError;

        let album = match moosicbox_tidal::album(db, &tidal_album_id.into(), None, None, None, None)
            .await
        {
            Ok(album) => Ok(Some(album)),
            Err(err) => match err {
                TidalAlbumError::AuthenticatedRequest(
                    AuthenticatedRequestError::RequestFailed(404, _),
                ) => Ok(None),
                _ => Err(err),
            },
        }?;

        ALBUM_CACHE
            .write()
            .as_mut()
            .unwrap()
            .insert(tidal_album_id, album.clone());

        album
    }
    .ok_or_else(|| AlbumCoverError::NotFound(AlbumId::Tidal(tidal_album_id)))?;

    let size = size
        .map(|size| (size as u16).into())
        .unwrap_or(TidalAlbumImageSize::Max);

    let url = album
        .cover_url(size)
        .ok_or(AlbumCoverError::NotFound(AlbumId::Tidal(tidal_album_id)))?;

    let file_path = get_album_cover_path(
        &size.to_string(),
        "tidal",
        &album.id.to_string(),
        &album.artist,
        &album.title,
    );

    Ok(AlbumCoverRequest { url, file_path })
}

async fn get_tidal_album_cover(
    tidal_album_id: u64,
    db: &Db,
    size: Option<u32>,
) -> Result<String, AlbumCoverError> {
    let request = get_tidal_album_cover_request(tidal_album_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_tidal_album_cover_bytes(
    tidal_album_id: u64,
    db: &Db,
    size: Option<u32>,
) -> Result<BytesStream, AlbumCoverError> {
    let request = get_tidal_album_cover_request(tidal_album_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(&request.url, &request.file_path).await?)
}

struct AlbumCoverRequest {
    url: String,
    file_path: PathBuf,
}

async fn get_qobuz_album_cover_request(
    qobuz_album_id: &str,
    db: &Db,
    size: Option<u32>,
) -> Result<AlbumCoverRequest, AlbumCoverError> {
    static ALBUM_CACHE: Lazy<RwLock<HashMap<String, Option<QobuzAlbum>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    let album = if let Some(album) = {
        let binding = ALBUM_CACHE.read().unwrap();
        binding.get(qobuz_album_id).cloned()
    } {
        album
    } else {
        use moosicbox_qobuz::AuthenticatedRequestError;
        let album = match moosicbox_qobuz::album(db, &qobuz_album_id.into(), None, None).await {
            Ok(album) => Ok(Some(album)),
            Err(err) => match err {
                QobuzAlbumError::AuthenticatedRequest(
                    AuthenticatedRequestError::RequestFailed(404, _),
                ) => Ok(None),
                _ => Err(err),
            },
        }?;

        ALBUM_CACHE
            .write()
            .as_mut()
            .unwrap()
            .insert(qobuz_album_id.to_string(), album.clone());

        album
    }
    .ok_or_else(|| AlbumCoverError::NotFound(AlbumId::Qobuz(qobuz_album_id.to_string())))?;

    let size = size
        .map(|size| (size as u16).into())
        .unwrap_or(QobuzImageSize::Mega);

    let url = album
        .image
        .as_ref()
        .and_then(|image| image.cover_url_for_size(size))
        .ok_or(AlbumCoverError::NotFound(AlbumId::Qobuz(
            qobuz_album_id.to_string(),
        )))?;

    let file_path = get_album_cover_path(
        &size.to_string(),
        "qobuz",
        &album.id,
        &album.artist,
        &album.title,
    );

    Ok(AlbumCoverRequest { url, file_path })
}

async fn get_qobuz_album_cover(
    qobuz_album_id: &str,
    db: &Db,
    size: Option<u32>,
) -> Result<String, AlbumCoverError> {
    let request = get_qobuz_album_cover_request(qobuz_album_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_qobuz_album_cover_bytes(
    qobuz_album_id: &str,
    db: &Db,
    size: Option<u32>,
) -> Result<BytesStream, AlbumCoverError> {
    let request = get_qobuz_album_cover_request(qobuz_album_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(&request.url, &request.file_path).await?)
}
