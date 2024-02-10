use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use async_recursion::async_recursion;
use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_core::sqlite::{
    db::{get_album_database, DbError},
    models::{
        qobuz::{QobuzAlbum, QobuzImageSize},
        tidal::{TidalAlbum, TidalAlbumImageSize},
        AlbumId,
    },
};
use moosicbox_database::{query::*, Database, DatabaseError, DatabaseValue};
use moosicbox_qobuz::QobuzAlbumError;
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use moosicbox_tidal::TidalAlbumError;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    get_or_fetch_cover_bytes_from_remote_url, get_or_fetch_cover_from_remote_url,
    sanitize_filename, search_for_cover, CoverBytes, FetchCoverError,
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
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("No Album Cover")]
    NoAlbumCover,
}

async fn fetch_local_album_cover(
    db: Arc<Box<dyn Database>>,
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

        db.update("albums")
            .filter(where_eq("id", album_id))
            .value("artwork", artwork)
            .execute(&db)
            .await?;

        return Ok(path.to_str().unwrap().to_string());
    }

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

async fn fetch_local_album_cover_bytes(
    db: Arc<Box<dyn Database>>,
    cover: Option<String>,
    album_id: i32,
    directory: Option<String>,
) -> Result<CoverBytes, FetchLocalAlbumCoverError> {
    let cover = cover.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::is_file(&cover_path) {
        let file = tokio::fs::File::open(cover_path.to_path_buf()).await?;

        let size = if let Ok(metadata) = file.metadata().await {
            Some(metadata.len())
        } else {
            None
        };

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None)? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!("Updating Album {album_id} artwork file from '{cover}' to '{artwork}'");

        db.update("albums")
            .filter(where_eq("id", album_id))
            .value("artwork", artwork)
            .execute(&db)
            .await?;

        let file = tokio::fs::File::open(path).await?;

        let size = if let Ok(metadata) = file.metadata().await {
            Some(metadata.len())
        } else {
            None
        };

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
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
    Database(#[from] DatabaseError),
    #[error(transparent)]
    TidalAlbum(#[from] moosicbox_tidal::TidalAlbumError),
    #[error(transparent)]
    QobuzAlbum(#[from] moosicbox_qobuz::QobuzAlbumError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

async fn copy_streaming_cover_to_local(
    db: Arc<Box<dyn Database>>,
    album_id: i32,
    cover: String,
) -> Result<String, AlbumCoverError> {
    log::debug!("Updating Album {album_id} cover file to '{cover}'");

    db.update("albums")
        .filter(where_eq("id", album_id))
        .value("artwork", cover.clone())
        .execute(&db)
        .await?;

    Ok(cover)
}

#[async_recursion]
pub async fn get_album_cover_bytes(
    album_id: AlbumId,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    Ok(match &album_id {
        AlbumId::Library(library_album_id) => {
            get_library_album_cover_bytes(*library_album_id, db, try_to_get_stream_size).await?
        }
        AlbumId::Tidal(tidal_album_id) => {
            get_tidal_album_cover_bytes(*tidal_album_id, db, size, try_to_get_stream_size).await?
        }
        AlbumId::Qobuz(qobuz_album_id) => {
            get_qobuz_album_cover_bytes(qobuz_album_id, db, size, try_to_get_stream_size).await?
        }
    })
}

#[async_recursion]
pub async fn get_album_cover(
    album_id: AlbumId,
    db: Arc<Box<dyn Database>>,
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
    db: Arc<Box<dyn Database>>,
) -> Result<String, AlbumCoverError> {
    let album = get_album_database(&db, "id", DatabaseValue::UNumber(library_album_id as u64))
        .await?
        .ok_or(AlbumCoverError::NotFound(AlbumId::Library(
            library_album_id,
        )))?;

    if let Ok(cover) =
        fetch_local_album_cover(db.clone(), album.artwork, album.id, album.directory).await
    {
        return Ok(cover);
    }

    if let Some(tidal_id) = album.tidal_id {
        if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
            get_album_cover(AlbumId::Tidal(tidal_id), db.clone(), None).await
        {
            return Ok(copy_streaming_cover_to_local(db.clone(), album.id, cover).await?);
        }
    }

    if let Some(qobuz_id) = album.qobuz_id {
        if let Ok(AlbumCoverSource::LocalFilePath(cover)) =
            get_album_cover(AlbumId::Qobuz(qobuz_id), db.clone(), None).await
        {
            return Ok(copy_streaming_cover_to_local(db, album.id, cover).await?);
        }
    }

    return Err(AlbumCoverError::NotFound(AlbumId::Library(
        library_album_id,
    )));
}

pub async fn get_library_album_cover_bytes(
    library_album_id: i32,
    db: Arc<Box<dyn Database>>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let album = get_album_database(&db, "id", DatabaseValue::UNumber(library_album_id as u64))
        .await?
        .ok_or(AlbumCoverError::NotFound(AlbumId::Library(
            library_album_id,
        )))?;

    if let Ok(bytes) =
        fetch_local_album_cover_bytes(db.clone(), album.artwork, album.id, album.directory).await
    {
        return Ok(bytes);
    }

    if let Some(tidal_id) = album.tidal_id {
        if let Ok(bytes) = get_album_cover_bytes(
            AlbumId::Tidal(tidal_id),
            db.clone(),
            None,
            try_to_get_stream_size,
        )
        .await
        {
            return Ok(bytes);
        }
    }

    if let Some(qobuz_id) = album.qobuz_id {
        if let Ok(bytes) =
            get_album_cover_bytes(AlbumId::Qobuz(qobuz_id), db, None, try_to_get_stream_size).await
        {
            return Ok(bytes);
        }
    }

    return Err(AlbumCoverError::NotFound(AlbumId::Library(
        library_album_id,
    )));
}

async fn get_tidal_album_cover_request(
    tidal_album_id: u64,
    db: Arc<Box<dyn Database>>,
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
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<String, AlbumCoverError> {
    let request = get_tidal_album_cover_request(tidal_album_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_tidal_album_cover_bytes(
    tidal_album_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let request = get_tidal_album_cover_request(tidal_album_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

struct AlbumCoverRequest {
    url: String,
    file_path: PathBuf,
}

async fn get_qobuz_album_cover_request(
    qobuz_album_id: &str,
    db: Arc<Box<dyn Database>>,
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
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<String, AlbumCoverError> {
    let request = get_qobuz_album_cover_request(qobuz_album_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_qobuz_album_cover_bytes(
    qobuz_album_id: &str,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let request = get_qobuz_album_cover_request(qobuz_album_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}
