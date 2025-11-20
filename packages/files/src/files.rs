//! Core file handling operations for tracks, albums, and artists.
//!
//! This module provides functionality for managing track files, cover images, and audio
//! visualization data. It includes submodules for album covers, artist covers, track
//! streaming, and track pooling/caching.

use std::str::FromStr as _;

/// Album cover image fetching and caching.
pub mod album;
/// Artist cover image fetching and caching.
pub mod artist;
/// Track audio streaming, metadata, and visualization.
pub mod track;

mod track_bytes_media_source;
/// Track byte stream pooling and caching service.
pub mod track_pool;

/// Extracts the filename from a file path string.
///
/// Returns just the filename component (without directory path) if the path is valid.
pub(crate) fn filename_from_path_str(path: &str) -> Option<String> {
    std::path::PathBuf::from_str(path).ok().and_then(|p| {
        p.file_name()
            .and_then(|x| x.to_str().map(ToString::to_string))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_from_path_str_unix_path() {
        assert_eq!(
            filename_from_path_str("/path/to/file.mp3"),
            Some("file.mp3".to_string())
        );
        assert_eq!(
            filename_from_path_str("/home/user/music/track.flac"),
            Some("track.flac".to_string())
        );
    }

    #[test]
    fn test_filename_from_path_str_filename_only() {
        assert_eq!(
            filename_from_path_str("track.mp3"),
            Some("track.mp3".to_string())
        );
        assert_eq!(
            filename_from_path_str("album.flac"),
            Some("album.flac".to_string())
        );
    }

    #[test]
    fn test_filename_from_path_str_directory_path() {
        // Directory paths with trailing slash return the last component
        assert_eq!(
            filename_from_path_str("/path/to/directory/"),
            Some("directory".to_string())
        );
    }

    #[test]
    fn test_filename_from_path_str_with_spaces() {
        assert_eq!(
            filename_from_path_str("/music/my track.mp3"),
            Some("my track.mp3".to_string())
        );
    }

    #[test]
    fn test_filename_from_path_str_with_special_chars() {
        assert_eq!(
            filename_from_path_str("/music/track [remastered].mp3"),
            Some("track [remastered].mp3".to_string())
        );
    }

    #[test]
    fn test_filename_from_path_str_empty() {
        assert_eq!(filename_from_path_str(""), None);
    }
}
