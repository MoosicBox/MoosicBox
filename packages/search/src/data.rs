use moosicbox_core::{app::DbConnection, sqlite::db::DbError};
use thiserror::Error;

use crate::{DataValue, PopulateIndexError, RecreateIndexError};

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
    crate::recreate_global_search_index()?;

    let artists = moosicbox_core::sqlite::db::get_artists(&connection.inner)?
        .iter()
        .map(|artist| {
            vec![
                ("document_type", DataValue::String("artists".into())),
                ("artist_title", DataValue::String(artist.title.clone())),
                ("artist_id", DataValue::Number(artist.id as u64)),
                ("album_title", DataValue::String("".into())),
                ("track_title", DataValue::String("".into())),
                (
                    "cover",
                    DataValue::String(artist.cover.clone().unwrap_or("".to_string())),
                ),
                ("blur", DataValue::Bool(false)),
                ("date_released", DataValue::String("".into())),
                ("date_added", DataValue::String("".into())),
                ("version_formats", DataValue::String("".into())),
                ("version_sources", DataValue::String("".into())),
            ]
        })
        .collect::<Vec<_>>();

    crate::populate_global_search_index(artists, false)?;

    let albums = moosicbox_core::sqlite::db::get_albums(&connection.inner)?
        .iter()
        .map(|album| {
            let mut data = vec![
                ("document_type", DataValue::String("albums".into())),
                ("artist_title", DataValue::String(album.artist.clone())),
                ("artist_id", DataValue::Number(album.artist_id as u64)),
                ("album_title", DataValue::String(album.title.clone())),
                ("album_id", DataValue::Number(album.id as u64)),
                ("track_title", DataValue::String("".into())),
                (
                    "cover",
                    DataValue::String(album.artwork.clone().unwrap_or("".to_string())),
                ),
                ("blur", DataValue::Bool(album.blur)),
                (
                    "date_released",
                    DataValue::String(album.date_released.clone().unwrap_or("".to_string())),
                ),
                (
                    "date_added",
                    DataValue::String(album.date_added.clone().unwrap_or("".to_string())),
                ),
            ];

            for version in &album.versions {
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
        })
        .collect::<Vec<_>>();

    crate::populate_global_search_index(albums, false)?;

    let tracks = moosicbox_core::sqlite::db::get_tracks(&connection.inner, None)?
        .iter()
        .map(|track| {
            vec![
                ("document_type", DataValue::String("tracks".into())),
                ("artist_title", DataValue::String(track.artist.clone())),
                ("artist_id", DataValue::Number(track.artist_id as u64)),
                ("album_title", DataValue::String(track.album.clone())),
                ("album_id", DataValue::Number(track.album_id as u64)),
                ("track_title", DataValue::String(track.title.clone())),
                ("track_id", DataValue::Number(track.id as u64)),
                (
                    "cover",
                    DataValue::String(track.artwork.clone().unwrap_or("".to_string())),
                ),
                ("blur", DataValue::Bool(track.blur)),
                (
                    "date_released",
                    DataValue::String(track.date_released.clone().unwrap_or("".to_string())),
                ),
                (
                    "date_added",
                    DataValue::String(track.date_added.clone().unwrap_or("".to_string())),
                ),
                (
                    "version_formats",
                    DataValue::String(
                        track
                            .format
                            .map(|a| a.as_ref().to_string())
                            .unwrap_or("".to_string()),
                    ),
                ),
                (
                    "version_bit_depths",
                    DataValue::Number(track.bit_depth.unwrap_or_default() as u64),
                ),
                (
                    "version_sample_rates",
                    DataValue::Number(track.sample_rate.unwrap_or_default() as u64),
                ),
                (
                    "version_channels",
                    DataValue::Number(track.sample_rate.unwrap_or_default() as u64),
                ),
                (
                    "version_sources",
                    DataValue::String(track.source.as_ref().to_string()),
                ),
            ]
        })
        .collect::<Vec<_>>();

    crate::populate_global_search_index(tracks, false)?;

    Ok(())
}
