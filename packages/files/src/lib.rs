#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    io::Write,
    path::{Path, PathBuf},
};

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
