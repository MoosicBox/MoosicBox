#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    io::{Seek, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, LazyLock, atomic::AtomicUsize},
    time::Instant,
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

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "files")]
pub mod files;

#[cfg(feature = "range")]
pub mod range;

static NON_ALPHA_NUMERIC_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

pub fn sanitize_filename(string: &str) -> String {
    NON_ALPHA_NUMERIC_REGEX.replace_all(string, "_").to_string()
}

#[derive(Debug, Error)]
pub enum GetContentLengthError {
    #[error(transparent)]
    Http(#[from] moosicbox_http::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

static CLIENT: LazyLock<moosicbox_http::Client> =
    LazyLock::new(|| moosicbox_http::Client::builder().build().unwrap());

/// # Errors
///
/// * If the request fails
/// * If the content-length value is not a valid `u64`
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
            moosicbox_http::Header::Range.as_ref(),
            &format!("bytes={start}-{end}"),
        );
    }

    let mut res = client.send().await?;

    Ok(
        if let Some(header) = res
            .headers()
            .get(moosicbox_http::Header::ContentLength.as_ref())
        {
            Some(header.parse::<u64>()?)
        } else {
            None
        },
    )
}

/// # Panics
///
/// * If the path has no parent directory
///
/// # Errors
///
/// * If there is an IO error
pub fn save_bytes_to_file(
    bytes: &[u8],
    path: &Path,
    start: Option<u64>,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = std::fs::OpenOptions::new()
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

#[derive(Debug, Error)]
pub enum SaveBytesStreamToFileError {
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    #[error("IO Error after read {bytes_read} bytes: {source:?}")]
    Read {
        bytes_read: u64,
        #[source]
        source: tokio::io::Error,
    },
    #[error("IO Error after reading {bytes_read} bytes: {source:?}")]
    Write {
        bytes_read: u64,
        #[source]
        source: tokio::io::Error,
    },
}

/// # Errors
///
/// * If there is an IO error
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

/// # Errors
///
/// * If there is an IO error
pub async fn save_bytes_stream_to_file_with_speed_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    on_speed: OnSpeed,
    on_progress: Option<OnProgress>,
) -> Result<(), SaveBytesStreamToFileError> {
    let last_instant = Arc::new(tokio::sync::Mutex::new(Instant::now()));
    let bytes_since_last_interval = Arc::new(AtomicUsize::new(0));
    let speed = Arc::new(AtomicF64::new(0.0));

    let has_on_progress = on_progress.is_some();
    let on_progress =
        Arc::new(tokio::sync::Mutex::new(on_progress.unwrap_or_else(|| {
            Box::new(|_, _| Box::pin(async move {}) as OnProgressFut)
        })));
    let on_speed = Arc::new(tokio::sync::Mutex::new(on_speed));

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
                    let now = Instant::now();
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

/// # Panics
///
/// * If the path has no parent directory
///
/// # Errors
///
/// * If there is an IO error
pub async fn save_bytes_stream_to_file_with_progress_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    on_progress: Option<OnProgress>,
) -> Result<(), SaveBytesStreamToFileError> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = tokio::fs::OpenOptions::new()
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

        log::trace!("Writing bytes to {path:?}: {len} ({read} total)");

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

#[derive(Debug, Error)]
pub enum FetchCoverError {
    #[error(transparent)]
    Http(#[from] moosicbox_http::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

#[cfg(feature = "files")]
pub(crate) type BytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

#[cfg(feature = "files")]
pub struct CoverBytes {
    pub stream: moosicbox_stream_utils::stalled_monitor::StalledReadMonitor<
        Result<Bytes, std::io::Error>,
        BytesStream,
    >,
    pub size: Option<u64>,
}

#[cfg(feature = "files")]
async fn get_or_fetch_cover_bytes_from_remote_url(
    url: &str,
    file_path: &Path,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, FetchCoverError> {
    use tokio_util::codec::{BytesCodec, FramedRead};

    static IMAGE_CLIENT: LazyLock<moosicbox_http::Client> =
        LazyLock::new(moosicbox_http::Client::new);

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
            fetch_bytes_from_remote_url(&IMAGE_CLIENT, url).await?,
        ),
        size,
    })
}

