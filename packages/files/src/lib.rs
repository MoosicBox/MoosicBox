#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    fs,
    io::{Seek, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::atomic::AtomicUsize,
    time::Instant,
};

use atomic_float::AtomicF64;
use audiotags::AudioTag;
use bytes::{Bytes, BytesMut};
use futures::{StreamExt, TryStreamExt};
use futures_core::Stream;
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    pin,
};
use tokio_util::codec::{BytesCodec, FramedRead};

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "files")]
pub mod files;

#[cfg(feature = "range")]
pub mod range;

static NON_ALPHA_NUMERIC_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

pub fn sanitize_filename(string: &str) -> String {
    NON_ALPHA_NUMERIC_REGEX.replace_all(string, "_").to_string()
}

#[derive(Debug, Error)]
pub enum GetContentLengthError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ToStr(#[from] reqwest::header::ToStrError),
}

pub async fn get_content_length(
    url: &str,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<Option<u64>, GetContentLengthError> {
    let mut client = reqwest::Client::new().head(url);

    if start.is_some() || end.is_some() {
        let start = start.map(|x| x.to_string()).unwrap_or("".into());
        let end = end.map(|x| x.to_string()).unwrap_or("".into());

        client = client.header(
            actix_web::http::header::RANGE,
            format!("bytes={start}-{end}"),
        );
    }

    let res = client.send().await.unwrap();

    Ok(
        if let Some(header) = res.headers().get(actix_web::http::header::CONTENT_LENGTH) {
            Some(header.to_str()?.parse::<u64>()?)
        } else {
            None
        },
    )
}

pub fn save_bytes_to_file(
    bytes: &[u8],
    path: &Path,
    start: Option<u64>,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!start.is_some_and(|start| start > 0))
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

pub async fn save_bytes_stream_to_file<S: Stream<Item = Result<Bytes, std::io::Error>>>(
    stream: S,
    path: &Path,
    start: Option<u64>,
) -> Result<(), SaveBytesStreamToFileError> {
    save_bytes_stream_to_file_with_progress_listener(stream, path, start, None).await
}

pub async fn save_bytes_stream_to_file_with_speed_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>>,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    mut on_speed: Box<dyn FnMut(f64) + Send>,
    on_progress: Option<Box<dyn FnMut(usize, usize) + Send>>,
) -> Result<(), SaveBytesStreamToFileError> {
    let last_instant = std::sync::Arc::new(std::sync::Mutex::new(Instant::now()));
    let bytes_since_last_interval = AtomicUsize::new(0);
    let speed = AtomicF64::new(0.0);

    let has_on_progress = on_progress.is_some();
    let mut on_progress = if let Some(on_progress) = on_progress {
        on_progress
    } else {
        Box::new(|_, _| {})
    };

    let result = save_bytes_stream_to_file_with_progress_listener(
        stream,
        path,
        start,
        Some(Box::new(move |read, total| {
            if has_on_progress {
                on_progress(read, total);
            }

            let mut last_instant = last_instant.lock().unwrap();
            let bytes = bytes_since_last_interval
                .fetch_add(read, std::sync::atomic::Ordering::SeqCst)
                + read;
            let now = Instant::now();
            let millis = now.duration_since(*last_instant).as_millis();

            if millis >= 1000 {
                let speed_millis = (bytes as f64) * (millis as f64 / 1000.0);
                speed.store(speed_millis, std::sync::atomic::Ordering::SeqCst);
                log::debug!(
                    "Speed: {speed_millis} b/s {} KiB/s {} MiB/s",
                    speed_millis / 1024.0,
                    speed_millis / 1024.0 / 1024.0,
                );
                on_speed(speed_millis);
                *last_instant = now;
                bytes_since_last_interval.store(0, std::sync::atomic::Ordering::SeqCst)
            }
        })),
    )
    .await;

    result
}

pub async fn save_bytes_stream_to_file_with_progress_listener<
    S: Stream<Item = Result<Bytes, std::io::Error>>,
