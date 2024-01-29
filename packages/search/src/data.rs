use std::path::Path;

use moosicbox_core::{
    app::DbConnection,
    sqlite::{
        db::DbError,
        models::{Album, Artist, LibraryTrack},
    },
};
use thiserror::Error;

use crate::{DataValue, PopulateIndexError, RecreateIndexError, GLOBAL_SEARCH_INDEX_PATH};

pub trait AsDataValues {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)>;
}

impl AsDataValues for Artist {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String(self.title.clone())),
            ("artist_id", DataValue::Number(self.id as u64)),
            ("album_title", DataValue::String("".into())),
            ("track_title", DataValue::String("".into())),
            (
                "cover",
                DataValue::String(self.cover.clone().unwrap_or("".to_string())),
            ),
            ("blur", DataValue::Bool(false)),
            ("date_released", DataValue::String("".into())),
            ("date_added", DataValue::String("".into())),
            ("version_formats", DataValue::String("".into())),
            ("version_sources", DataValue::String("".into())),
        ]
    }
}

impl AsDataValues for Album {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        let mut data = vec![
            ("document_type", DataValue::String("albums".into())),
            ("artist_title", DataValue::String(self.artist.clone())),
            ("artist_id", DataValue::Number(self.artist_id as u64)),
            ("album_title", DataValue::String(self.title.clone())),
            ("album_id", DataValue::Number(self.id as u64)),
            ("track_title", DataValue::String("".into())),
            (
                "cover",
                DataValue::String(self.artwork.clone().unwrap_or("".to_string())),
            ),
            ("blur", DataValue::Bool(self.blur)),
            (
                "date_released",
                DataValue::String(self.date_released.clone().unwrap_or("".to_string())),
            ),
            (
                "date_added",
                DataValue::String(self.date_added.clone().unwrap_or("".to_string())),
            ),
        ];

        for version in &self.versions {
            data.extend_from_slice(&[
                (
                    "version_formats",
                    DataValue::String(
                        version
                            .format
                            .map(|a| a.as_ref().to_string())
                            .unwrap_or("".to_string()),
                    ),
                ),
                (
                    "version_bit_depths",
                    DataValue::Number(version.bit_depth.unwrap_or_default() as u64),
                ),
                (
                    "version_sample_rates",
                    DataValue::Number(version.sample_rate.unwrap_or_default() as u64),
                ),
                (
                    "version_channels",
                    DataValue::Number(version.sample_rate.unwrap_or_default() as u64),
                ),
                (
                    "version_sources",
                    DataValue::String(version.source.as_ref().to_string()),
                ),
            ]);
        }

        data
    }
}

impl AsDataValues for LibraryTrack {
    fn as_data_values<'a>(&self) -> Vec<(&'a str, DataValue)> {
        vec![
            ("document_type", DataValue::String("tracks".into())),
            ("artist_title", DataValue::String(self.artist.clone())),
            ("artist_id", DataValue::Number(self.artist_id as u64)),
            ("album_title", DataValue::String(self.album.clone())),
            ("album_id", DataValue::Number(self.album_id as u64)),
            ("track_title", DataValue::String(self.title.clone())),
            ("track_id", DataValue::Number(self.id as u64)),
            (
                "cover",
                DataValue::String(self.artwork.clone().unwrap_or("".to_string())),
            ),
            ("blur", DataValue::Bool(self.blur)),
            (
                "date_released",
                DataValue::String(self.date_released.clone().unwrap_or("".to_string())),
            ),
            (
                "date_added",
                DataValue::String(self.date_added.clone().unwrap_or("".to_string())),
            ),
            (
                "version_formats",
                DataValue::String(
                    self.format
                        .map(|a| a.as_ref().to_string())
                        .unwrap_or("".to_string()),
                ),
            ),
            (
                "version_bit_depths",
                DataValue::Number(self.bit_depth.unwrap_or_default() as u64),
            ),
            (
                "version_sample_rates",
                DataValue::Number(self.sample_rate.unwrap_or_default() as u64),
            ),
            (
                "version_channels",
                DataValue::Number(self.sample_rate.unwrap_or_default() as u64),
            ),
            (
                "version_sources",
                DataValue::String(self.source.as_ref().to_string()),
            ),
        ]
    }
}

pub trait AsDeleteTerm {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue);
}

impl AsDeleteTerm for Artist {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("artist_id", DataValue::Number(self.id as u64))
    }
}

impl AsDeleteTerm for Album {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("album_id", DataValue::Number(self.id as u64))
    }
}

impl AsDeleteTerm for LibraryTrack {
    fn as_delete_term<'a>(&self) -> (&'a str, DataValue) {
        ("track_id", DataValue::Number(self.id as u64))
    }
}

#[derive(Debug, Error)]
pub enum ReindexFromDbError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
}

pub fn reindex_global_search_index_from_db(
    connection: &DbConnection,
) -> Result<(), ReindexFromDbError> {
    let path: &Path = GLOBAL_SEARCH_INDEX_PATH.as_ref();
    crate::recreate_global_search_index(path)?;

    let artists = moosicbox_core::sqlite::db::get_artists(&connection.inner)?
        .into_iter()
        .map(|artist| artist.as_data_values())
        .collect::<Vec<_>>();

    crate::populate_global_search_index(artists, false)?;

    let albums = moosicbox_core::sqlite::db::get_albums(&connection.inner)?
        .into_iter()
        .map(|album| album.as_data_values())
        .collect::<Vec<_>>();

    crate::populate_global_search_index(albums, false)?;

    let tracks = moosicbox_core::sqlite::db::get_tracks(&connection.inner, None)?
        .into_iter()
        .map(|track| track.as_data_values())
        .collect::<Vec<_>>();

    crate::populate_global_search_index(tracks, false)?;

    Ok(())
}
