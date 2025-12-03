//! Artist cover image fetching and caching.
//!
//! Provides functionality for retrieving artist cover artwork from local files or remote URLs,
//! with database integration for tracking cover locations and automatic fallback between sources.

#![allow(clippy::module_name_repetitions)]

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_music_api::{
    MusicApi,
    models::{ImageCoverSize, ImageCoverSource},
};
use moosicbox_music_models::{Artist, id::Id};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use switchy_database::{DatabaseError, profiles::LibraryDatabase, query::FilterableQuery};
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    CoverBytes, FetchCoverError, get_or_fetch_cover_bytes_from_remote_url,
    get_or_fetch_cover_from_remote_url, sanitize_filename, search_for_cover,
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

/// Errors that can occur when retrieving artist cover artwork.
#[derive(Debug, Error)]
pub enum ArtistCoverError {
    /// Artist cover not found for the specified artist ID
    #[error("Artist cover not found for artist: {0} ({1})")]
    NotFound(Id, String),
    /// Music API error
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
    /// Error fetching cover from remote source
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    /// Error fetching local artist cover file
    #[error(transparent)]
    FetchLocalArtistCover(#[from] FetchLocalArtistCoverError),
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

/// Retrieves the local file path to an artist cover image.
///
/// First checks for a local file, then falls back to fetching from remote sources if available.
/// Updates the database with the located cover path.
///
/// # Errors
///
/// * `ArtistCoverError::NotFound` - If the artist cover was not found
/// * `ArtistCoverError::MusicApi` - If failed to get the artist info
/// * `ArtistCoverError::IO` - If an IO error occurs
/// * `ArtistCoverError::Database` - If a database error occurs
/// * `ArtistCoverError::InvalidSource` - If the `ApiSource` is invalid
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

/// Retrieves an artist cover image as a stream of bytes.
///
/// First checks for a local file, then falls back to fetching from remote sources if available.
/// Returns a byte stream suitable for streaming to clients.
///
/// # Errors
///
/// * `ArtistCoverError::NotFound` - If the artist cover was not found
/// * `ArtistCoverError::MusicApi` - If failed to get the artist info
/// * `ArtistCoverError::IO` - If an IO error occurs
/// * `ArtistCoverError::Database` - If a database error occurs
/// * `ArtistCoverError::InvalidSource` - If the `ApiSource` is invalid
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

/// Errors that can occur when fetching local artist cover files.
#[derive(Debug, Error)]
pub enum FetchLocalArtistCoverError {
    /// IO error reading cover file
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// No artist cover available
    #[error("No Artist Cover")]
    NoArtistCover,
    /// Invalid or unsupported source type
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
                    .execute(&**db)
                    .await?;

                return Ok(path.to_str().unwrap().to_string());
            }

            Err(FetchLocalArtistCoverError::NoArtistCover)
        }
        ImageCoverSource::RemoteUrl { .. } => Err(FetchLocalArtistCoverError::InvalidSource),
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
        .execute(&**db)
        .await?;

    Ok(cover)
}

/// Retrieves the file path to an artist cover image at the specified size.
///
/// This is the main public API for getting artist covers. It delegates to `get_local_artist_cover`
/// to handle local and remote sources.
///
/// # Errors
///
/// * `ArtistCoverError::NotFound` - If the artist cover was not found
/// * `ArtistCoverError::MusicApi` - If failed to get the artist info
/// * `ArtistCoverError::IO` - If an IO error occurs
/// * `ArtistCoverError::Database` - If a database error occurs
/// * `ArtistCoverError::InvalidSource` - If the `ApiSource` is invalid
pub async fn get_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    artist: &Artist,
    size: ImageCoverSize,
) -> Result<String, ArtistCoverError> {
    get_local_artist_cover(api, db, artist, size).await
}

/// Retrieves an artist cover image as a stream of bytes at the specified size.
///
/// This is the main public API for getting artist cover byte streams. It delegates to
/// `get_local_artist_cover_bytes` to handle local and remote sources.
///
/// # Errors
///
/// * `ArtistCoverError::NotFound` - If the artist cover was not found
/// * `ArtistCoverError::MusicApi` - If failed to get the artist info
/// * `ArtistCoverError::IO` - If an IO error occurs
/// * `ArtistCoverError::Database` - If a database error occurs
/// * `ArtistCoverError::InvalidSource` - If the `ApiSource` is invalid
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
        ImageCoverSource::RemoteUrl { url, headers } => {
            let file_path = get_artist_cover_path(
                &size.to_string(),
                artist.api_source.as_ref(),
                &artist.id.to_string(),
                &artist.title,
            );

            Ok(ArtistCoverRequest {
                url,
                file_path,
                headers,
            })
        }
    }
}

