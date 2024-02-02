#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use audiotags::AudioTag;
use once_cell::sync::Lazy;
use thiserror::Error;

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

fn save_bytes_to_file(bytes: &[u8], path: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    file.write_all(bytes)
}

#[derive(Debug, Error)]
pub enum FetchAndSaveBytesFromRemoteUrlError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub async fn fetch_and_save_bytes_from_remote_url(
    client: &reqwest::Client,
    file_path: &Path,
    url: &str,
) -> Result<PathBuf, FetchAndSaveBytesFromRemoteUrlError> {
    let bytes = client.get(url).send().await?.bytes().await?;
    save_bytes_to_file(&bytes, file_path)?;
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
