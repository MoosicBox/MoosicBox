//! Album cover image fetching and caching.
//!
//! Provides functionality for retrieving album cover artwork from local files or remote URLs,
//! with database integration for tracking cover locations and automatic fallback between sources.

#![allow(clippy::module_name_repetitions)]

use std::path::{Path, PathBuf};

use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_music_api::{
    MusicApi,
    models::{ImageCoverSize, ImageCoverSource},
};
use moosicbox_music_models::{Album, id::Id};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use switchy_database::{DatabaseError, profiles::LibraryDatabase, query::FilterableQuery};
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    CoverBytes, FetchCoverError, get_or_fetch_cover_bytes_from_remote_url,
    get_or_fetch_cover_from_remote_url, sanitize_filename, search_for_cover,
};

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

    let filename = format!("album_{album_id}_{size}.jpg");

    path.join(filename)
}

/// Errors that can occur when retrieving album cover artwork.
#[derive(Debug, Error)]
pub enum AlbumCoverError {
    /// Album cover not found for the specified album ID
    #[error("Album cover not found for album: {0}")]
    NotFound(Id),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Error fetching cover from remote source
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    /// Error fetching local album cover file
    #[error(transparent)]
    FetchLocalAlbumCover(#[from] FetchLocalAlbumCoverError),
    /// IO error reading or writing cover file
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to read cover file at the specified path
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
    /// Invalid or unsupported API source
    #[error("Invalid source")]
    InvalidSource,
}

/// Retrieves the local file path to an album cover image.
///
/// First checks for a local file, then falls back to fetching from remote sources if available.
/// Updates the database with the located cover path.
///
/// # Errors
///
/// * `AlbumCoverError::NotFound` - If the album cover was not found
/// * `AlbumCoverError::MusicApi` - If failed to get the album info
/// * `AlbumCoverError::IO` - If an IO error occurs
/// * `AlbumCoverError::Database` - If a database error occurs
/// * `AlbumCoverError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_local_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    let source = api
        .album_cover_source(album, size)
        .await?
        .ok_or_else(|| AlbumCoverError::NotFound(album.id.clone()))?;

    if let Ok(cover) =
        fetch_local_album_cover(db, album, source.clone(), album.directory.as_ref()).await
    {
        return Ok(cover);
    }

    if let Ok(cover) = get_remote_album_cover(album, source, size).await {
        log::debug!("Found {} artist cover", api.source());
        return copy_streaming_cover_to_local(db, album, cover).await;
    }

    Err(AlbumCoverError::NotFound(album.id.clone()))
}

/// Retrieves an album cover image as a stream of bytes.
///
/// First checks for a local file, then falls back to fetching from remote sources if available.
/// Returns a byte stream suitable for streaming to clients.
///
/// # Errors
///
/// * `AlbumCoverError::NotFound` - If the album cover was not found
/// * `AlbumCoverError::MusicApi` - If failed to get the album info
/// * `AlbumCoverError::IO` - If an IO error occurs
/// * `AlbumCoverError::Database` - If a database error occurs
/// * `AlbumCoverError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_local_album_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let source = api
        .album_cover_source(album, size)
        .await?
        .ok_or_else(|| AlbumCoverError::NotFound(album.id.clone()))?;

    if let Ok(cover) = fetch_local_album_cover_bytes(db, album, album.directory.as_ref()).await {
        return Ok(cover);
    }

    if let Ok(cover) =
        get_remote_album_cover_bytes(album, source, size, try_to_get_stream_size).await
    {
        return Ok(cover);
    }

    Err(AlbumCoverError::NotFound(album.id.clone()))
}

/// Errors that can occur when fetching local album cover files.
#[derive(Debug, Error)]
pub enum FetchLocalAlbumCoverError {
    /// IO error reading cover file
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// No album cover available
    #[error("No Album Cover")]
    NoAlbumCover,
    /// Invalid or unsupported source type
    #[error("Invalid source")]
    InvalidSource,
}

async fn fetch_local_album_cover(
    db: &LibraryDatabase,
    album: &Album,
    source: ImageCoverSource,
    directory: Option<&String>,
) -> Result<String, FetchLocalAlbumCoverError> {
    log::trace!("fetch_local_album_cover");
    match source {
        ImageCoverSource::LocalFilePath(cover) => {
            let cover_path = std::path::PathBuf::from(&cover);
            log::trace!(
                "fetch_local_album_cover: LocalFilePath cover_path={}",
                cover_path.display()
            );

            if Path::is_file(&cover_path) {
                log::trace!(
                    "fetch_local_album_cover: is_file cover_path={}",
                    cover_path.display()
                );
                return Ok(cover_path.to_str().unwrap().to_string());
            }

            let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
            let directory_path = std::path::PathBuf::from(directory);

            if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
                log::trace!("fetch_local_album_cover: found path={}", path.display());
                let artwork = path.to_str().unwrap().to_string();

                log::debug!(
                    "Updating Album {} artwork file from '{cover}' to '{artwork}'",
                    &album.id
                );

                db.update("albums")
                    .where_eq("id", &album.id)
                    .value("artwork", artwork)
                    .execute(&**db)
                    .await?;

                return Ok(path.to_str().unwrap().to_string());
            }

            Err(FetchLocalAlbumCoverError::NoAlbumCover)
        }
        ImageCoverSource::RemoteUrl { .. } => Err(FetchLocalAlbumCoverError::InvalidSource),
    }
}

