//! Artist management operations for the music library.
//!
//! This module provides functionality for querying, filtering, and sorting artists
//! from the music library database.

#![allow(clippy::module_name_repetitions)]

use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{db::get_artists, models::LibraryArtist};
use moosicbox_music_models::{AlbumSource, ArtistSort};
use serde::{Deserialize, Serialize};
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;

/// Request parameters for querying artists from the library.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistsRequest {
    /// Optional list of album sources to filter by
    pub sources: Option<Vec<AlbumSource>>,
    /// Optional sort order for the results
    pub sort: Option<ArtistSort>,
    /// Filters to apply to the artist query
    pub filters: ArtistFilters,
}

/// Filter criteria for artist queries.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistFilters {
    /// Filter by artist name (case-insensitive substring match)
    pub name: Option<String>,
    /// Generic search query (case-insensitive substring match)
    pub search: Option<String>,
}

/// Filters a list of artists based on the provided request criteria.
///
/// Applies name and search filters from the request to the artist list.
#[must_use]
pub fn filter_artists(artists: Vec<LibraryArtist>, request: &ArtistsRequest) -> Vec<LibraryArtist> {
    artists
        .into_iter()
        .filter(|artist| {
            request
                .filters
                .name
                .as_ref()
                .is_none_or(|s| artist.title.to_lowercase().contains(s))
        })
        .filter(|artist| {
            request.filters.search.as_ref().is_none_or(|s| {
                artist.title.to_lowercase().contains(s) || artist.title.to_lowercase().contains(s)
            })
        })
        .collect()
}

/// Sorts a list of artists based on the sort order specified in the request.
///
/// Applies case-insensitive sorting by artist name in ascending or descending order.
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
    DatabaseFetch(#[from] DatabaseFetchError),
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
