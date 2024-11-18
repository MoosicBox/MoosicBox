#![allow(clippy::module_name_repetitions)]

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_core::sqlite::{
    db::DbError,
    models::{Artist, Id},
};
use moosicbox_database::{profiles::LibraryDatabase, query::FilterableQuery, DatabaseError};
use moosicbox_music_api::{
    models::{ImageCoverSize, ImageCoverSource},
    ArtistError, MusicApi,
};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    get_or_fetch_cover_bytes_from_remote_url, get_or_fetch_cover_from_remote_url,
    sanitize_filename, search_for_cover, CoverBytes, FetchCoverError,
};

fn get_artist_cover_path(size: &str, source: &str, artist_id: &str, artist_name: &str) -> PathBuf {
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name));

    let filename = format!("artist_{artist_id}_{size}.jpg");

    path.join(filename)
}

fn get_artist_directory(artist: &Artist) -> Option<String> {
    artist
        .cover
        .as_ref()
        .and_then(|x| PathBuf::from_str(x.as_str()).ok())
        .and_then(|x| x.parent().and_then(|x| x.to_str()).map(ToString::to_string))
}

#[derive(Debug, Error)]
pub enum ArtistCoverError {
    #[error("Artist cover not found for artist: {0} ({1})")]
    NotFound(Id, String),
    #[error(transparent)]
    Artist(#[from] ArtistError),
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    #[error(transparent)]
    FetchLocalArtistCover(#[from] FetchLocalArtistCoverError),
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
    #[error("Invalid source")]
    InvalidSource,
}

/// # Errors
///
/// * If the artist cover was not found
/// * If failed to get the artist info
/// * If an IO error occurs
/// * If a database error occurs
/// * If the `ApiSource` is invalid
pub async fn get_local_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    artist: &Artist,
    size: ImageCoverSize,
) -> Result<String, ArtistCoverError> {
    log::debug!(
        "get_local_artist_cover: api_source={} artist={artist:?} size={size}",
        api.source()
    );
    let source = api
        .artist_cover_source(artist, size)
        .await?
        .ok_or_else(|| {
            log::debug!("get_local_artist_cover: artist cover source not found");
            ArtistCoverError::NotFound(
                artist.id.clone(),
                "Artist cover source not found".to_owned(),
            )
        })?;

    let directory = get_artist_directory(artist);
    if let Ok(cover) =
        fetch_local_artist_cover(db, artist, source.clone(), directory.as_ref()).await
    {
        return Ok(cover);
    }

    if let Ok(cover) = get_remote_artist_cover(artist, source, size).await {
        log::debug!("Found {} artist cover", api.source());
        return copy_streaming_cover_to_local(db, artist, cover).await;
    }

    Err(ArtistCoverError::NotFound(
        artist.id.clone(),
        "Artist cover remote image not found".to_owned(),
    ))
}

/// # Errors
///
/// * If the artist cover was not found
/// * If failed to get the artist info
/// * If an IO error occurs
/// * If a database error occurs
/// * If the `ApiSource` is invalid
pub async fn get_local_artist_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    artist: &Artist,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    let source = api
        .artist_cover_source(artist, size)
        .await?
        .ok_or_else(|| {
            ArtistCoverError::NotFound(
                artist.id.clone(),
                "Artist cover source not found".to_owned(),
            )
        })?;

    let directory = get_artist_directory(artist);
    if let Ok(cover) = fetch_local_artist_cover_bytes(db, artist, directory.as_ref()).await {
        return Ok(cover);
    }

    if let Ok(cover) =
        get_remote_artist_cover_bytes(artist, source, size, try_to_get_stream_size).await
    {
        return Ok(cover);
    }

    Err(ArtistCoverError::NotFound(
        artist.id.clone(),
        "Artist cover remote image not found".to_owned(),
    ))
}

#[derive(Debug, Error)]
pub enum FetchLocalArtistCoverError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("No Artist Cover")]
    NoArtistCover,
    #[error("Invalid source")]
    InvalidSource,
}

