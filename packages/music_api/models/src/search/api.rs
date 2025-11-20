//! API models for global search results.
//!
//! This module defines the data structures for representing search results from global
//! music searches, including artists, albums, and tracks. It provides both typed result
//! structures for public APIs and conversion utilities for parsing raw search index
//! documents (using Tantivy) into typed results.
//!
//! # Search Result Types
//!
//! * [`ApiGlobalArtistSearchResult`] - Artist search result with basic metadata
//! * [`ApiGlobalAlbumSearchResult`] - Album search result with artist info and available versions
//! * [`ApiGlobalTrackSearchResult`] - Track search result with full audio quality metadata
//! * [`ApiGlobalSearchResult`] - Tagged enum containing any of the above result types
//!
//! # Examples
//!
//! Creating a search response:
//!
//! ```rust
//! use moosicbox_music_api_models::search::api::{ApiSearchResultsResponse, ApiGlobalSearchResult};
//!
//! let results: Vec<ApiGlobalSearchResult> = vec![];
//! let response: ApiSearchResultsResponse = results.into();
//! assert_eq!(response.position, 0);
//! ```

use std::str::FromStr as _;

use moosicbox_json_utils::{ParseError, ToValueType, tantivy::ToValue as _};
use moosicbox_music_models::{
    ApiSource, AudioFormat, TrackApiSource, api::ApiAlbumVersionQuality, id::Id,
};
use serde::{Deserialize, Serialize};
use tantivy::schema::NamedFieldDocument;

/// Artist search result from a global search query.
///
/// Contains essential artist information returned from search operations.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalArtistSearchResult {
    /// Unique identifier for the artist
    pub artist_id: Id,
    /// Artist name/title
    pub title: String,
    /// Whether the artist has associated cover art
    pub contains_cover: bool,
    /// Whether the cover image should be blurred (e.g., explicit content)
    pub blur: bool,
    /// API source that provided this result
    pub api_source: ApiSource,
}

/// Album search result from a global search query.
///
/// Contains comprehensive album information including artist details and available versions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalAlbumSearchResult {
    /// Unique identifier for the artist
    pub artist_id: Id,
    /// Artist name
    pub artist: String,
    /// Unique identifier for the album
    pub album_id: Id,
    /// Album title
    pub title: String,
    /// Whether the album has associated cover art
    pub contains_cover: bool,
    /// Whether the cover image should be blurred (e.g., explicit content)
    pub blur: bool,
    /// Date the album was released
    pub date_released: Option<String>,
    /// Date the album was added to the library
    pub date_added: Option<String>,
    /// Available quality versions of the album
    pub versions: Vec<ApiAlbumVersionQuality>,
    /// API source that provided this result
    pub api_source: ApiSource,
}

/// Track search result from a global search query.
///
/// Contains detailed track information including parent album/artist and audio quality metadata.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalTrackSearchResult {
    /// Unique identifier for the artist
    pub artist_id: Id,
    /// Artist name
    pub artist: String,
    /// Unique identifier for the album
    pub album_id: Id,
    /// Album title
    pub album: String,
    /// Unique identifier for the track
    pub track_id: Id,
    /// Track title
    pub title: String,
    /// Whether the track has associated cover art
    pub contains_cover: bool,
    /// Whether the cover image should be blurred (e.g., explicit content)
    pub blur: bool,
    /// Date the track was released
    pub date_released: Option<String>,
    /// Date the track was added to the library
    pub date_added: Option<String>,
    /// Audio format of the track
    pub format: Option<AudioFormat>,
    /// Audio bit depth in bits (e.g., 16, 24)
    pub bit_depth: Option<u8>,
    /// Audio sample rate in Hz (e.g., 44100, 96000)
    pub sample_rate: Option<u32>,
    /// Number of audio channels (e.g., 2 for stereo)
    pub channels: Option<u8>,
    /// Track API source identifier
    pub source: TrackApiSource,
    /// API source that provided this result
    pub api_source: ApiSource,
}

/// A search result that can be an artist, album, or track.
///
/// Tagged enum representing different types of music entities that can be returned
/// from a global search operation.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiGlobalSearchResult {
    /// An artist search result
    Artist(ApiGlobalArtistSearchResult),
    /// An album search result
    Album(ApiGlobalAlbumSearchResult),
    /// A track search result
    Track(ApiGlobalTrackSearchResult),
}

/// Response containing search results with pagination information.
///
/// Wraps a collection of search results along with the current position for pagination.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSearchResultsResponse {
    /// Current position in the result set (for pagination)
    pub position: u32,
    /// Collection of search results
    pub results: Vec<ApiGlobalSearchResult>,
}

impl From<Vec<ApiGlobalSearchResult>> for ApiSearchResultsResponse {
    fn from(value: Vec<ApiGlobalSearchResult>) -> Self {
        Self {
            position: 0,
            results: value,
        }
    }
}

