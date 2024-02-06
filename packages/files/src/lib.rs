#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
};

use audiotags::AudioTag;
use bytes::{Bytes, BytesMut};
use futures::{StreamExt, TryFutureExt, TryStreamExt};
use futures_core::Stream;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::{
    io::{AsyncWriteExt, BufWriter},
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

pub fn save_bytes_to_file(bytes: &[u8], path: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    file.write_all(bytes)
}

pub async fn save_bytes_stream_to_file<S: Stream<Item = Result<Bytes, std::io::Error>>>(
    stream: S,
    path: &Path,
) -> Result<(), tokio::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .await?;

    let mut writer = BufWriter::new(file);

    pin!(stream);

    while let Some(bytes) = stream.next().await {
        writer.write(&bytes?).await?;
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
    FetchAndSaveBytesFromRemoteUrl(#[from] FetchAndSaveBytesFromRemoteUrlError),
}

pub(crate) type BytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

async fn get_or_fetch_cover_bytes_from_remote_url(
    url: &str,
    file_path: &Path,
) -> Result<BytesStream, FetchCoverError> {
    static IMAGE_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

    if Path::exists(file_path) {
        Ok(tokio::fs::File::open(file_path.to_path_buf())
            .map_ok(|file| FramedRead::new(file, BytesCodec::new()).map_ok(BytesMut::freeze))
            .try_flatten_stream()
            .boxed())
    } else {
        Ok(fetch_bytes_from_remote_url(&IMAGE_CLIENT, url).await?)
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
    save_bytes_stream_to_file(stream, file_path).await?;
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
                save_bytes_to_file(tag_cover.data, &cover_file_path)?;
                return Ok(Some(cover_file_path));
            }
        }
    }

    Ok(None)
}