async fn get_remote_artist_cover(
    artist: &Artist,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<String, ArtistCoverError> {
    let request = get_remote_artist_cover_request(artist, source, size)?;

    Ok(get_or_fetch_cover_from_remote_url(
        &request.url,
        request.headers.as_deref(),
        &request.file_path,
    )
    .await?)
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
        request.headers.as_deref(),
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

struct ArtistCoverRequest {
    url: String,
    file_path: PathBuf,
    headers: Option<Vec<(String, String)>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::{ApiSource, ApiSources};

    fn create_test_artist(cover: Option<String>) -> Artist {
        Artist {
            id: moosicbox_music_models::id::Id::Number(1),
            title: "Test Artist".to_string(),
            cover,
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        }
    }

    #[test_log::test]
    fn test_get_artist_directory_with_cover_path() {
        let artist = create_test_artist(Some("/music/artist/cover.jpg".to_string()));
        let result = get_artist_directory(&artist);
        assert_eq!(result, Some("/music/artist".to_string()));
    }

    #[test_log::test]
    fn test_get_artist_directory_with_deeply_nested_path() {
        let artist = create_test_artist(Some(
            "/home/user/music/library/artist/album/cover.png".to_string(),
        ));
        let result = get_artist_directory(&artist);
        assert_eq!(
            result,
            Some("/home/user/music/library/artist/album".to_string())
        );
    }

    #[test_log::test]
    fn test_get_artist_directory_without_cover() {
        let artist = create_test_artist(None);
        let result = get_artist_directory(&artist);
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_get_artist_directory_with_root_file() {
        let artist = create_test_artist(Some("/cover.jpg".to_string()));
        let result = get_artist_directory(&artist);
        assert_eq!(result, Some("/".to_string()));
    }

    #[test_log::test]
    fn test_get_artist_directory_with_relative_path() {
        let artist = create_test_artist(Some("music/artist/cover.jpg".to_string()));
        let result = get_artist_directory(&artist);
        assert_eq!(result, Some("music/artist".to_string()));
    }

    #[test_log::test]
    fn test_get_artist_directory_filename_only() {
        // When there's only a filename with no directory
        let artist = create_test_artist(Some("cover.jpg".to_string()));
        let result = get_artist_directory(&artist);
        // The parent of a file without directory parts is an empty string
        assert_eq!(result, Some(String::new()));
    }

    #[test_log::test]
    fn test_get_artist_cover_path_basic() {
        let path = get_artist_cover_path("large", "library", "123", "Artist Name");

        // Check that the path ends with the expected filename
        assert!(path.to_string_lossy().ends_with("artist_123_large.jpg"));

        // Check that the path contains the sanitized artist name
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("Artist_Name"));
        assert!(path_str.contains("library"));
    }

    #[test_log::test]
    fn test_get_artist_cover_path_sanitizes_special_characters() {
        let path = get_artist_cover_path("max", "tidal", "456", "The Artist's Name!");

        let path_str = path.to_string_lossy();
        // Apostrophes and special chars should be replaced with underscores
        assert!(path_str.contains("The_Artist_s_Name_"));
        assert!(path_str.ends_with("artist_456_max.jpg"));
    }

    #[test_log::test]
    fn test_get_artist_cover_path_different_sizes() {
        let small_path = get_artist_cover_path("small", "source", "1", "Artist");
        let medium_path = get_artist_cover_path("medium", "source", "1", "Artist");
        let large_path = get_artist_cover_path("large", "source", "1", "Artist");

        assert!(small_path.to_string_lossy().ends_with("artist_1_small.jpg"));
        assert!(
            medium_path
                .to_string_lossy()
                .ends_with("artist_1_medium.jpg")
        );
        assert!(large_path.to_string_lossy().ends_with("artist_1_large.jpg"));
    }

    #[test_log::test]
    fn test_get_artist_cover_path_unicode_characters() {
        let path = get_artist_cover_path("max", "library", "789", "アーティスト");

        // Unicode characters should be sanitized to underscores
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("artist_789_max.jpg"));
    }

    #[test_log::test]
    fn test_get_artist_cover_path_empty_strings() {
        let path = get_artist_cover_path("max", "library", "", "");

        // Even with empty strings, the path structure should be valid
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("artist__max.jpg"));
    }
}
