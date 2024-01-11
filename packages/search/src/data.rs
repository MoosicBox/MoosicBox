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
            ]
        })
        .collect::<Vec<_>>();

    crate::populate_global_search_index(artists, false)?;

    let albums = moosicbox_core::sqlite::db::get_albums(&connection.inner)?
        .iter()
        .map(|album| {
            vec![
                ("document_type", DataValue::String("albums".into())),
                ("artist_title", DataValue::String(album.artist.clone())),
                ("artist_id", DataValue::Number(album.artist_id as u64)),
                ("album_title", DataValue::String(album.title.clone())),
                ("album_id", DataValue::Number(album.id as u64)),
                ("track_title", DataValue::String("".into())),
            ]
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
            ]
        })
        .collect::<Vec<_>>();

    crate::populate_global_search_index(tracks, false)?;

    Ok(())
}
