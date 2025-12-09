//! File handling utilities for `MoosicBox` music server.
//!
//! This crate provides comprehensive file operations for the `MoosicBox` music server, including:
//!
//! * **HTTP file operations** - Download and stream files from remote URLs with progress tracking
//! * **Media cover images** - Fetch and cache album/artist cover artwork
//! * **Audio file handling** - Process and serve audio tracks in various formats
//! * **Byte range support** - Handle partial content requests for streaming media
//!
//! # Features
//!
//! * `api` - Actix-web HTTP endpoints for file services
//! * `files` - Core file handling and track management
//! * `range` - Byte range parsing for partial content requests
//! * `image` / `libvips` - Image resizing and processing
//! * Format-specific features: `format-aac`, `format-flac`, `format-mp3`, `format-opus`
//!
//! # Examples
//!
//! Download and save a file from a remote URL:
//!
//! ```rust,no_run
//! # use moosicbox_files::{fetch_and_save_bytes_from_remote_url};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = switchy_http::Client::new();
//! let file_path = std::path::Path::new("/tmp/audio.flac");
//! let url = "https://example.com/audio.flac";
//!
//! fetch_and_save_bytes_from_remote_url(&client, file_path, url, None).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    io::{Seek, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, LazyLock, atomic::AtomicUsize},
};

use atomic_float::AtomicF64;
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use futures_core::{Future, Stream};
use moosicbox_audiotags::AudioTag;
use thiserror::Error;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    pin,
};

/// HTTP API endpoints for file services using Actix-web.
///
/// Provides REST endpoints for streaming tracks, fetching cover images, and retrieving track metadata.
#[cfg(feature = "api")]
pub mod api;

/// Core file handling operations for tracks, albums, and artists.
///
/// Includes functionality for managing track files, cover images, and audio visualization data.
#[cfg(feature = "files")]
pub mod files;

/// Byte range parsing for HTTP partial content requests.
///
/// Supports parsing RFC 7233 byte range specifications for streaming media.
#[cfg(feature = "range")]
pub mod range;

static NON_ALPHA_NUMERIC_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

/// Sanitizes a filename by replacing all non-alphanumeric characters with underscores.
///
/// Preserves only ASCII letters (A-Z, a-z), digits (0-9), and underscores.
#[must_use]
pub fn sanitize_filename(string: &str) -> String {
    NON_ALPHA_NUMERIC_REGEX.replace_all(string, "_").to_string()
}