>(
    stream: S,
    path: &Path,
    start: Option<u64>,
    on_progress: Option<Box<dyn FnMut(usize, usize) + Send>>,
) -> Result<(), SaveBytesStreamToFileError> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!start.is_some_and(|start| start > 0))
        .open(path)
        .await?;

    let mut writer = BufWriter::new(file);

    if let Some(start) = start {
        writer.seek(std::io::SeekFrom::Start(start)).await?;
    }

    pin!(stream);

    let mut read = start.unwrap_or(0) as usize;

    let has_on_progress = on_progress.is_some();
    let mut on_progress = if let Some(on_progress) = on_progress {
        on_progress
    } else {
        Box::new(|_, _| {})
    };

    while let Some(bytes) = stream.next().await {
        let bytes = bytes.map_err(|err| SaveBytesStreamToFileError::Read {
            bytes_read: read as u64,
            source: err,
        })?;

        let len = bytes.len();

        read += len;

        log::debug!("Writing bytes to {path:?}: {len} ({read} total)");

        writer
            .write(&bytes)
            .await
            .map_err(|err| SaveBytesStreamToFileError::Write {
                bytes_read: read as u64,
                source: err,
            })?;

        if has_on_progress {
            on_progress(len, read);
        }
    }

    writer.flush().await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum FetchCoverError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    #[error(transparent)]
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

pub(crate) type BytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

pub struct CoverBytes {
    pub stream: StalledReadMonitor<Bytes, BytesStream>,
    pub size: Option<u64>,
}

async fn get_or_fetch_cover_bytes_from_remote_url(
    url: &str,
    file_path: &Path,
) -> Result<CoverBytes, FetchCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    if Path::exists(file_path) {
        let file = tokio::fs::File::open(file_path.to_path_buf()).await?;

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
    } else {
        let size = get_content_length(&url, None, None).await?;

        Ok(CoverBytes {
            stream: StalledReadMonitor::new(fetch_bytes_from_remote_url(&IMAGE_CLIENT, url).await?),
            size,
        })
    }
}

async fn get_or_fetch_cover_from_remote_url(
    url: &str,
    file_path: &Path,
) -> Result<String, FetchCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    if Path::exists(file_path) {
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
pub enum FetchAndSaveBytesFromRemoteUrlError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error("Request failed: (error {status})")]
    RequestFailed { status: u16, message: String },
}

pub async fn fetch_bytes_from_remote_url(
    client: &reqwest::Client,
    url: &str,
) -> Result<
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    FetchAndSaveBytesFromRemoteUrlError,
> {
    log::debug!("Fetching bytes from remote url: {url}");
    let response = client.get(url).send().await?;

    let status = response.status();

    if status != 200 {
        let message = response.text().await.unwrap_or("".to_string());

        log::error!("Request failed: {status} ({message})");
        return Err(FetchAndSaveBytesFromRemoteUrlError::RequestFailed {
            status: status.into(),
            message,
        });
    }

    Ok(response
        .bytes_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
        .boxed())
}

pub async fn fetch_and_save_bytes_from_remote_url(
    client: &reqwest::Client,
    file_path: &Path,
    url: &str,
) -> Result<PathBuf, FetchAndSaveBytesFromRemoteUrlError> {
    log::debug!("Saving bytes to file: {file_path:?}");
    let stream = fetch_bytes_from_remote_url(client, url).await?;
    save_bytes_stream_to_file(stream, file_path, None).await?;
    Ok(file_path.to_path_buf())
}

pub fn search_for_cover(
    path: PathBuf,
    filename: &str,
    save_path: Option<PathBuf>,
    tag: Option<Box<dyn AudioTag + Send + Sync>>,
) -> Result<Option<PathBuf>, std::io::Error> {
    log::trace!("Searching for cover {path:?}");
    if let Some(cover_file) = fs::read_dir(path.clone()).ok().and_then(|path| {
        path.filter_map(|p| p.ok())
            .find(|p| {
                p.file_name().to_str().is_some_and(|name| {
                    name.to_lowercase()
                        .starts_with(format!("{filename}.").as_str())
                })
            })
            .map(|dir| dir.path())
    }) {
        return Ok(Some(cover_file));
    } else if let Some(save_path) = save_path {
        if let Some(tag) = tag {
            if let Some(tag_cover) = tag.album_cover() {
                let cover_file_path = match tag_cover.mime_type {
                    audiotags::MimeType::Png => save_path.join(format!("{filename}.png")),
                    audiotags::MimeType::Jpeg => save_path.join(format!("{filename}.jpg")),
                    audiotags::MimeType::Tiff => save_path.join(format!("{filename}.tiff")),
                    audiotags::MimeType::Bmp => save_path.join(format!("{filename}.bmp")),
                    audiotags::MimeType::Gif => save_path.join(format!("{filename}.gif")),
                };
                save_bytes_to_file(tag_cover.data, &cover_file_path, None)?;
                return Ok(Some(cover_file_path));
            }
        }
    }

    Ok(None)
}