async fn fetch_local_artist_cover(
    db: &LibraryDatabase,
    artist: &Artist,
    source: ImageCoverSource,
    directory: Option<&String>,
) -> Result<String, FetchLocalArtistCoverError> {
    match source {
        ImageCoverSource::LocalFilePath(cover) => {
            let cover_path = std::path::PathBuf::from(&cover);

            if Path::is_file(&cover_path) {
                return Ok(cover_path.to_str().unwrap().to_string());
            }

            let directory = directory.ok_or(FetchLocalArtistCoverError::NoArtistCover)?;
            let directory_path = std::path::PathBuf::from(directory);

            if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
                let new_cover = path.to_str().unwrap().to_string();

                log::debug!(
                    "Updating Artist {} cover file from '{cover}' to '{new_cover}'",
                    &artist.id
                );

                db.update("artists")
                    .where_eq("id", &artist.id)
                    .value("cover", new_cover)
                    .execute(db)
                    .await?;

                return Ok(path.to_str().unwrap().to_string());
            }

            Err(FetchLocalArtistCoverError::NoArtistCover)
        }
        ImageCoverSource::RemoteUrl(_) => Err(FetchLocalArtistCoverError::InvalidSource),
    }
}

async fn fetch_local_artist_cover_bytes(
    db: &LibraryDatabase,
    artist: &Artist,
    directory: Option<&String>,
) -> Result<CoverBytes, FetchLocalArtistCoverError> {
    let cover = artist
        .cover
        .as_ref()
        .ok_or(FetchLocalArtistCoverError::NoArtistCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::is_file(&cover_path) {
        let file = tokio::fs::File::open(cover_path.clone()).await?;

        let size = (file.metadata().await).map_or(None, |metadata| Some(metadata.len()));

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    let directory = directory.ok_or(FetchLocalArtistCoverError::NoArtistCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
        let new_cover = path.to_str().unwrap().to_string();

        log::debug!(
            "Updating Artist {} cover file from '{cover}' to '{new_cover}'",
            &artist.id
        );

        db.update("artists")
            .where_eq("id", &artist.id)
            .value("cover", new_cover)
            .execute(db)
            .await?;

        let file = tokio::fs::File::open(path).await?;

        let size = (file.metadata().await).map_or(None, |metadata| Some(metadata.len()));

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    Err(FetchLocalArtistCoverError::NoArtistCover)
}

async fn copy_streaming_cover_to_local(
    db: &LibraryDatabase,
    artist: &Artist,
    cover: String,
) -> Result<String, ArtistCoverError> {
    log::debug!("Updating Artist {} cover file to '{cover}'", artist.id);

    db.update("artists")
        .where_eq("id", &artist.id)
        .value("cover", cover.clone())
        .execute(db)
        .await?;

    Ok(cover)
}

/// # Errors
///
/// * If the artist cover was not found
/// * If failed to get the artist info
/// * If an IO error occurs
/// * If a database error occurs
/// * If the `ApiSource` is invalid
pub async fn get_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    artist: &Artist,
    size: ImageCoverSize,
) -> Result<String, ArtistCoverError> {
    get_local_artist_cover(api, db, artist, size).await
}

/// # Errors
///
/// * If the artist cover was not found
/// * If failed to get the artist info
/// * If an IO error occurs
/// * If a database error occurs
/// * If the `ApiSource` is invalid
pub async fn get_artist_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    artist: &Artist,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    get_local_artist_cover_bytes(api, db, artist, size, try_to_get_stream_size).await
}

fn get_remote_artist_cover_request(
    artist: &Artist,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<ArtistCoverRequest, ArtistCoverError> {
    match source {
        ImageCoverSource::LocalFilePath(_) => Err(ArtistCoverError::InvalidSource),
        ImageCoverSource::RemoteUrl(url) => {
            let file_path = get_artist_cover_path(
                &size.to_string(),
                artist.api_source.as_ref(),
                &artist.id.to_string(),
                &artist.title,
            );

            Ok(ArtistCoverRequest { url, file_path })
        }
    }
}

async fn get_remote_artist_cover(
    artist: &Artist,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<String, ArtistCoverError> {
    let request = get_remote_artist_cover_request(artist, source, size)?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_remote_artist_cover_bytes(
    artist: &Artist,
    source: ImageCoverSource,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, ArtistCoverError> {
    let request = get_remote_artist_cover_request(artist, source, size)?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

struct ArtistCoverRequest {
    url: String,
    file_path: PathBuf,
}