/// Errors that can occur when retrieving content length from a remote URL.
#[derive(Debug, Error)]
pub enum GetContentLengthError {
    /// HTTP request error
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// Failed to parse content-length header as integer
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

static CLIENT: LazyLock<switchy_http::Client> =
    LazyLock::new(|| switchy_http::Client::builder().build().unwrap());

/// Retrieves the content length of a remote resource via HTTP HEAD request.
///
/// Optionally supports byte range requests to get the size of a specific range.
///
/// # Errors
///
/// * If the HTTP request fails
/// * If the content-length header value is not a valid `u64`
pub async fn get_content_length(
    url: &str,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<Option<u64>, GetContentLengthError> {
    let mut client = CLIENT.head(url);

    if start.is_some() || end.is_some() {
        let start = start.map_or_else(String::new, |x| x.to_string());
        let end = end.map_or_else(String::new, |x| x.to_string());

        client = client.header(
            switchy_http::Header::Range.as_ref(),
            &format!("bytes={start}-{end}"),
        );
    }

    let mut res = client.send().await?;

    Ok(
        if let Some(header) = res
            .headers()
            .get(switchy_http::Header::ContentLength.as_ref())
        {
            Some(header.parse::<u64>()?)
        } else {
            None
        },
    )
}

/// Saves bytes to a file at the specified path, optionally starting at a byte offset.
///
/// Creates parent directories if they don't exist. If `start` is `None` or `0`, the file is truncated.
///
/// # Panics
///
/// * If the path has no parent directory
///
/// # Errors
///
/// * If there is an IO error creating directories or writing the file
pub fn save_bytes_to_file(
    bytes: &[u8],
    path: &Path,
    start: Option<u64>,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = switchy_fs::sync::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(start.is_none_or(|start| start == 0))
        .open(path)?;

    let mut writer = std::io::BufWriter::new(file);

    if let Some(start) = start {
        writer.seek(std::io::SeekFrom::Start(start))?;
    }

    writer.write_all(bytes)
}

/// Errors that can occur when saving a byte stream to a file.
#[derive(Debug, Error)]
pub enum SaveBytesStreamToFileError {
    /// General IO error
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    /// Error reading from the stream after processing some bytes
    #[error("IO Error after read {bytes_read} bytes: {source:?}")]
    Read {
        /// Number of bytes successfully read before error
        bytes_read: u64,
        #[source]
        source: tokio::io::Error,
    },
    /// Error writing to file after reading some bytes
    #[error("IO Error after reading {bytes_read} bytes: {source:?}")]
    Write {
        /// Number of bytes read before write error
        bytes_read: u64,
        /// The underlying IO error
        #[source]
        source: tokio::io::Error,
    },
}

/// Saves a stream of bytes to a file, optionally starting at a byte offset.
///
/// # Errors
///
/// * If there is an IO error reading from the stream or writing to the file
pub async fn save_bytes_stream_to_file<S: Stream<Item = Result<Bytes, std::io::Error>> + Send>(
    stream: S,
    path: &Path,
    start: Option<u64>,
) -> Result<(), SaveBytesStreamToFileError> {
    save_bytes_stream_to_file_with_progress_listener(stream, path, start, None).await
}

type OnSpeed = Box<dyn (FnMut(f64) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>;
// type OnProgress = Box<dyn FnMut(usize, usize) + Send + Sync>;
type OnProgressFut = Pin<Box<dyn Future<Output = ()> + Send>>;
type OnProgress = Box<dyn (FnMut(usize, usize) -> OnProgressFut) + Send>;

/// Saves a stream of bytes to a file with download speed and progress callbacks.
///
/// Tracks download speed in bytes per second and invokes callbacks for speed updates and progress.
///
/// # Errors
///
/// * If there is an IO error reading from the stream or writing to the file
pub async fn save_bytes_stream_to_file_with_speed_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    on_speed: OnSpeed,
    on_progress: Option<OnProgress>,
) -> Result<(), SaveBytesStreamToFileError> {
    let last_instant = Arc::new(switchy_async::sync::Mutex::new(switchy_time::instant_now()));
    let bytes_since_last_interval = Arc::new(AtomicUsize::new(0));
    let speed = Arc::new(AtomicF64::new(0.0));

    let has_on_progress = on_progress.is_some();
    let on_progress = Arc::new(switchy_async::sync::Mutex::new(
        on_progress.unwrap_or_else(|| Box::new(|_, _| Box::pin(async move {}) as OnProgressFut)),
    ));
    let on_speed = Arc::new(switchy_async::sync::Mutex::new(on_speed));

    save_bytes_stream_to_file_with_progress_listener(
        stream,
        path,
        start,
        Some(Box::new({
            move |read, total| {
                let last_instant = last_instant.clone();
                let bytes_since_last_interval = bytes_since_last_interval.clone();
                let speed = speed.clone();
                let on_progress = on_progress.clone();
                let on_speed = on_speed.clone();
                Box::pin(async move {
                    if has_on_progress {
                        (on_progress.lock().await)(read, total).await;
                    }

                    let mut last_instant = last_instant.lock().await;
                    let bytes = bytes_since_last_interval
                        .fetch_add(read, std::sync::atomic::Ordering::SeqCst)
                        + read;
                    let now = switchy_time::instant_now();
                    let millis = now.duration_since(*last_instant).as_millis();

                    if millis >= 1000 {
                        #[allow(clippy::cast_precision_loss)]
                        let speed_millis = (bytes as f64) * (millis as f64 / 1000.0);
                        speed.store(speed_millis, std::sync::atomic::Ordering::SeqCst);
                        log::debug!(
                            "Speed: {speed_millis} b/s {} KiB/s {} MiB/s",
                            speed_millis / 1024.0,
                            speed_millis / 1024.0 / 1024.0,
                        );
                        (on_speed.lock().await)(speed_millis).await;
                        *last_instant = now;
                        drop(last_instant);
                        bytes_since_last_interval.store(0, std::sync::atomic::Ordering::SeqCst);
                    }
                }) as Pin<Box<dyn Future<Output = ()> + Send>>
            }
        })),
    )
    .await
}

/// Saves a stream of bytes to a file with progress tracking.
///
/// Invokes the optional progress callback with bytes written per chunk and total bytes written.
///
/// # Panics
///
/// * If the path has no parent directory
///
/// # Errors
///
/// * If there is an IO error reading from the stream or writing to the file
pub async fn save_bytes_stream_to_file_with_progress_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    on_progress: Option<OnProgress>,
) -> Result<(), SaveBytesStreamToFileError> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = switchy_fs::unsync::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(start.is_none_or(|start| start == 0))
        .open(path)
        .await?;

