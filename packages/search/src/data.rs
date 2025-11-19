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
