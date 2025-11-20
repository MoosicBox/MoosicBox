//! Database integration and model conversion utilities.
//!
//! This module provides traits and functions for converting music models
//! (artists, albums, tracks) into search index data values and delete terms.

use std::path::Path;

use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_models::{Album, Artist, Track};
use thiserror::Error;
use tokio::task::JoinError;

use crate::{
    DataValue, GLOBAL_SEARCH_INDEX_PATH, PopulateIndexError, RecreateIndexError, SEMAPHORE,
};

/// Trait for converting music models into search index data values.
pub trait AsDataValues {
    /// Converts the implementing type into a vector of field-value pairs for indexing.
    #[must_use]
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)>;
}

impl AsDataValues for Artist {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String(self.title.clone())),
            ("artist_id", DataValue::String(self.id.to_string())),
            ("album_title", DataValue::String(String::new())),
            ("track_title", DataValue::String(String::new())),
            (
                "cover",
                DataValue::String(self.cover.clone().unwrap_or_default()),
            ),
            ("blur", DataValue::Bool(false)),
            ("date_released", DataValue::String(String::new())),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String(String::new())),
            ("version_sources", DataValue::String(String::new())),
        ]
    }
}

impl AsDataValues for Album {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        let mut data = vec![
            ("document_type", DataValue::String("albums".into())),
            ("artist_title", DataValue::String(self.artist.clone())),
            ("artist_id", DataValue::String(self.artist_id.to_string())),
            ("album_title", DataValue::String(self.title.clone())),
            ("album_id", DataValue::String(self.id.to_string())),
            ("track_title", DataValue::String(String::new())),
            (
                "cover",
                DataValue::String(self.artwork.clone().unwrap_or_default()),
            ),
            ("blur", DataValue::Bool(self.blur)),
            (
                "date_released",
                DataValue::String(
                    self.date_released
                        .map(|x| x.and_utc().to_rfc3339())
                        .unwrap_or_default(),
                ),
            ),
            (
                "date_added",
                DataValue::String(
                    self.date_added
                        .map(|x| x.and_utc().to_rfc3339())
                        .unwrap_or_default(),
                ),
            ),
        ];

        for version in &self.versions {
            data.extend_from_slice(&[
                (
                    "version_formats",
                    DataValue::String(
                        version
                            .format
                            .map_or_else(String::new, |a| a.as_ref().to_string()),
                    ),
                ),
                (
                    "version_bit_depths",
                    DataValue::Number(u64::from(version.bit_depth.unwrap_or_default())),
                ),
                (
                    "version_sample_rates",
                    DataValue::Number(u64::from(version.sample_rate.unwrap_or_default())),
                ),
                (
                    "version_channels",
                    DataValue::Number(u64::from(version.sample_rate.unwrap_or_default())),
                ),
                (
                    "version_sources",
                    DataValue::String(version.source.to_string()),
                ),
            ]);
        }

        data
    }
}

impl AsDataValues for Track {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        vec![
            ("document_type", DataValue::String("tracks".into())),
            ("artist_title", DataValue::String(self.artist.clone())),
            ("artist_id", DataValue::String(self.artist_id.to_string())),
            ("album_title", DataValue::String(self.album.clone())),
            ("album_id", DataValue::String(self.album_id.to_string())),
            ("track_title", DataValue::String(self.title.clone())),
            ("track_id", DataValue::String(self.id.to_string())),
            (
                "cover",
                DataValue::String(self.artwork.clone().unwrap_or_default()),
            ),
            ("blur", DataValue::Bool(self.blur)),
            (
                "date_released",
                DataValue::String(self.date_released.clone().unwrap_or_default()),
            ),
            (
                "date_added",
                DataValue::String(self.date_added.clone().unwrap_or_default()),
            ),
            (
                "version_formats",
                DataValue::String(
                    self.format
                        .map_or_else(String::new, |a| a.as_ref().to_string()),
                ),
            ),
            (
                "version_bit_depths",
                DataValue::Number(u64::from(self.bit_depth.unwrap_or_default())),
            ),
            (
                "version_sample_rates",
                DataValue::Number(u64::from(self.sample_rate.unwrap_or_default())),
            ),
            (
                "version_channels",
                DataValue::Number(u64::from(self.sample_rate.unwrap_or_default())),
            ),
            (
                "version_sources",
                DataValue::String(self.track_source.to_string()),
            ),
        ]
    }
}