async fn fetch_local_album_cover_bytes(
    db: &LibraryDatabase,
    album: &Album,
    directory: Option<&String>,
) -> Result<CoverBytes, FetchLocalAlbumCoverError> {
    let cover = album
        .artwork
        .as_ref()
        .ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;

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

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!(
            "Updating Album {} artwork file from '{cover}' to '{artwork}'",
            &album.id
        );

        db.update("albums")
            .where_eq("id", &album.id)
            .value("artwork", artwork)
            .execute(&**db)
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

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

async fn copy_streaming_cover_to_local(
    db: &LibraryDatabase,
    album: &Album,
    cover: String,
) -> Result<String, AlbumCoverError> {
    log::debug!("Updating Album {} cover file to '{cover}'", album.id);

    db.update("albums")
        .where_eq("id", &album.id)
        .value("artwork", cover.clone())
        .execute(&**db)
        .await?;

    Ok(cover)
}

/// Retrieves the file path to an album cover image at the specified size.
///
/// This is the main public API for getting album covers. It delegates to `get_local_album_cover`
/// to handle local and remote sources.
///
/// # Errors
///
/// * `AlbumCoverError::NotFound` - If the album cover was not found
/// * `AlbumCoverError::MusicApi` - If failed to get the album info
/// * `AlbumCoverError::IO` - If an IO error occurs
/// * `AlbumCoverError::Database` - If a database error occurs
/// * `AlbumCoverError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    get_local_album_cover(api, db, album, size).await
}

/// Retrieves an album cover image as a stream of bytes at the specified size.
///
/// This is the main public API for getting album cover byte streams. It delegates to
/// `get_local_album_cover_bytes` to handle local and remote sources.
///
/// # Errors
///
/// * `AlbumCoverError::NotFound` - If the album cover was not found
/// * `AlbumCoverError::MusicApi` - If failed to get the album info
/// * `AlbumCoverError::IO` - If an IO error occurs
/// * `AlbumCoverError::Database` - If a database error occurs
/// * `AlbumCoverError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_album_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    get_local_album_cover_bytes(api, db, album, size, try_to_get_stream_size).await
}

fn get_remote_album_cover_request(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<AlbumCoverRequest, AlbumCoverError> {
    match source {
        ImageCoverSource::LocalFilePath(_) => Err(AlbumCoverError::InvalidSource),
        ImageCoverSource::RemoteUrl { url, headers } => {
            let file_path = get_album_cover_path(
                &size.to_string(),
                album.album_source.as_ref(),
                &album.id.to_string(),
                &album.artist,
                &album.title,
            );

            Ok(AlbumCoverRequest {
                url,
                file_path,
                headers,
            })
        }
    }
}

async fn get_remote_album_cover(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    let request = get_remote_album_cover_request(album, source, size)?;

    Ok(get_or_fetch_cover_from_remote_url(
        &request.url,
        request.headers.as_deref(),
        &request.file_path,
    )
    .await?)
}

async fn get_remote_album_cover_bytes(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let request = get_remote_album_cover_request(album, source, size)?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        request.headers.as_deref(),
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

struct AlbumCoverRequest {
    url: String,
    file_path: PathBuf,
    headers: Option<Vec<(String, String)>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_get_album_cover_path_basic() {
        let path = get_album_cover_path("large", "library", "123", "Artist Name", "Album Title");

        // Check that the path ends with the expected filename
        assert!(path.to_string_lossy().ends_with("album_123_large.jpg"));

        // Check that the path contains the sanitized artist and album names
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("Artist_Name"));
        assert!(path_str.contains("Album_Title"));
        assert!(path_str.contains("library"));
    }

    #[test_log::test]
    fn test_get_album_cover_path_sanitizes_special_characters() {
        let path = get_album_cover_path(
            "max",
            "tidal",
            "456",
            "The Artist's Name!",
            "Album: Remastered (2024)",
        );

        let path_str = path.to_string_lossy();
        // Apostrophes and special chars should be replaced with underscores
        assert!(path_str.contains("The_Artist_s_Name_"));
        assert!(path_str.contains("Album__Remastered__2024_"));
        assert!(path_str.ends_with("album_456_max.jpg"));
    }

    #[test_log::test]
    fn test_get_album_cover_path_different_sizes() {
        let small_path = get_album_cover_path("small", "source", "1", "Artist", "Album");
        let medium_path = get_album_cover_path("medium", "source", "1", "Artist", "Album");
        let large_path = get_album_cover_path("large", "source", "1", "Artist", "Album");

        assert!(small_path.to_string_lossy().ends_with("album_1_small.jpg"));
        assert!(
            medium_path
                .to_string_lossy()
                .ends_with("album_1_medium.jpg")
        );
        assert!(large_path.to_string_lossy().ends_with("album_1_large.jpg"));
    }

    #[test_log::test]
    fn test_get_album_cover_path_unicode_characters() {
        let path = get_album_cover_path("max", "library", "789", "アーティスト", "アルバム");

        // Unicode characters should be sanitized to underscores
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("album_789_max.jpg"));
    }

    #[test_log::test]
    fn test_get_album_cover_path_empty_strings() {
        let path = get_album_cover_path("max", "library", "", "", "");

        // Even with empty strings, the path structure should be valid
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("album__max.jpg"));
    }

    #[test_log::test]
    fn test_get_album_cover_path_structure() {
        let path = get_album_cover_path("max", "source_name", "100", "My Artist", "My Album");

        // Check the path contains expected values in the structure
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("source_name"));
        assert!(path_str.contains("My_Artist"));
        assert!(path_str.contains("My_Album"));
        assert!(path_str.contains("album_100_max.jpg"));
    }
}
