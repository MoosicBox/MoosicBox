use std::str::FromStr as _;

pub mod album;
pub mod artist;
pub mod track;

mod track_bytes_media_source;
pub mod track_pool;

pub(crate) fn filename_from_path_str(path: &str) -> Option<String> {
    std::path::PathBuf::from_str(path).ok().and_then(|p| {
        p.file_name()
            .and_then(|x| x.to_str().map(|x| x.to_string()))
    })
}