/// Trait for converting music models into delete terms for removing from the search index.
pub trait AsDeleteTerm {
    /// Converts the implementing type into a field-value pair for deletion.
    #[must_use]
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue);
}

impl AsDeleteTerm for Artist {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("artist_id_string", DataValue::String(self.id.to_string()))
    }
}

impl AsDeleteTerm for Album {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("album_id_string", DataValue::String(self.id.to_string()))
    }
}

impl AsDeleteTerm for Track {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("track_id_string", DataValue::String(self.id.to_string()))
    }
}

/// Error type for failures when reindexing from a database.
#[derive(Debug, Error)]
pub enum ReindexFromDbError {
    /// Failed to fetch data from the database
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to recreate the search index
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    /// Failed to populate the index with data
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    /// The asynchronous task failed to complete
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Recreates the global search index from scratch.
///
/// This function deletes the existing index and creates a new empty index. This operation
/// is performed asynchronously in a blocking task.
///
/// # Errors
///
/// * `RecreateIndexError::CreateIndex` if failed to create the new index
/// * `RecreateIndexError::GetIndexReader` if failed to get the index reader
/// * `RecreateIndexError::Join` if the tokio task failed to join
pub async fn recreate_global_search_index() -> Result<(), RecreateIndexError> {
    let permit = SEMAPHORE.acquire().await;
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("recreate_global_search_index", || {
            let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
            crate::recreate_global_search_index_sync(path)
        })
        .await??;
    drop(permit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::{
        AlbumSource, AlbumType, AlbumVersionQuality, ApiSource, ApiSources, TrackApiSource, id::Id,
    };
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_artist_as_data_values() {
        let artist = Artist {
            id: Id::Number(123),
            title: "Test Artist".to_string(),
            cover: Some("cover.jpg".to_string()),
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        };

        let data = artist.as_data_values();

        assert_eq!(data.len(), 11);
        assert!(data.contains(&("document_type", DataValue::String("artists".into()))));
        assert!(data.contains(&("artist_title", DataValue::String("Test Artist".into()))));
        assert!(data.contains(&("artist_id", DataValue::String("123".into()))));
        assert!(data.contains(&("album_title", DataValue::String(String::new()))));
        assert!(data.contains(&("track_title", DataValue::String(String::new()))));
        assert!(data.contains(&("cover", DataValue::String("cover.jpg".into()))));
        assert!(data.contains(&("blur", DataValue::Bool(false))));
    }

    #[test_log::test]
    fn test_artist_as_data_values_no_cover() {
        let artist = Artist {
            id: Id::Number(456),
            title: "No Cover Artist".to_string(),
            cover: None,
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        };

        let data = artist.as_data_values();

        assert!(data.contains(&("cover", DataValue::String(String::new()))));
    }

    #[test_log::test]
    fn test_album_as_data_values() {
        let album = Album {
            id: Id::Number(789),
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(123),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: Some("album.jpg".to_string()),
            directory: None,
            blur: true,
            versions: vec![AlbumVersionQuality {
                format: None,
                bit_depth: Some(24),
                sample_rate: Some(96000),
                channels: Some(2),
                source: TrackApiSource::Local,
            }],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        };

        let data = album.as_data_values();

        assert!(data.contains(&("document_type", DataValue::String("albums".into()))));
        assert!(data.contains(&("artist_title", DataValue::String("Test Artist".into()))));
        assert!(data.contains(&("artist_id", DataValue::String("123".into()))));
        assert!(data.contains(&("album_title", DataValue::String("Test Album".into()))));
        assert!(data.contains(&("album_id", DataValue::String("789".into()))));
        assert!(data.contains(&("track_title", DataValue::String(String::new()))));
        assert!(data.contains(&("cover", DataValue::String("album.jpg".into()))));
        assert!(data.contains(&("blur", DataValue::Bool(true))));
        assert!(data.contains(&("version_bit_depths", DataValue::Number(24))));
        assert!(data.contains(&("version_sample_rates", DataValue::Number(96000))));
    }

    #[test_log::test]
    fn test_album_as_data_values_no_versions() {
        let album = Album {
            id: Id::Number(111),
            title: "Empty Album".to_string(),
            artist: "Artist".to_string(),
            artist_id: Id::Number(222),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: None,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        };

        let data = album.as_data_values();

        // Should still have base fields but no version fields
        assert!(data.contains(&("document_type", DataValue::String("albums".into()))));
        assert!(data.contains(&("album_title", DataValue::String("Empty Album".into()))));
        // No version fields should be present
        assert!(!data.iter().any(|(key, _)| *key == "version_formats" && matches!(data.iter().find(|(k, _)| *k == "version_formats").map(|(_, v)| v), Some(DataValue::String(s)) if !s.is_empty())));
    }

    #[test_log::test]
    fn test_track_as_data_values() {
        let track = Track {
            id: Id::Number(999),
            number: 1,
            title: "Test Track".to_string(),
            duration: 180.0,
            album: "Test Album".to_string(),
            album_id: Id::Number(789),
            album_type: AlbumType::Lp,
            date_released: Some("2024-01-01".to_string()),
            date_added: Some("2024-01-15".to_string()),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(123),
            file: None,
            artwork: Some("track.jpg".to_string()),
            blur: false,
            bytes: 1024,
            format: None,
            bit_depth: Some(16),
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: Some(44100),
            channels: Some(2),
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
        };

        let data = track.as_data_values();

        assert!(data.contains(&("document_type", DataValue::String("tracks".into()))));
        assert!(data.contains(&("artist_title", DataValue::String("Test Artist".into()))));
        assert!(data.contains(&("artist_id", DataValue::String("123".into()))));
        assert!(data.contains(&("album_title", DataValue::String("Test Album".into()))));
        assert!(data.contains(&("album_id", DataValue::String("789".into()))));
        assert!(data.contains(&("track_title", DataValue::String("Test Track".into()))));
        assert!(data.contains(&("track_id", DataValue::String("999".into()))));
        assert!(data.contains(&("cover", DataValue::String("track.jpg".into()))));
        assert!(data.contains(&("blur", DataValue::Bool(false))));
        assert!(data.contains(&("date_released", DataValue::String("2024-01-01".into()))));
        assert!(data.contains(&("date_added", DataValue::String("2024-01-15".into()))));
        assert!(data.contains(&("version_bit_depths", DataValue::Number(16))));
        assert!(data.contains(&("version_sample_rates", DataValue::Number(44100))));
    }

    #[test_log::test]
    fn test_track_as_data_values_minimal() {
        let track = Track {
            id: Id::Number(888),
            number: 1,
            title: "Minimal Track".to_string(),
            duration: 120.0,
            album: "Album".to_string(),
            album_id: Id::Number(111),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artist: "Artist".to_string(),
            artist_id: Id::Number(222),
            file: None,
            artwork: None,
            blur: true,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
        };

        let data = track.as_data_values();

        assert!(data.contains(&("track_title", DataValue::String("Minimal Track".into()))));
        assert!(data.contains(&("cover", DataValue::String(String::new()))));
        assert!(data.contains(&("blur", DataValue::Bool(true))));
        assert!(data.contains(&("date_released", DataValue::String(String::new()))));
        assert!(data.contains(&("date_added", DataValue::String(String::new()))));
        assert!(data.contains(&("version_formats", DataValue::String(String::new()))));
        assert!(data.contains(&("version_bit_depths", DataValue::Number(0))));
        assert!(data.contains(&("version_sample_rates", DataValue::Number(0))));
    }

    #[test_log::test]
    fn test_artist_as_delete_term() {
        let artist = Artist {
            id: Id::Number(555),
            title: "Delete Artist".to_string(),
            cover: None,
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        };

        let term = artist.as_delete_term();

        assert_eq!(term.0, "artist_id_string");
        assert!(matches!(term.1, DataValue::String(ref s) if s == "555"));
    }

    #[test_log::test]
    fn test_album_as_delete_term() {
        let album = Album {
            id: Id::Number(666),
            title: "Delete Album".to_string(),
            artist: "Artist".to_string(),
            artist_id: Id::Number(555),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: None,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        };

        let term = album.as_delete_term();

        assert_eq!(term.0, "album_id_string");
        assert!(matches!(term.1, DataValue::String(ref s) if s == "666"));
    }

    #[test_log::test]
    fn test_track_as_delete_term() {
        let track = Track {
            id: Id::Number(777),
            number: 1,
            title: "Delete Track".to_string(),
            duration: 100.0,
            album: "Album".to_string(),
            album_id: Id::Number(666),
            album_type: AlbumType::Lp,
            date_released: None,
            date_added: None,
            artist: "Artist".to_string(),
            artist_id: Id::Number(555),
            file: None,
            artwork: None,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
        };

        let term = track.as_delete_term();

        assert_eq!(term.0, "track_id_string");
        assert!(matches!(term.1, DataValue::String(ref s) if s == "777"));
    }
}
