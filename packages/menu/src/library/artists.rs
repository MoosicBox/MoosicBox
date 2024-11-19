#![allow(clippy::module_name_repetitions)]

use moosicbox_core::sqlite::{
    db::DbError,
    models::{AlbumSource, ArtistSort},
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_library::{db::get_artists, models::LibraryArtist};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<ArtistSort>,
    pub filters: ArtistFilters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistFilters {
    pub name: Option<String>,
    pub search: Option<String>,
}

#[must_use]
pub fn filter_artists(artists: Vec<LibraryArtist>, request: &ArtistsRequest) -> Vec<LibraryArtist> {
    artists
        .into_iter()
        .filter(|artist| {
            !request
                .filters
                .name
                .as_ref()
                .is_some_and(|s| !artist.title.to_lowercase().contains(s))
        })
        .filter(|artist| {
            !request.filters.search.as_ref().is_some_and(|s| {
                !(artist.title.to_lowercase().contains(s)
                    || artist.title.to_lowercase().contains(s))
            })
        })
        .collect()
}

#[must_use]
pub fn sort_artists(
    mut artists: Vec<LibraryArtist>,
    request: &ArtistsRequest,
) -> Vec<LibraryArtist> {
    match request.sort {
        Some(ArtistSort::NameAsc) => artists.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(ArtistSort::NameDesc) => artists.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(ArtistSort::NameAsc) | None => {
            artists.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        Some(ArtistSort::NameDesc) => {
            artists.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()));
        }
    }

    artists
}

#[derive(Debug, Error)]
pub enum GetArtistsError {
    #[error(transparent)]
    DbError(#[from] DbError),
}

/// # Errors
///
/// * If failed to get the artists from the database
pub async fn get_all_artists(
    db: &LibraryDatabase,
    request: &ArtistsRequest,
) -> Result<Vec<LibraryArtist>, GetArtistsError> {
    let artists = get_artists(db).await?;

    Ok(sort_artists(filter_artists(artists, request), request))
}