/// Raw search results response containing unparsed Tantivy documents.
///
/// Internal representation of search results before conversion to typed result structures.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRawSearchResultsResponse {
    /// Current position in the result set (for pagination)
    pub position: u32,
    /// Raw Tantivy document results
    pub results: Vec<NamedFieldDocument>,
}

impl ToValueType<ApiGlobalArtistSearchResult> for &NamedFieldDocument {
    /// Converts a Tantivy document to an artist search result.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required fields are missing from the document
    /// * Field values cannot be converted to the expected types
    fn to_value_type(self) -> std::result::Result<ApiGlobalArtistSearchResult, ParseError> {
        Ok(ApiGlobalArtistSearchResult {
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("artist_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
            api_source: ApiSource::library(),
        })
    }
}

impl ToValueType<ApiGlobalAlbumSearchResult> for &NamedFieldDocument {
    /// Converts a Tantivy document to an album search result.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required fields are missing from the document
    /// * Field values cannot be converted to the expected types
    /// * Audio format or track source strings cannot be parsed
    fn to_value_type(self) -> std::result::Result<ApiGlobalAlbumSearchResult, ParseError> {
        Ok(ApiGlobalAlbumSearchResult {
            artist_id: self.to_value("artist_id")?,
            artist: self.to_value("artist_title")?,
            album_id: self.to_value("album_id")?,
            title: self.to_value("album_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            versions: self
                .to_value::<Vec<Option<&str>>>("version_formats")?
                .iter()
                .zip(self.to_value::<Vec<&str>>("version_sources")?.iter())
                .zip(
                    self.to_value::<Vec<Option<u8>>>("version_bit_depths")?
                        .iter(),
                )
                .zip(
                    self.to_value::<Vec<Option<u32>>>("version_sample_rates")?
                        .iter(),
                )
                .zip(self.to_value::<Vec<Option<u8>>>("version_channels")?.iter())
                .map(|((((format, source), bit_depth), sample_rate), channels)| {
                    Ok(ApiAlbumVersionQuality {
                        format: format
                            .map(|format| {
                                AudioFormat::from_str(format).map_err(|_| {
                                    ParseError::ConvertType(format!("AudioFormat '{format}'"))
                                })
                            })
                            .transpose()?,
                        bit_depth: *bit_depth,
                        sample_rate: *sample_rate,
                        channels: *channels,
                        source: TrackApiSource::from_str(source)
                            .map_err(|_| ParseError::ConvertType("TrackSource".into()))?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            api_source: ApiSource::library(),
        })
    }
}

impl ToValueType<ApiGlobalTrackSearchResult> for &NamedFieldDocument {
    /// Converts a Tantivy document to a track search result.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required fields are missing from the document
    /// * Field values cannot be converted to the expected types
    /// * Audio format or track source strings cannot be parsed
    fn to_value_type(self) -> std::result::Result<ApiGlobalTrackSearchResult, ParseError> {
        Ok(ApiGlobalTrackSearchResult {
            artist_id: self.to_value("artist_id")?,
            artist: self.to_value("artist_title")?,
            album_id: self.to_value("album_id")?,
            album: self.to_value("album_title")?,
            track_id: self.to_value("track_id")?,
            title: self.to_value("track_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            format: self
                .to_value::<Option<&str>>("version_formats")?
                .map(|format| {
                    AudioFormat::from_str(format)
                        .map_err(|_| ParseError::ConvertType(format!("AudioFormat '{format}'")))
                })
                .transpose()?,
            bit_depth: self.to_value("version_bit_depths")?,
            sample_rate: self.to_value("version_sample_rates")?,
            channels: self.to_value("version_channels")?,
            source: TrackApiSource::from_str(self.to_value("version_sources")?)
                .map_err(|_| ParseError::ConvertType("TrackSource".into()))?,
            api_source: ApiSource::library(),
        })
    }
}

impl ToValueType<ApiGlobalSearchResult> for &NamedFieldDocument {
    /// Converts a Tantivy document to a typed global search result.
    ///
    /// Determines the result type based on the `document_type` field and converts accordingly.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * The `document_type` field is missing or contains an unrecognized value
    /// * The document cannot be converted to the appropriate result type
    fn to_value_type(self) -> std::result::Result<ApiGlobalSearchResult, ParseError> {
        Ok(match self.to_value("document_type")? {
            "artists" => ApiGlobalSearchResult::Artist(self.to_value_type()?),
            "albums" => ApiGlobalSearchResult::Album(self.to_value_type()?),
            "tracks" => ApiGlobalSearchResult::Track(self.to_value_type()?),
            _ => {
                return Err(ParseError::ConvertType("document_type".into()));
            }
        })
    }
}

impl ApiGlobalSearchResult {
    /// Generates a unique string key for this search result.
    ///
    /// Creates a composite key combining the result type with relevant identifying fields,
    /// useful for deduplication or keying in maps.
    #[must_use]
    pub fn to_key(&self) -> String {
        match self {
            Self::Artist(artist) => format!("artist|{}", artist.title),
            Self::Album(album) => {
                format!("album|{}|{}", album.title, album.artist)
            }
            Self::Track(track) => {
                format!("track|{}|{}|{}", track.title, track.album, track.artist)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::id::Id;

    mod api_search_results_response {
        use super::*;

        #[test]
        fn test_from_empty_vec() {
            let results: Vec<ApiGlobalSearchResult> = vec![];
            let response: ApiSearchResultsResponse = results.into();
            assert_eq!(response.position, 0);
            assert_eq!(response.results.len(), 0);
        }

        #[test]
        fn test_from_vec_with_results() {
            let artist = ApiGlobalArtistSearchResult {
                artist_id: Id::Number(1),
                title: "Test Artist".to_string(),
                contains_cover: true,
                blur: false,
                api_source: ApiSource::library(),
            };
            let results = vec![ApiGlobalSearchResult::Artist(artist)];
            let response: ApiSearchResultsResponse = results.into();
            assert_eq!(response.position, 0);
            assert_eq!(response.results.len(), 1);
        }
    }

    mod api_global_search_result_to_key {
        use super::*;

        #[test]
        fn test_artist_to_key() {
            let artist = ApiGlobalArtistSearchResult {
                artist_id: Id::Number(1),
                title: "The Beatles".to_string(),
                contains_cover: true,
                blur: false,
                api_source: ApiSource::library(),
            };
            let result = ApiGlobalSearchResult::Artist(artist);
            assert_eq!(result.to_key(), "artist|The Beatles");
        }

        #[test]
        fn test_album_to_key() {
            let album = ApiGlobalAlbumSearchResult {
                artist_id: Id::Number(1),
                artist: "Pink Floyd".to_string(),
                album_id: Id::Number(2),
                title: "Dark Side of the Moon".to_string(),
                contains_cover: true,
                blur: false,
                date_released: Some("1973-03-01".to_string()),
                date_added: Some("2024-01-01".to_string()),
                versions: vec![],
                api_source: ApiSource::library(),
            };
            let result = ApiGlobalSearchResult::Album(album);
            assert_eq!(result.to_key(), "album|Dark Side of the Moon|Pink Floyd");
        }

        #[test]
        fn test_track_to_key() {
            let track = ApiGlobalTrackSearchResult {
                artist_id: Id::Number(1),
                artist: "Led Zeppelin".to_string(),
                album_id: Id::Number(2),
                album: "Led Zeppelin IV".to_string(),
                track_id: Id::Number(3),
                title: "Stairway to Heaven".to_string(),
                contains_cover: false,
                blur: false,
                date_released: Some("1971-11-08".to_string()),
                date_added: None,
                format: Some(AudioFormat::Source),
                bit_depth: Some(24),
                sample_rate: Some(96000),
                channels: Some(2),
                source: TrackApiSource::Local,
                api_source: ApiSource::library(),
            };
            let result = ApiGlobalSearchResult::Track(track);
            assert_eq!(
                result.to_key(),
                "track|Stairway to Heaven|Led Zeppelin IV|Led Zeppelin"
            );
        }

        #[test]
        fn test_to_key_uniqueness() {
            let artist1 = ApiGlobalArtistSearchResult {
                artist_id: Id::Number(1),
                title: "Same Title".to_string(),
                contains_cover: false,
                blur: false,
                api_source: ApiSource::library(),
            };

            let album1 = ApiGlobalAlbumSearchResult {
                artist_id: Id::Number(2),
                artist: "Artist".to_string(),
                album_id: Id::Number(3),
                title: "Same Title".to_string(),
                contains_cover: false,
                blur: false,
                date_released: None,
                date_added: None,
                versions: vec![],
                api_source: ApiSource::library(),
            };

            let result1 = ApiGlobalSearchResult::Artist(artist1);
            let result2 = ApiGlobalSearchResult::Album(album1);

            // Keys should be different even with same title
            assert_ne!(result1.to_key(), result2.to_key());
            assert!(result1.to_key().starts_with("artist|"));
            assert!(result2.to_key().starts_with("album|"));
        }

        #[test]
        fn test_to_key_with_special_characters() {
            let artist = ApiGlobalArtistSearchResult {
                artist_id: Id::Number(1),
                title: "Artist|With|Pipes".to_string(),
                contains_cover: false,
                blur: false,
                api_source: ApiSource::library(),
            };
            let result = ApiGlobalSearchResult::Artist(artist);
            assert_eq!(result.to_key(), "artist|Artist|With|Pipes");
        }
    }
}