#[cfg(feature = "files")]
async fn get_or_fetch_cover_from_remote_url(
    url: &str,
    file_path: &Path,
) -> Result<String, FetchCoverError> {
    use std::sync::LazyLock;

    static IMAGE_CLIENT: LazyLock<moosicbox_http::Client> =
        LazyLock::new(moosicbox_http::Client::new);

    if Path::exists(file_path) {
        Ok(file_path.to_str().unwrap().to_string())
    } else {
        Ok(
            fetch_and_save_bytes_from_remote_url(&IMAGE_CLIENT, file_path, url)
                .await?
                .to_str()
                .unwrap()
                .to_string(),
        )
    }
}

#[derive(Debug, Error)]
pub enum FetchAndSaveBytesFromRemoteUrlError {
    #[error(transparent)]
    Http(#[from] moosicbox_http::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error("Request failed: (error {status})")]
    RequestFailed { status: u16, message: String },
}

/// # Errors
///
/// * If the request fails
/// * If there is an IO error
pub async fn fetch_bytes_from_remote_url(
    client: &moosicbox_http::Client,
    url: &str,
) -> Result<
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    FetchAndSaveBytesFromRemoteUrlError,
> {
    log::debug!("Fetching bytes from remote url: {url}");
    let response = client.get(url).send().await?;

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

/// # Errors
///
/// * If the request fails
/// * If there is an IO error
pub async fn fetch_and_save_bytes_from_remote_url(
    client: &moosicbox_http::Client,
    file_path: &Path,
    url: &str,
) -> Result<PathBuf, FetchAndSaveBytesFromRemoteUrlError> {
    log::debug!("Saving bytes to file: {file_path:?}");
    let stream = fetch_bytes_from_remote_url(client, url).await?;
    save_bytes_stream_to_file(stream, file_path, None).await?;
    Ok(file_path.to_path_buf())
}

/// # Errors
///
/// * If the request fails
/// * If there is an IO error
pub async fn search_for_cover(
    path: PathBuf,
    filename: &str,
    save_path: Option<PathBuf>,
    tag: Option<Box<dyn AudioTag + Send + Sync>>,
) -> Result<Option<PathBuf>, std::io::Error> {
    log::trace!("Searching for cover {path:?}");
    if let Ok(mut cover_dir) = tokio::fs::read_dir(path.clone()).await {
        while let Ok(Some(p)) = cover_dir.next_entry().await {
            if p.file_name().to_str().is_some_and(|name| {
                name.to_lowercase()
                    .starts_with(format!("{filename}.").as_str())
            }) {
                return Ok(Some(p.path()));
            }
        }
    }
    if let Some(save_path) = save_path {
        if let Some(tag) = tag {
            if let Some(tag_cover) = tag.album_cover() {
                let cover_file_path = match tag_cover.mime_type {
                    moosicbox_audiotags::MimeType::Png => save_path.join(format!("{filename}.png")),
                    moosicbox_audiotags::MimeType::Jpeg => {
                        save_path.join(format!("{filename}.jpg"))
                    }
                    moosicbox_audiotags::MimeType::Tiff => {
                        save_path.join(format!("{filename}.tiff"))
                    }
                    moosicbox_audiotags::MimeType::Bmp => save_path.join(format!("{filename}.bmp")),
                    moosicbox_audiotags::MimeType::Gif => save_path.join(format!("{filename}.gif")),
                };
                save_bytes_to_file(tag_cover.data, &cover_file_path, None)?;
                return Ok(Some(cover_file_path));
            }
        }
    }

    Ok(None)
}