    let mut writer = BufWriter::new(file);

    if let Some(start) = start {
        writer.seek(std::io::SeekFrom::Start(start)).await?;
    }

    pin!(stream);

    let mut read = usize::try_from(start.unwrap_or(0)).unwrap();

    let has_on_progress = on_progress.is_some();
    let mut on_progress = on_progress.unwrap_or_else(|| {
        Box::new(|_, _| Box::pin(async move {}) as Pin<Box<dyn Future<Output = ()> + Send>>)
    });

    while let Some(bytes) = stream.next().await {
        let bytes = bytes.map_err(|err| SaveBytesStreamToFileError::Read {
            bytes_read: read as u64,
            source: err,
        })?;

        let len = bytes.len();

        read += len;

        log::trace!("Writing bytes to {}: {len} ({read} total)", path.display());

        writer
            .write(&bytes)
            .await
            .map_err(|err| SaveBytesStreamToFileError::Write {
                bytes_read: read as u64,
                source: err,
            })?;

        if has_on_progress {
            on_progress(len, read).await;
        }
    }

    writer.flush().await?;

    Ok(())
}

/// Errors that can occur when fetching cover artwork.
#[derive(Debug, Error)]
pub enum FetchCoverError {
    /// HTTP request error
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// IO error reading or writing cover file
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Error getting content length
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    /// Error fetching and saving bytes from remote URL
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

#[cfg(feature = "files")]
pub(crate) type BytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// Cover image bytes with optional size information.
///
/// Contains a byte stream for streaming cover image data along with its size (if known).
/// Used for returning cover artwork from albums and artists to HTTP clients.
#[cfg(feature = "files")]
pub struct CoverBytes {
    /// Stream of image bytes
    pub stream: moosicbox_stream_utils::stalled_monitor::StalledReadMonitor<
        Result<Bytes, std::io::Error>,
        BytesStream,
    >,
    /// Optional size of the image in bytes
    pub size: Option<u64>,
}

#[cfg(feature = "files")]
async fn get_or_fetch_cover_bytes_from_remote_url(
    url: &str,
    headers: Option<&[(String, String)]>,
    file_path: &Path,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, FetchCoverError> {
    use tokio_util::codec::{BytesCodec, FramedRead};

    static IMAGE_CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

    if Path::exists(file_path) {
        let file = tokio::fs::File::open(file_path.to_path_buf()).await?;

        let size = (file.metadata().await).map_or(None, |metadata| Some(metadata.len()));

        return Ok(CoverBytes {
            stream: moosicbox_stream_utils::stalled_monitor::StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(bytes::BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    let size = if try_to_get_stream_size {
        get_content_length(url, None, None).await?
    } else {
        None
    };

    Ok(CoverBytes {
        stream: moosicbox_stream_utils::stalled_monitor::StalledReadMonitor::new(
            fetch_bytes_from_remote_url(&IMAGE_CLIENT, url, headers).await?,
        ),
        size,
    })
}

#[cfg(feature = "files")]
async fn get_or_fetch_cover_from_remote_url(
    url: &str,
    headers: Option<&[(String, String)]>,
    file_path: &Path,
) -> Result<String, FetchCoverError> {
    use std::sync::LazyLock;

    static IMAGE_CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

    if Path::exists(file_path) {
        Ok(file_path.to_str().unwrap().to_string())
    } else {
        Ok(
            fetch_and_save_bytes_from_remote_url(&IMAGE_CLIENT, file_path, url, headers)
                .await?
                .to_str()
                .unwrap()
                .to_string(),
        )
    }
}

/// Errors that can occur when fetching and saving bytes from a remote URL.
#[derive(Debug, Error)]
pub enum FetchAndSaveBytesFromRemoteUrlError {
    /// HTTP request error
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// IO error writing to file
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Error saving byte stream to file
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    /// HTTP request returned non-success status code
    #[error("Request failed: (error {status})")]
    RequestFailed {
        /// HTTP status code
        status: u16,
        /// Response body message
        message: String,
    },
}

/// Fetches bytes from a remote URL as a stream.
///
/// Returns a stream of byte chunks that can be processed incrementally.
///
/// # Errors
///
/// * If the HTTP request fails
/// * If the server returns a non-success status code
pub async fn fetch_bytes_from_remote_url(
    client: &switchy_http::Client,
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    FetchAndSaveBytesFromRemoteUrlError,
> {
    log::debug!("Fetching bytes from remote url: {url}");
    let mut builder = client.get(url);

    for (k, v) in headers.unwrap_or_default() {
        builder = builder.header(k, v);
    }

    let response = builder.send().await?;

    let status = response.status();

    if !status.is_success() {
        let message = response.text().await.unwrap_or_else(|_| String::new());

        log::error!("Request failed: {status} ({message})");
        return Err(FetchAndSaveBytesFromRemoteUrlError::RequestFailed {
            status: status.into(),
            message,
        });
    }

    Ok(response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .boxed())
}

/// Downloads bytes from a remote URL and saves them to a file.
///
/// Creates parent directories if they don't exist.
///
/// # Errors
///
/// * If the HTTP request fails
/// * If there is an IO error creating directories or writing the file
pub async fn fetch_and_save_bytes_from_remote_url(
    client: &switchy_http::Client,
    file_path: &Path,
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<PathBuf, FetchAndSaveBytesFromRemoteUrlError> {
    log::debug!("Saving bytes to file: {}", file_path.display());
    let stream = fetch_bytes_from_remote_url(client, url, headers).await?;
    save_bytes_stream_to_file(stream, file_path, None).await?;
    Ok(file_path.to_path_buf())
}

/// Searches for cover artwork in a directory or extracts it from audio file tags.
///
/// First searches the directory for files matching the filename pattern. If not found and a tag
/// is provided, extracts embedded cover art and saves it to the `save_path`.
///
/// # Errors
///
/// * If there is an IO error reading the directory or saving the cover file
pub async fn search_for_cover(
    path: PathBuf,
    filename: &str,
    save_path: Option<PathBuf>,
    tag: Option<Box<dyn AudioTag + Send + Sync>>,
) -> Result<Option<PathBuf>, std::io::Error> {
    log::trace!("Searching for cover {}", path.display());
    if let Ok(mut cover_dir) = tokio::fs::read_dir(path.clone()).await {
        let mut entries = vec![];
        while let Ok(Some(p)) = cover_dir.next_entry().await {
            entries.push(p);
        }

        // Sort entries for deterministic processing
        entries.sort_by_key(tokio::fs::DirEntry::file_name);

        for p in entries {
            if p.file_name().to_str().is_some_and(|name| {
                name.to_lowercase()
                    .starts_with(format!("{filename}.").as_str())
            }) {
                return Ok(Some(p.path()));
            }
        }
    }
    if let Some(save_path) = save_path
        && let Some(tag) = tag
        && let Some(tag_cover) = tag.album_cover()
    {
        let cover_file_path = match tag_cover.mime_type {
            moosicbox_audiotags::MimeType::Png => save_path.join(format!("{filename}.png")),
            moosicbox_audiotags::MimeType::Jpeg => save_path.join(format!("{filename}.jpg")),
            moosicbox_audiotags::MimeType::Tiff => save_path.join(format!("{filename}.tiff")),
            moosicbox_audiotags::MimeType::Bmp => save_path.join(format!("{filename}.bmp")),
            moosicbox_audiotags::MimeType::Gif => save_path.join(format!("{filename}.gif")),
        };
        save_bytes_to_file(tag_cover.data, &cover_file_path, None)?;
        return Ok(Some(cover_file_path));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_sanitize_filename_removes_special_characters() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
        assert_eq!(sanitize_filename("test@file#123"), "test_file_123");
        assert_eq!(sanitize_filename("my-track.mp3"), "my_track_mp3");
    }

    #[test_log::test]
    fn test_sanitize_filename_preserves_alphanumeric_and_underscores() {
        assert_eq!(sanitize_filename("Track_123"), "Track_123");
        assert_eq!(sanitize_filename("Album2024"), "Album2024");
        assert_eq!(sanitize_filename("a_b_c_123"), "a_b_c_123");
    }

    #[test_log::test]
    fn test_sanitize_filename_handles_empty_string() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test_log::test]
    fn test_sanitize_filename_handles_unicode_characters() {
        assert_eq!(sanitize_filename("café"), "caf_");
        assert_eq!(sanitize_filename("日本語"), "___");
        assert_eq!(sanitize_filename("Ñoño"), "_o_o");
    }

    #[test_log::test]
    fn test_sanitize_filename_multiple_special_chars() {
        assert_eq!(sanitize_filename("!!!???"), "______");
        assert_eq!(sanitize_filename("a!!!b???c"), "a___b___c");
    }
}
