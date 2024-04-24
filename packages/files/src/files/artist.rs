use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use async_recursion::async_recursion;
use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_core::sqlite::{
    db::{get_artist, DbError},
    models::{
        qobuz::{QobuzArtist, QobuzImageSize},
        tidal::{TidalArtist, TidalArtistImageSize},
        ArtistId,
    },
};
use moosicbox_database::{query::*, Database, DatabaseError};
use moosicbox_qobuz::QobuzArtistError;
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use moosicbox_tidal::TidalArtistError;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    get_or_fetch_cover_bytes_from_remote_url, get_or_fetch_cover_from_remote_url,
    sanitize_filename, CoverBytes, FetchCoverError,
};

pub enum ArtistCoverSource {
    LocalFilePath(String),
}

fn get_artist_cover_path(size: &str, source: &str, artist_id: u64, artist_name: &str) -> PathBuf {
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name));

    let filename = format!("artist_{artist_id}_{size}.jpg");

    path.join(filename)
}

#[derive(Debug, Error)]
pub enum FetchLocalArtistCoverError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No Artist Cover")]
    NoArtistCover,
}

fn fetch_local_artist_cover(cover: Option<String>) -> Result<String, FetchLocalArtistCoverError> {
    let cover = cover.ok_or(FetchLocalArtistCoverError::NoArtistCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    log::debug!("Checking if local path exists: {cover_path:?}");

    if Path::exists(&cover_path) {
        log::debug!("Path exists");

        return Ok(cover_path.to_str().unwrap().to_string());
    }

    log::debug!("Path does not exist");

    Err(FetchLocalArtistCoverError::NoArtistCover)
}

async fn fetch_local_artist_cover_bytes(
    cover: Option<String>,
) -> Result<CoverBytes, FetchLocalArtistCoverError> {
    let cover = cover.ok_or(FetchLocalArtistCoverError::NoArtistCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    log::debug!("Checking if local path exists: {cover_path:?}");

    if Path::exists(&cover_path) {
        log::debug!("Path exists");
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

    log::debug!("Path does not exist");

    Err(FetchLocalArtistCoverError::NoArtistCover)
}

#[derive(Debug, Error)]
pub enum ArtistCoverError {
    #[error("Artist cover not found for artist: {0:?}")]
    NotFound(ArtistId),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    #[error(transparent)]
    TidalArtist(#[from] moosicbox_tidal::TidalArtistError),
    #[error(transparent)]
    QobuzArtist(#[from] moosicbox_qobuz::QobuzArtistError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

async fn copy_streaming_cover_to_local(
    db: Arc<Box<dyn Database>>,
    artist_id: i32,
    cover: String,
) -> Result<String, ArtistCoverError> {
    log::debug!("Updating Artist {artist_id} cover file to '{cover}'");

    db.update("artists")
        .where_eq("id", artist_id)
        .value("cover", cover.clone())
        .execute(&**db)
        .await?;

    Ok(cover)
}

#[async_recursion]
pub async fn get_artist_cover_bytes(
    artist_id: ArtistId,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    Ok(match &artist_id {
        ArtistId::Library(library_artist_id) => {
            get_library_artist_cover_bytes(*library_artist_id, db, try_to_get_stream_size).await?
        }
        ArtistId::Tidal(tidal_artist_id) => {
            get_tidal_artist_cover_bytes(*tidal_artist_id, db, size, try_to_get_stream_size).await?
        }
        ArtistId::Qobuz(qobuz_artist_id) => {
            get_qobuz_artist_cover_bytes(*qobuz_artist_id, db, size, try_to_get_stream_size).await?
        }
    })
}

#[async_recursion]
pub async fn get_artist_cover(
    artist_id: ArtistId,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<ArtistCoverSource, ArtistCoverError> {
    let path = match &artist_id {
        ArtistId::Library(library_artist_id) => {
            get_library_artist_cover(*library_artist_id, db).await?
        }
        ArtistId::Tidal(tidal_artist_id) => {
            get_tidal_artist_cover(*tidal_artist_id, db, size).await?
        }
        ArtistId::Qobuz(qobuz_artist_id) => {
            get_qobuz_artist_cover(*qobuz_artist_id, db, size).await?
        }
    };

    Ok(ArtistCoverSource::LocalFilePath(path))
}

pub async fn get_library_artist_cover(
    library_artist_id: i32,
    db: Arc<Box<dyn Database>>,
) -> Result<String, ArtistCoverError> {
    let artist = get_artist(&**db, "id", library_artist_id as u64)
        .await?
        .ok_or(ArtistCoverError::NotFound(ArtistId::Library(
            library_artist_id,
        )))?;

    log::debug!("Looking for local artist cover");
    if let Ok(cover) = fetch_local_artist_cover(artist.cover) {
        log::debug!("Found local artist cover");
        return Ok(cover);
    }

    log::debug!("Looking for Tidal artist cover");
    if let Some(tidal_id) = artist.tidal_id {
        if let Ok(ArtistCoverSource::LocalFilePath(cover)) =
            get_artist_cover(ArtistId::Tidal(tidal_id), db.clone(), None).await
        {
            log::debug!("Found Tidal artist cover");
            return copy_streaming_cover_to_local(db.clone(), artist.id, cover).await;
        }
    }

    log::debug!("Looking for Qobuz artist cover");
    if let Some(qobuz_id) = artist.qobuz_id {
        if let Ok(ArtistCoverSource::LocalFilePath(cover)) =
            get_artist_cover(ArtistId::Qobuz(qobuz_id), db.clone(), None).await
        {
            log::debug!("Found Qobuz artist cover");
            return copy_streaming_cover_to_local(db, artist.id, cover).await;
        }
    }

    log::debug!("No artist covers found");
    Err(ArtistCoverError::NotFound(ArtistId::Library(
        library_artist_id,
    )))
}

pub async fn get_library_artist_cover_bytes(
    library_artist_id: i32,
    db: Arc<Box<dyn Database>>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    let artist = get_artist(&**db, "id", library_artist_id as u64)
        .await?
        .ok_or(ArtistCoverError::NotFound(ArtistId::Library(
            library_artist_id,
        )))?;

    if let Ok(bytes) = fetch_local_artist_cover_bytes(artist.cover).await {
        return Ok(bytes);
    }

    if let Some(tidal_id) = artist.tidal_id {
        if let Ok(bytes) = get_artist_cover_bytes(
            ArtistId::Tidal(tidal_id),
            db.clone(),
            None,
            try_to_get_stream_size,
        )
        .await
        {
            return Ok(bytes);
        }
    }

    if let Some(qobuz_id) = artist.qobuz_id {
        if let Ok(bytes) = get_artist_cover_bytes(
            ArtistId::Qobuz(qobuz_id),
            db.clone(),
            None,
            try_to_get_stream_size,
        )
        .await
        {
            return Ok(bytes);
        }
    }

    Err(ArtistCoverError::NotFound(ArtistId::Library(
        library_artist_id,
    )))
}

struct ArtistCoverRequest {
    url: String,
    file_path: PathBuf,
}

pub async fn get_tidal_artist_cover_bytes(
    tidal_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    let request = get_tidal_artist_cover_request(tidal_artist_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

pub async fn get_tidal_artist_cover(
    tidal_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<String, ArtistCoverError> {
    let request = get_tidal_artist_cover_request(tidal_artist_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_tidal_artist_cover_request(
    tidal_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<ArtistCoverRequest, ArtistCoverError> {
    static ARTIST_CACHE: Lazy<RwLock<HashMap<u64, Option<TidalArtist>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    let artist =
        if let Some(artist) = {
            let binding = ARTIST_CACHE.read().unwrap();
            binding.get(&tidal_artist_id).cloned()
        } {
            artist
        } else {
            use moosicbox_tidal::AuthenticatedRequestError;

            let artist =
                match moosicbox_tidal::artist(db, &tidal_artist_id.into(), None, None, None, None)
                    .await
                {
                    Ok(album) => Ok(Some(album)),
                    Err(err) => match err {
                        TidalArtistError::AuthenticatedRequest(
                            AuthenticatedRequestError::RequestFailed(404, _),
                        ) => Ok(None),
                        _ => Err(err),
                    },
                }?;

            ARTIST_CACHE
                .write()
                .as_mut()
                .unwrap()
                .insert(tidal_artist_id, artist.clone());

            artist
        }
        .ok_or_else(|| ArtistCoverError::NotFound(ArtistId::Tidal(tidal_artist_id)))?;

    let size = size
        .map(|size| (size as u16).into())
        .unwrap_or(TidalArtistImageSize::Max);

    log::debug!(
        "Getting Tidal artist picture from url={:?} size={size}",
        artist.picture_url(size)
    );

    let url = artist
        .picture_url(size)
        .ok_or(ArtistCoverError::NotFound(ArtistId::Tidal(tidal_artist_id)))?;

    log::debug!(
        "Got Tidal artist picture from url={:?} size={size}: {url}",
        artist.picture_url(size)
    );

    Ok(ArtistCoverRequest {
        url,
        file_path: get_artist_cover_path(&size.to_string(), "tidal", artist.id, &artist.name),
    })
}

pub async fn get_qobuz_artist_cover_bytes(
    qobuz_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    let request = get_qobuz_artist_cover_request(qobuz_artist_id, db, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

pub async fn get_qobuz_artist_cover(
    qobuz_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<String, ArtistCoverError> {
    let request = get_qobuz_artist_cover_request(qobuz_artist_id, db, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_qobuz_artist_cover_request(
    qobuz_artist_id: u64,
    db: Arc<Box<dyn Database>>,
    size: Option<u32>,
) -> Result<ArtistCoverRequest, ArtistCoverError> {
    static ARTIST_CACHE: Lazy<RwLock<HashMap<u64, Option<QobuzArtist>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    let artist = if let Some(artist) = {
        let binding = ARTIST_CACHE.read().unwrap();
        binding.get(&qobuz_artist_id).cloned()
    } {
        artist
    } else {
        use moosicbox_qobuz::AuthenticatedRequestError;

        let artist = match moosicbox_qobuz::artist(db, &qobuz_artist_id.into(), None, None).await {
            Ok(album) => Ok(Some(album)),
            Err(err) => match err {
                QobuzArtistError::AuthenticatedRequest(
                    AuthenticatedRequestError::RequestFailed(404, _),
                ) => Ok(None),
                _ => Err(err),
            },
        }?;

        ARTIST_CACHE
            .write()
            .as_mut()
            .unwrap()
            .insert(qobuz_artist_id, artist.clone());

        artist
    }
    .ok_or_else(|| ArtistCoverError::NotFound(ArtistId::Qobuz(qobuz_artist_id)))?;

    let size = size
        .map(|size| (size as u16).into())
        .unwrap_or(QobuzImageSize::Mega);

    let url = artist
        .image
        .as_ref()
        .and_then(|image| image.cover_url_for_size(size))
        .ok_or(ArtistCoverError::NotFound(ArtistId::Qobuz(qobuz_artist_id)))?;

    Ok(ArtistCoverRequest {
        url,
        file_path: get_artist_cover_path(&size.to_string(), "qobuz", artist.id, &artist.name),
    })
}
