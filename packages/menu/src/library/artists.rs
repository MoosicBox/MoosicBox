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

/// Error types that can occur when retrieving artists.
#[derive(Debug, Error)]
pub enum GetArtistsError {
    /// Database fetch error
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves all artists from the library with filtering and sorting.
///
/// Fetches all artists from the database, applies the specified filters (name, search),
/// and sorts them according to the requested sort order. This is the main entry point
/// for querying artists from the library.
///
/// # Errors
///
/// * `GetArtistsError::DatabaseFetch` if fetching artists from the database fails
pub async fn get_all_artists(
    db: &LibraryDatabase,
    request: &ArtistsRequest,
) -> Result<Vec<LibraryArtist>, GetArtistsError> {
    let artists = get_artists(db).await?;

    Ok(sort_artists(filter_artists(artists, request), request))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_artist(id: u64, title: &str) -> LibraryArtist {
        use moosicbox_music_models::ApiSources;

        LibraryArtist {
            id,
            title: title.to_string(),
            cover: None,
            api_sources: ApiSources::default(),
        }
    }

    #[test_log::test]
    fn test_filter_artists_by_name() {
        let artists = vec![
            create_test_artist(1, "The Beatles"),
            create_test_artist(2, "Pink Floyd"),
            create_test_artist(3, "The Rolling Stones"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("the".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|a| a.title == "The Beatles"));
        assert!(filtered.iter().any(|a| a.title == "The Rolling Stones"));
        assert!(!filtered.iter().any(|a| a.title == "Pink Floyd"));
    }

    #[test_log::test]
    fn test_filter_artists_by_name_case_insensitive() {
        let artists = vec![
            create_test_artist(1, "METALLICA"),
            create_test_artist(2, "metallica underground"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("metal".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);

        assert_eq!(filtered.len(), 2);
    }

    #[test_log::test]
    fn test_filter_artists_by_search() {
        let artists = vec![
            create_test_artist(1, "AC/DC"),
            create_test_artist(2, "Coldplay"),
            create_test_artist(3, "AC/DC Tribute Band"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: None,
                search: Some("ac/dc".to_string()),
            },
        };

        let filtered = filter_artists(artists, &request);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|a| a.title == "AC/DC"));
        assert!(filtered.iter().any(|a| a.title == "AC/DC Tribute Band"));
    }

    #[test_log::test]
    fn test_filter_artists_with_no_filters() {
        let artists = vec![
            create_test_artist(1, "Artist One"),
            create_test_artist(2, "Artist Two"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);

        assert_eq!(filtered.len(), 2);
    }

    #[test_log::test]
    fn test_filter_artists_no_matches() {
        let artists = vec![
            create_test_artist(1, "Artist One"),
            create_test_artist(2, "Artist Two"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("nonexistent".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);

        assert_eq!(filtered.len(), 0);
    }

    #[test_log::test]
    fn test_sort_artists_name_asc() {
        let artists = vec![
            create_test_artist(1, "Zebra"),
            create_test_artist(2, "alpha"),
            create_test_artist(3, "Beta"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        assert_eq!(sorted[0].title, "alpha");
        assert_eq!(sorted[1].title, "Beta");
        assert_eq!(sorted[2].title, "Zebra");
    }

    #[test_log::test]
    fn test_sort_artists_name_desc() {
        let artists = vec![
            create_test_artist(1, "alpha"),
            create_test_artist(2, "Zebra"),
            create_test_artist(3, "Beta"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameDesc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        assert_eq!(sorted[0].title, "Zebra");
        assert_eq!(sorted[1].title, "Beta");
        assert_eq!(sorted[2].title, "alpha");
    }

    #[test_log::test]
    fn test_sort_artists_no_sort_specified() {
        let artists = vec![
            create_test_artist(1, "Zebra"),
            create_test_artist(2, "alpha"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        // When no sort is specified, should default to NameAsc behavior
        assert_eq!(sorted[0].title, "alpha");
        assert_eq!(sorted[1].title, "Zebra");
    }

    #[test_log::test]
    fn test_sort_artists_case_insensitive() {
        let artists = vec![
            create_test_artist(1, "abc"),
            create_test_artist(2, "ABC"),
            create_test_artist(3, "AbC"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        // All should be treated as equal case-insensitively
        assert_eq!(sorted.len(), 3);
    }

    #[test_log::test]
    fn test_filter_artists_with_combined_name_and_search_filters() {
        let artists = vec![
            create_test_artist(1, "The Beatles"), // matches name "the" but not search "rock"
            create_test_artist(2, "The Rock Band"), // matches both name "the" and search "rock"
            create_test_artist(3, "Pink Floyd Rock"), // matches search "rock" but not name "the"
            create_test_artist(4, "Queen"),       // matches neither
            create_test_artist(5, "The Rock Stars"), // matches both name "the" and search "rock"
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("the".to_string()),
                search: Some("rock".to_string()),
            },
        };

        let filtered = filter_artists(artists, &request);

        // Only artists matching BOTH filters should be returned
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|a| a.title == "The Rock Band"));
        assert!(filtered.iter().any(|a| a.title == "The Rock Stars"));
        // These should NOT be in the result
        assert!(!filtered.iter().any(|a| a.title == "The Beatles"));
        assert!(!filtered.iter().any(|a| a.title == "Pink Floyd Rock"));
        assert!(!filtered.iter().any(|a| a.title == "Queen"));
    }

    #[test_log::test]
    fn test_filter_and_sort_artists_integration() {
        let artists = vec![
            create_test_artist(1, "Zebra Band"),
            create_test_artist(2, "Alpha Band"),
            create_test_artist(3, "Middle Band"),
            create_test_artist(4, "Other Artist"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: Some("band".to_string()),
                search: None,
            },
        };

        // First filter, then sort (matching the get_all_artists behavior)
        let filtered = filter_artists(artists, &request);
        let sorted = sort_artists(filtered, &request);

        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].title, "Alpha Band");
        assert_eq!(sorted[1].title, "Middle Band");
        assert_eq!(sorted[2].title, "Zebra Band");
    }

    #[test_log::test]
    fn test_filter_artists_with_unicode_characters() {
        let artists = vec![
            create_test_artist(1, "东方神起"),       // TVXQ in Chinese
            create_test_artist(2, "東方神起"),       // TVXQ in Japanese
            create_test_artist(3, "BTS 방탄소년단"), // BTS in Korean
            create_test_artist(4, "Björk"),          // Icelandic artist
            create_test_artist(5, "Motörhead"),      // Umlaut
        ];

        // Test filtering with Unicode in search term
        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("神起".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|a| a.title == "东方神起"));
        assert!(filtered.iter().any(|a| a.title == "東方神起"));
    }

    #[test_log::test]
    fn test_filter_artists_with_accented_characters() {
        let artists = vec![
            create_test_artist(1, "Björk"),
            create_test_artist(2, "bjork"),
            create_test_artist(3, "BJORK"),
        ];

        // Lowercase 'björk' should only match exact lowercase (after lowercasing)
        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("björk".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);
        // Only "Björk" matches because the filter uses exact lowercase comparison
        assert_eq!(filtered.len(), 1);
        assert!(filtered.iter().any(|a| a.title == "Björk"));
    }

    #[test_log::test]
    fn test_filter_artists_empty_artist_name() {
        let artists = vec![
            create_test_artist(1, ""),            // Empty name
            create_test_artist(2, "Some Artist"), // Non-empty
            create_test_artist(3, " "),           // Whitespace only
        ];

        // Empty filter matches all
        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some(String::new()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);
        // Empty string filter should match all artist names (all contain "")
        assert_eq!(filtered.len(), 3);
    }

    #[test_log::test]
    fn test_filter_artists_whitespace_filter() {
        let artists = vec![
            create_test_artist(1, "The Beatles"),
            create_test_artist(2, "Metallica"),
            create_test_artist(3, "AC/DC"),
        ];

        // Space in filter
        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some(" ".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);
        // Only "The Beatles" contains a space
        assert_eq!(filtered.len(), 1);
        assert!(filtered.iter().any(|a| a.title == "The Beatles"));
    }

    #[test_log::test]
    fn test_sort_artists_unicode_ordering() {
        let artists = vec![
            create_test_artist(1, "Zebra"),
            create_test_artist(2, "东方神起"), // Chinese characters
            create_test_artist(3, "Apple"),
            create_test_artist(4, "日本"), // Japanese characters
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        // ASCII characters should come before CJK characters in standard Unicode ordering
        // "Apple" < "Zebra" < CJK characters
        assert_eq!(sorted.len(), 4);
        assert_eq!(sorted[0].title, "Apple");
        assert_eq!(sorted[1].title, "Zebra");
        // CJK characters are sorted after ASCII
    }

    #[test_log::test]
    fn test_sort_artists_empty_names() {
        let artists = vec![
            create_test_artist(1, "Beta"),
            create_test_artist(2, ""),
            create_test_artist(3, "Alpha"),
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        // Empty string should sort before non-empty strings
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].title, "");
        assert_eq!(sorted[1].title, "Alpha");
        assert_eq!(sorted[2].title, "Beta");
    }

    #[test_log::test]
    fn test_sort_artists_mixed_leading_whitespace() {
        let artists = vec![
            create_test_artist(1, " Alpha"), // Leading space
            create_test_artist(2, "Beta"),
            create_test_artist(3, "  Charlie"), // Two leading spaces
        ];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        // Spaces sort before letters in ASCII/Unicode
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].title, "  Charlie"); // Most spaces first
        assert_eq!(sorted[1].title, " Alpha");
        assert_eq!(sorted[2].title, "Beta");
    }

    #[test_log::test]
    fn test_sort_artists_single_element() {
        let artists = vec![create_test_artist(1, "Solo Artist")];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].title, "Solo Artist");
    }

    #[test_log::test]
    fn test_sort_artists_empty_list() {
        let artists: Vec<LibraryArtist> = vec![];

        let request = ArtistsRequest {
            sources: None,
            sort: Some(ArtistSort::NameAsc),
            filters: ArtistFilters {
                name: None,
                search: None,
            },
        };

        let sorted = sort_artists(artists, &request);

        assert!(sorted.is_empty());
    }

    #[test_log::test]
    fn test_filter_artists_empty_list() {
        let artists: Vec<LibraryArtist> = vec![];

        let request = ArtistsRequest {
            sources: None,
            sort: None,
            filters: ArtistFilters {
                name: Some("test".to_string()),
                search: None,
            },
        };

        let filtered = filter_artists(artists, &request);

        assert!(filtered.is_empty());
    }
}
