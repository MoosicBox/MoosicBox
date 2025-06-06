use std::str::FromStr as _;

use moosicbox_json_utils::{ParseError, ToValueType, tantivy::ToValue as _};
use moosicbox_music_models::{AudioFormat, TrackApiSource, api::ApiAlbumVersionQuality, id::Id};
use serde::{Deserialize, Serialize};
use tantivy::schema::NamedFieldDocument;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalArtistSearchResult {
    pub artist_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalAlbumSearchResult {
    pub artist_id: Id,
    pub artist: String,
    pub album_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub versions: Vec<ApiAlbumVersionQuality>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiGlobalTrackSearchResult {
    pub artist_id: Id,
    pub artist: String,
    pub album_id: Id,
    pub album: String,
    pub track_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiGlobalSearchResult {
    Artist(ApiGlobalArtistSearchResult),
    Album(ApiGlobalAlbumSearchResult),
    Track(ApiGlobalTrackSearchResult),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSearchResultsResponse {
    pub position: usize,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRawSearchResultsResponse {
    pub position: usize,
    pub results: Vec<NamedFieldDocument>,
}

impl ToValueType<ApiGlobalArtistSearchResult> for &NamedFieldDocument {
    fn to_value_type(self) -> std::result::Result<ApiGlobalArtistSearchResult, ParseError> {
        Ok(ApiGlobalArtistSearchResult {
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("artist_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
        })
    }
}

impl ToValueType<ApiGlobalAlbumSearchResult> for &NamedFieldDocument {
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
        })
    }
}

impl ToValueType<ApiGlobalTrackSearchResult> for &NamedFieldDocument {
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
        })
    }
}

impl ToValueType<ApiGlobalSearchResult> for &NamedFieldDocument {
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
