//! Tidal API data models and type conversions.
//!
//! This module contains structs representing Tidal's API data structures including
//! artists, albums, tracks, and search results. It provides conversions between
//! Tidal-specific types and MoosicBox common types.

use std::fmt::Display;

use moosicbox_date_utils::chrono::{self, parse_date_time};
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::AsModelResult,
    serde_json::{ToNestedValue, ToValue},
};
use moosicbox_music_api::models::{
    ImageCoverSize,
    search::api::{
        ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
        ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, ApiSources, Artist, Track,
    api::{ApiAlbum, ApiArtist},
    id::TryFromIdError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{API_SOURCE, TidalAlbumType};

/// Tidal artist metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtist {
    /// Tidal artist ID.
    pub id: u64,
    /// Artist picture hash (convert with hyphens replaced by slashes for URL construction).
    pub picture: Option<String>,
    /// Whether the artist has cover artwork available.
    pub contains_cover: bool,
    /// Artist popularity score.
    pub popularity: u32,
    /// Artist name.
    pub name: String,
}

impl From<TidalArtist> for Artist {
    fn from(value: TidalArtist) -> Self {
        let cover = value
            .picture
            .as_deref()
            .map(|x| artist_picture_url(x, TidalArtistImageSize::Max));

        Self {
            id: value.id.into(),
            title: value.name,
            cover,
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<TidalArtist> for ApiArtist {
    fn from(value: TidalArtist) -> Self {
        Self {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.contains_cover,
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<TidalArtist> for ApiGlobalSearchResult {
    fn from(value: TidalArtist) -> Self {
        Self::Artist(ApiGlobalArtistSearchResult {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.contains_cover,
            blur: false,
            api_source: API_SOURCE.clone(),
        })
    }
}

/// Available image sizes for Tidal artist pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TidalArtistImageSize {
    /// Maximum resolution (750x750 pixels).
    Max,
    /// Large resolution (480x480 pixels).
    Large,
    /// Medium resolution (320x320 pixels).
    Medium,
    /// Small resolution (160x160 pixels).
    Small,
}

impl From<ImageCoverSize> for TidalArtistImageSize {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => Self::Max,
            ImageCoverSize::Large => Self::Large,
            ImageCoverSize::Medium => Self::Medium,
            ImageCoverSize::Small | ImageCoverSize::Thumbnail => Self::Small,
        }
    }
}

impl From<TidalArtistImageSize> for u16 {
    fn from(value: TidalArtistImageSize) -> Self {
        match value {
            TidalArtistImageSize::Max => 750,
            TidalArtistImageSize::Large => 480,
            TidalArtistImageSize::Medium => 320,
            TidalArtistImageSize::Small => 160,
        }
    }
}

impl From<u16> for TidalArtistImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=160 => Self::Small,
            161..=320 => Self::Medium,
            321..=480 => Self::Large,
            _ => Self::Max,
        }
    }
}

impl Display for TidalArtistImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl TidalArtist {
    /// Returns the full URL for the artist's picture at the specified size.
    #[must_use]
    pub fn picture_url(&self, size: TidalArtistImageSize) -> Option<String> {
        self.picture.as_deref().map(|x| artist_picture_url(x, size))
    }
}

fn artist_picture_url(picture: &str, size: TidalArtistImageSize) -> String {
    let picture_path = picture.replace('-', "/");
    format!("https://resources.tidal.com/images/{picture_path}/{size}x{size}.jpg")
}

impl ToValueType<TidalArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(TidalArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            popularity: self.to_value("popularity")?,
            name: self.to_value("name")?,
        })
    }
}

/// Tidal artist metadata from search results.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchArtist {
    /// Tidal artist ID.
    pub id: u64,
    /// Artist picture hash (convert with hyphens replaced by slashes for URL construction).
    pub picture: Option<String>,
    /// Whether the artist has cover artwork available.
    pub contains_cover: bool,
    /// Artist type identifier.
    pub r#type: String,
    /// Artist name.
    pub name: String,
}

impl From<TidalSearchArtist> for ApiGlobalSearchResult {
    fn from(value: TidalSearchArtist) -> Self {
        Self::Artist(ApiGlobalArtistSearchResult {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.contains_cover,
            blur: false,
            api_source: API_SOURCE.clone(),
        })
    }
}

impl TidalSearchArtist {
    /// Returns the full URL for the artist's picture at the specified size.
    #[must_use]
    pub fn picture_url(&self, size: TidalArtistImageSize) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.tidal.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<TidalSearchArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalSearchArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalSearchArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalSearchArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(TidalSearchArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            r#type: self.to_value("type")?,
            name: self.to_value("name")?,
        })
    }
}

/// Tidal album metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    /// Tidal album ID.
    pub id: u64,
    /// Album artist name.
    pub artist: String,
    /// Tidal artist ID.
    pub artist_id: u64,
    /// Album type (LP, EPs/Singles, Compilations).
    pub album_type: TidalAlbumType,
    /// Whether the album has cover artwork available.
    pub contains_cover: bool,
    /// Audio quality level for this album.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Album cover hash (convert with hyphens replaced by slashes for URL construction).
    pub cover: Option<String>,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album contains explicit content.
    pub explicit: bool,
    /// Total number of tracks on the album.
    pub number_of_tracks: u32,
    /// Album popularity score.
    pub popularity: u32,
    /// Release date in ISO 8601 format.
    pub release_date: Option<String>,
    /// Album title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
}

impl TryFrom<TidalAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: TidalAlbum) -> Result<Self, Self::Error> {
        let artwork = value.cover_url(TidalAlbumImageSize::Max);

        Ok(Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value
                .release_date
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            date_added: None,
            artwork,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            artist_sources: ApiSources::default()
                .with_source(API_SOURCE.clone(), value.artist_id.into()),
            album_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        })
    }
}

impl TryFrom<TidalAlbum> for ApiAlbum {
    type Error = <Album as TryFrom<TidalAlbum>>::Error;

    fn try_from(value: TidalAlbum) -> Result<Self, Self::Error> {
        let album: Album = value.try_into()?;
        Ok(album.into())
    }
}

impl TryFrom<Album> for TidalAlbum {
    type Error = TryFromIdError;

    fn try_from(value: Album) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.try_into()?,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.try_into()?,
            album_type: value.album_type.try_into().unwrap_or(TidalAlbumType::Lp),
            contains_cover: value.artwork.is_some(),
            audio_quality: "N/A".to_string(),
            copyright: None,
            cover: value.artwork,
            duration: 0,
            explicit: false,
            number_of_tracks: 0,
            popularity: 0,
            release_date: value.date_released.map(|x| x.and_utc().to_rfc3339()),
            media_metadata_tags: vec![],
        })
    }
}

/// Available image sizes for Tidal album covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TidalAlbumImageSize {
    /// Maximum resolution (1280x1280 pixels).
    Max,
    /// Large resolution (640x640 pixels).
    Large,
    /// Medium resolution (320x320 pixels).
    Medium,
    /// Small resolution (160x160 pixels).
    Small,
    /// Thumbnail resolution (80x80 pixels).
    Thumbnail,
}

impl From<ImageCoverSize> for TidalAlbumImageSize {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => Self::Max,
            ImageCoverSize::Large => Self::Large,
            ImageCoverSize::Medium => Self::Medium,
            ImageCoverSize::Small => Self::Small,
            ImageCoverSize::Thumbnail => Self::Thumbnail,
        }
    }
}

impl From<TidalAlbumImageSize> for u16 {
    fn from(value: TidalAlbumImageSize) -> Self {
        match value {
            TidalAlbumImageSize::Max => 1280,
            TidalAlbumImageSize::Large => 640,
            TidalAlbumImageSize::Medium => 320,
            TidalAlbumImageSize::Small => 160,
            TidalAlbumImageSize::Thumbnail => 80,
        }
    }
}

impl From<u16> for TidalAlbumImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=80 => Self::Thumbnail,
            81..=160 => Self::Small,
            161..=320 => Self::Medium,
            321..=640 => Self::Large,
            _ => Self::Max,
        }
    }
}

impl Display for TidalAlbumImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

/// Tidal album metadata from search results.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchAlbum {
    /// Tidal album ID.
    pub id: u64,
    /// Artists associated with this album.
    pub artists: Vec<TidalSearchArtist>,
    /// Whether the album has cover artwork available.
    pub contains_cover: bool,
    /// Audio quality level for this album.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Album cover hash (convert with hyphens replaced by slashes for URL construction).
    pub cover: Option<String>,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album contains explicit content.
    pub explicit: bool,
    /// Total number of tracks on the album.
    pub number_of_tracks: u32,
    /// Album popularity score.
    pub popularity: u32,
    /// Release date in ISO 8601 format.
    pub release_date: Option<String>,
    /// Album title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
}

impl From<TidalSearchAlbum> for ApiGlobalSearchResult {
    fn from(value: TidalSearchAlbum) -> Self {
        let artist = value.artists.into_iter().next().expect("Missing artist");
        Self::Album(ApiGlobalAlbumSearchResult {
            artist_id: artist.id.into(),
            artist: artist.name,
            album_id: value.id.into(),
            title: value.title,
            contains_cover: value.contains_cover,
            blur: false,
            date_released: value.release_date,
            date_added: None,
            versions: vec![],
            api_source: API_SOURCE.clone(),
        })
    }
}

impl TidalSearchAlbum {
    /// Returns the full URL for the album's cover art at the specified size.
    #[must_use]
    pub fn cover_url(&self, size: TidalAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.tidal.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<TidalSearchAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalSearchAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalSearchAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalSearchAlbum, ParseError> {
        Ok(TidalSearchAlbum {
            id: self.to_value("id")?,
            artists: self.to_value("artists")?,
            contains_cover: true,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("releaseDate")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

impl TidalAlbum {
    /// Returns the full URL for the album's cover art at the specified size.
    #[must_use]
    pub fn cover_url(&self, size: TidalAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.tidal.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<TidalAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalAlbum, ParseError> {
        Ok(TidalAlbum {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            album_type: self.to_value("type")?,
            contains_cover: true,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            cover: self.to_value("cover")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            number_of_tracks: self.to_value("numberOfTracks")?,
            popularity: self.to_value("popularity")?,
            release_date: self.to_value("releaseDate")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

/// Tidal track metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    /// Tidal track ID.
    pub id: u64,
    /// Track number on the album.
    pub track_number: u32,
    /// Tidal artist ID.
    pub artist_id: u64,
    /// Artist name.
    pub artist: String,
    /// Artist cover hash (convert with hyphens replaced by slashes for URL construction).
    pub artist_cover: Option<String>,
    /// Tidal album ID.
    pub album_id: u64,
    /// Album type (LP, EPs/Singles, Compilations).
    pub album_type: TidalAlbumType,
    /// Album title.
    pub album: String,
    /// Album cover hash (convert with hyphens replaced by slashes for URL construction).
    pub album_cover: Option<String>,
    /// Audio quality level for this track.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Track duration in seconds.
    pub duration: u32,
    /// Whether the track contains explicit content.
    pub explicit: bool,
    /// International Standard Recording Code.
    pub isrc: String,
    /// Track popularity score.
    pub popularity: u32,
    /// Track title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
}

impl From<TidalTrack> for Track {
    fn from(value: TidalTrack) -> Self {
        Self {
            id: value.id.into(),
            number: value.track_number,
            title: value.title,
            duration: f64::from(value.duration),
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: None,
            date_added: None,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork: value.artist_cover,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl ToValueType<TidalTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalTrack, ParseError> {
        let album_type: Option<TidalAlbumType> = self.to_nested_value(&["album", "type"])?;
        Ok(TidalTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album_type: album_type.unwrap_or_default(),
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            isrc: self.to_value("isrc")?,
            popularity: self.to_value("popularity")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

/// Tidal track metadata from search results.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchTrack {
    /// Tidal track ID.
    pub id: u64,
    /// Track number on the album.
    pub track_number: u32,
    /// Artists associated with this track.
    pub artists: Vec<TidalSearchArtist>,
    /// Artist cover hash (convert with hyphens replaced by slashes for URL construction).
    pub artist_cover: Option<String>,
    /// Tidal album ID.
    pub album_id: u64,
    /// Album title.
    pub album: String,
    /// Album cover hash (convert with hyphens replaced by slashes for URL construction).
    pub album_cover: Option<String>,
    /// Audio quality level for this track.
    pub audio_quality: String,
    /// Copyright information.
    pub copyright: Option<String>,
    /// Track duration in seconds.
    pub duration: u32,
    /// Whether the track contains explicit content.
    pub explicit: bool,
    /// International Standard Recording Code.
    pub isrc: String,
    /// Track popularity score.
    pub popularity: u32,
    /// Track title.
    pub title: String,
    /// Media metadata tags (e.g., "`LOSSLESS`", "`HIRES_LOSSLESS`").
    pub media_metadata_tags: Vec<String>,
}

impl From<TidalSearchTrack> for ApiGlobalSearchResult {
    fn from(value: TidalSearchTrack) -> Self {
        let artist = value.artists.into_iter().next().expect("Missing artist");
        Self::Track(ApiGlobalTrackSearchResult {
            artist_id: artist.id.into(),
            artist: artist.name,
            album_id: value.album_id.into(),
            album: value.album,
            title: value.title,
            contains_cover: value.album_cover.is_some(),
            blur: false,
            date_released: None,
            date_added: None,
            track_id: value.id.into(),
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
        })
    }
}

impl ToValueType<TidalSearchTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<TidalSearchTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalSearchTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<TidalSearchTrack, ParseError> {
        Ok(TidalSearchTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artists: self.to_value("artists")?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            isrc: self.to_value("isrc")?,
            popularity: self.to_value("popularity")?,
            title: self.to_value("title")?,
            media_metadata_tags: self.to_nested_value(&["mediaMetadata", "tags"])?,
        })
    }
}

/// Paginated list of search results from Tidal.
#[derive(Serialize, Deserialize)]
pub struct TidalSearchResultList<T> {
    /// List of result items.
    pub items: Vec<T>,
    /// Offset of this page in the total result set.
    pub offset: usize,
    /// Maximum number of items per page.
    pub limit: usize,
    /// Total number of items available.
    pub total: usize,
}

impl<T> ToValueType<TidalSearchResultList<T>> for &Value
where
    Value: AsModelResult<TidalSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<TidalSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<TidalSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Self: ToValueType<T>,
    for<'a> &'a Self: ToValueType<usize>,
{
    fn as_model(&self) -> Result<TidalSearchResultList<T>, ParseError> {
        Ok(TidalSearchResultList {
            items: self.to_value("items")?,
            offset: self.to_value("offset")?,
            limit: self.to_value("limit")?,
            total: self.to_value("totalNumberOfItems")?,
        })
    }
}

/// Search results containing albums, artists, and tracks from Tidal.
#[derive(Serialize, Deserialize)]
pub struct TidalSearchResults {
    /// Paginated list of album results.
    pub albums: TidalSearchResultList<TidalSearchAlbum>,
    /// Paginated list of artist results.
    pub artists: TidalSearchResultList<TidalArtist>,
    /// Paginated list of track results.
    pub tracks: TidalSearchResultList<TidalSearchTrack>,
    /// Offset of this page in the total result set.
    pub offset: usize,
    /// Maximum number of items per page.
    pub limit: usize,
}

#[allow(clippy::fallible_impl_from)]
impl From<TidalSearchResults> for ApiSearchResultsResponse {
    fn from(value: TidalSearchResults) -> Self {
        let artists = value
            .artists
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let albums = value
            .albums
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let tracks = value
            .tracks
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();

        let position = value.offset + value.limit;
        let position = if position > value.albums.total {
            value.albums.total
        } else {
            position
        };

        Self {
            position: u32::try_from(position).unwrap(),
            results: [artists, albums, tracks].concat(),
        }
    }
}

impl ToValueType<TidalSearchResults> for &Value {
    fn to_value_type(self) -> Result<TidalSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<TidalSearchResults, ParseError> {
        let albums: TidalSearchResultList<TidalSearchAlbum> = self.to_value("albums")?;
        let offset = albums.offset;
        let limit = albums.limit;
        Ok(TidalSearchResults {
            albums,
            artists: self.to_value("artists")?,
            tracks: self.to_value("tracks")?,
            offset,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_api::models::ImageCoverSize;
    use moosicbox_music_api::models::search::api::ApiGlobalSearchResult;
    use moosicbox_music_models::id::Id;
    use pretty_assertions::assert_eq;

    // TidalArtistImageSize tests
    #[test_log::test]
    fn test_tidal_artist_image_size_from_u16_boundary_values() {
        assert_eq!(
            TidalArtistImageSize::from(0_u16),
            TidalArtistImageSize::Small
        );
        assert_eq!(
            TidalArtistImageSize::from(160_u16),
            TidalArtistImageSize::Small
        );
        assert_eq!(
            TidalArtistImageSize::from(161_u16),
            TidalArtistImageSize::Medium
        );
        assert_eq!(
            TidalArtistImageSize::from(320_u16),
            TidalArtistImageSize::Medium
        );
        assert_eq!(
            TidalArtistImageSize::from(321_u16),
            TidalArtistImageSize::Large
        );
        assert_eq!(
            TidalArtistImageSize::from(480_u16),
            TidalArtistImageSize::Large
        );
        assert_eq!(
            TidalArtistImageSize::from(481_u16),
            TidalArtistImageSize::Max
        );
        assert_eq!(
            TidalArtistImageSize::from(1000_u16),
            TidalArtistImageSize::Max
        );
    }

    #[test_log::test]
    fn test_tidal_artist_image_size_to_u16() {
        assert_eq!(u16::from(TidalArtistImageSize::Small), 160);
        assert_eq!(u16::from(TidalArtistImageSize::Medium), 320);
        assert_eq!(u16::from(TidalArtistImageSize::Large), 480);
        assert_eq!(u16::from(TidalArtistImageSize::Max), 750);
    }

    #[test_log::test]
    fn test_tidal_artist_image_size_from_image_cover_size() {
        assert_eq!(
            TidalArtistImageSize::from(ImageCoverSize::Max),
            TidalArtistImageSize::Max
        );
        assert_eq!(
            TidalArtistImageSize::from(ImageCoverSize::Large),
            TidalArtistImageSize::Large
        );
        assert_eq!(
            TidalArtistImageSize::from(ImageCoverSize::Medium),
            TidalArtistImageSize::Medium
        );
        assert_eq!(
            TidalArtistImageSize::from(ImageCoverSize::Small),
            TidalArtistImageSize::Small
        );
        assert_eq!(
            TidalArtistImageSize::from(ImageCoverSize::Thumbnail),
            TidalArtistImageSize::Small
        );
    }

    #[test_log::test]
    fn test_tidal_artist_image_size_display() {
        assert_eq!(format!("{}", TidalArtistImageSize::Small), "160");
        assert_eq!(format!("{}", TidalArtistImageSize::Medium), "320");
        assert_eq!(format!("{}", TidalArtistImageSize::Large), "480");
        assert_eq!(format!("{}", TidalArtistImageSize::Max), "750");
    }

    // TidalAlbumImageSize tests
    #[test_log::test]
    fn test_tidal_album_image_size_from_u16_boundary_values() {
        assert_eq!(
            TidalAlbumImageSize::from(0_u16),
            TidalAlbumImageSize::Thumbnail
        );
        assert_eq!(
            TidalAlbumImageSize::from(80_u16),
            TidalAlbumImageSize::Thumbnail
        );
        assert_eq!(
            TidalAlbumImageSize::from(81_u16),
            TidalAlbumImageSize::Small
        );
        assert_eq!(
            TidalAlbumImageSize::from(160_u16),
            TidalAlbumImageSize::Small
        );
        assert_eq!(
            TidalAlbumImageSize::from(161_u16),
            TidalAlbumImageSize::Medium
        );
        assert_eq!(
            TidalAlbumImageSize::from(320_u16),
            TidalAlbumImageSize::Medium
        );
        assert_eq!(
            TidalAlbumImageSize::from(321_u16),
            TidalAlbumImageSize::Large
        );
        assert_eq!(
            TidalAlbumImageSize::from(640_u16),
            TidalAlbumImageSize::Large
        );
        assert_eq!(TidalAlbumImageSize::from(641_u16), TidalAlbumImageSize::Max);
        assert_eq!(
            TidalAlbumImageSize::from(2000_u16),
            TidalAlbumImageSize::Max
        );
    }

    #[test_log::test]
    fn test_tidal_album_image_size_to_u16() {
        assert_eq!(u16::from(TidalAlbumImageSize::Thumbnail), 80);
        assert_eq!(u16::from(TidalAlbumImageSize::Small), 160);
        assert_eq!(u16::from(TidalAlbumImageSize::Medium), 320);
        assert_eq!(u16::from(TidalAlbumImageSize::Large), 640);
        assert_eq!(u16::from(TidalAlbumImageSize::Max), 1280);
    }

    #[test_log::test]
    fn test_tidal_album_image_size_from_image_cover_size() {
        assert_eq!(
            TidalAlbumImageSize::from(ImageCoverSize::Max),
            TidalAlbumImageSize::Max
        );
        assert_eq!(
            TidalAlbumImageSize::from(ImageCoverSize::Large),
            TidalAlbumImageSize::Large
        );
        assert_eq!(
            TidalAlbumImageSize::from(ImageCoverSize::Medium),
            TidalAlbumImageSize::Medium
        );
        assert_eq!(
            TidalAlbumImageSize::from(ImageCoverSize::Small),
            TidalAlbumImageSize::Small
        );
        assert_eq!(
            TidalAlbumImageSize::from(ImageCoverSize::Thumbnail),
            TidalAlbumImageSize::Thumbnail
        );
    }

    #[test_log::test]
    fn test_tidal_album_image_size_display() {
        assert_eq!(format!("{}", TidalAlbumImageSize::Thumbnail), "80");
        assert_eq!(format!("{}", TidalAlbumImageSize::Small), "160");
        assert_eq!(format!("{}", TidalAlbumImageSize::Medium), "320");
        assert_eq!(format!("{}", TidalAlbumImageSize::Large), "640");
        assert_eq!(format!("{}", TidalAlbumImageSize::Max), "1280");
    }

    // Artist picture URL tests
    #[test_log::test]
    fn test_tidal_artist_picture_url_construction() {
        let artist = TidalArtist {
            id: 12345,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        let url = artist.picture_url(TidalArtistImageSize::Max);
        assert_eq!(
            url,
            Some("https://resources.tidal.com/images/abc/def/ghi/750x750.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_tidal_artist_picture_url_different_sizes() {
        let artist = TidalArtist {
            id: 12345,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        assert_eq!(
            artist.picture_url(TidalArtistImageSize::Small),
            Some("https://resources.tidal.com/images/abc/def/ghi/160x160.jpg".to_string())
        );
        assert_eq!(
            artist.picture_url(TidalArtistImageSize::Medium),
            Some("https://resources.tidal.com/images/abc/def/ghi/320x320.jpg".to_string())
        );
        assert_eq!(
            artist.picture_url(TidalArtistImageSize::Large),
            Some("https://resources.tidal.com/images/abc/def/ghi/480x480.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_tidal_artist_picture_url_none_when_no_picture() {
        let artist = TidalArtist {
            id: 12345,
            picture: None,
            contains_cover: false,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        assert_eq!(artist.picture_url(TidalArtistImageSize::Max), None);
    }

    // Album cover URL tests
    #[test_log::test]
    fn test_tidal_album_cover_url_construction() {
        let album = TidalAlbum {
            id: 67890,
            artist: "Test Artist".to_string(),
            artist_id: 12345,
            album_type: TidalAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Test".to_string()),
            cover: Some("xyz-abc-def".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: Some("2024-01-01".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec!["LOSSLESS".to_string()],
        };

        let url = album.cover_url(TidalAlbumImageSize::Max);
        assert_eq!(
            url,
            Some("https://resources.tidal.com/images/xyz/abc/def/1280x1280.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_tidal_album_cover_url_different_sizes() {
        let album = TidalAlbum {
            id: 67890,
            artist: "Test Artist".to_string(),
            artist_id: 12345,
            album_type: TidalAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            cover: Some("xyz-abc-def".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: None,
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(
            album.cover_url(TidalAlbumImageSize::Thumbnail),
            Some("https://resources.tidal.com/images/xyz/abc/def/80x80.jpg".to_string())
        );
        assert_eq!(
            album.cover_url(TidalAlbumImageSize::Small),
            Some("https://resources.tidal.com/images/xyz/abc/def/160x160.jpg".to_string())
        );
        assert_eq!(
            album.cover_url(TidalAlbumImageSize::Medium),
            Some("https://resources.tidal.com/images/xyz/abc/def/320x320.jpg".to_string())
        );
        assert_eq!(
            album.cover_url(TidalAlbumImageSize::Large),
            Some("https://resources.tidal.com/images/xyz/abc/def/640x640.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_tidal_album_cover_url_none_when_no_cover() {
        let album = TidalAlbum {
            id: 67890,
            artist: "Test Artist".to_string(),
            artist_id: 12345,
            album_type: TidalAlbumType::Lp,
            contains_cover: false,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            cover: None,
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: None,
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(album.cover_url(TidalAlbumImageSize::Max), None);
    }

    #[test_log::test]
    fn test_tidal_search_album_cover_url_construction() {
        let album = TidalSearchAlbum {
            id: 67890,
            artists: vec![],
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            cover: Some("xyz-abc-def".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: None,
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        let url = album.cover_url(TidalAlbumImageSize::Large);
        assert_eq!(
            url,
            Some("https://resources.tidal.com/images/xyz/abc/def/640x640.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_tidal_search_artist_picture_url_construction() {
        let artist = TidalSearchArtist {
            id: 12345,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            r#type: "ARTIST".to_string(),
            name: "Test Artist".to_string(),
        };

        let url = artist.picture_url(TidalArtistImageSize::Medium);
        assert_eq!(
            url,
            Some("https://resources.tidal.com/images/abc/def/ghi/320x320.jpg".to_string())
        );
    }

    // Search results position calculation tests
    #[test_log::test]
    fn test_search_results_position_calculation_within_bounds() {
        let search_results = TidalSearchResults {
            albums: TidalSearchResultList {
                items: vec![],
                offset: 10,
                limit: 5,
                total: 100,
            },
            artists: TidalSearchResultList {
                items: vec![],
                offset: 10,
                limit: 5,
                total: 50,
            },
            tracks: TidalSearchResultList {
                items: vec![],
                offset: 10,
                limit: 5,
                total: 75,
            },
            offset: 10,
            limit: 5,
        };

        let api_response: ApiSearchResultsResponse = search_results.into();
        assert_eq!(api_response.position, 15); // offset (10) + limit (5)
    }

    #[test_log::test]
    fn test_search_results_position_calculation_exceeds_total() {
        let search_results = TidalSearchResults {
            albums: TidalSearchResultList {
                items: vec![],
                offset: 95,
                limit: 10,
                total: 100,
            },
            artists: TidalSearchResultList {
                items: vec![],
                offset: 95,
                limit: 10,
                total: 50,
            },
            tracks: TidalSearchResultList {
                items: vec![],
                offset: 95,
                limit: 10,
                total: 75,
            },
            offset: 95,
            limit: 10,
        };

        let api_response: ApiSearchResultsResponse = search_results.into();
        assert_eq!(api_response.position, 100); // Capped at total (100)
    }

    #[test_log::test]
    fn test_search_results_position_calculation_at_start() {
        let search_results = TidalSearchResults {
            albums: TidalSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 100,
            },
            artists: TidalSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 50,
            },
            tracks: TidalSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 75,
            },
            offset: 0,
            limit: 10,
        };

        let api_response: ApiSearchResultsResponse = search_results.into();
        assert_eq!(api_response.position, 10); // offset (0) + limit (10)
    }

    // Model conversion tests
    #[test_log::test]
    fn test_tidal_artist_to_artist_conversion() {
        let tidal_artist = TidalArtist {
            id: 12345,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        let artist: Artist = tidal_artist.into();
        assert_eq!(artist.id, Id::from(12345_u64));
        assert_eq!(artist.title, "Test Artist");
        assert_eq!(
            artist.cover,
            Some("https://resources.tidal.com/images/abc/def/ghi/750x750.jpg".to_string())
        );
        assert_eq!(artist.api_source, *API_SOURCE);
    }

    #[test_log::test]
    fn test_tidal_artist_to_api_artist_conversion() {
        let tidal_artist = TidalArtist {
            id: 12345,
            picture: None,
            contains_cover: false,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        let api_artist: ApiArtist = tidal_artist.into();
        assert_eq!(api_artist.artist_id, Id::from(12345_u64));
        assert_eq!(api_artist.title, "Test Artist");
        assert!(!api_artist.contains_cover);
        assert_eq!(api_artist.api_source, *API_SOURCE);
    }

    #[test_log::test]
    fn test_tidal_track_to_track_conversion() {
        let tidal_track = TidalTrack {
            id: 98765,
            track_number: 3,
            artist_id: 12345,
            artist: "Test Artist".to_string(),
            artist_cover: Some("abc-def-ghi".to_string()),
            album_id: 67890,
            album_type: TidalAlbumType::Lp,
            album: "Test Album".to_string(),
            album_cover: Some("xyz-abc-def".to_string()),
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            duration: 180,
            explicit: false,
            isrc: "USRC12345678".to_string(),
            popularity: 85,
            title: "Test Track".to_string(),
            media_metadata_tags: vec!["LOSSLESS".to_string()],
        };

        let track: Track = tidal_track.into();
        assert_eq!(track.id, Id::from(98765_u64));
        assert_eq!(track.number, 3);
        assert_eq!(track.title, "Test Track");
        assert!((track.duration - 180.0).abs() < f64::EPSILON);
        assert_eq!(track.album, "Test Album");
        assert_eq!(track.album_id, Id::from(67890_u64));
        assert_eq!(track.artist, "Test Artist");
        assert_eq!(track.artist_id, Id::from(12345_u64));
        assert_eq!(track.api_source, *API_SOURCE);
    }

    #[test_log::test]
    fn test_tidal_album_to_album_conversion_with_release_date() {
        let tidal_album = TidalAlbum {
            id: 67890,
            artist: "Test Artist".to_string(),
            artist_id: 12345,
            album_type: TidalAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Test".to_string()),
            cover: Some("xyz-abc-def".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: Some("2024-01-15".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec!["LOSSLESS".to_string()],
        };

        let album: Album = tidal_album.try_into().unwrap();
        assert_eq!(album.id, Id::from(67890_u64));
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.artist, "Test Artist");
        assert_eq!(album.artist_id, Id::from(12345_u64));
        assert!(album.date_released.is_some());
        assert_eq!(
            album.artwork,
            Some("https://resources.tidal.com/images/xyz/abc/def/1280x1280.jpg".to_string())
        );
        assert_eq!(album.api_source, *API_SOURCE);
    }

    #[test_log::test]
    fn test_tidal_album_to_album_conversion_without_release_date() {
        let tidal_album = TidalAlbum {
            id: 67890,
            artist: "Test Artist".to_string(),
            artist_id: 12345,
            album_type: TidalAlbumType::EpsAndSingles,
            contains_cover: false,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: None,
            duration: 1800,
            explicit: true,
            number_of_tracks: 5,
            popularity: 50,
            release_date: None,
            title: "Test EP".to_string(),
            media_metadata_tags: vec![],
        };

        let album: Album = tidal_album.try_into().unwrap();
        assert_eq!(album.id, Id::from(67890_u64));
        assert!(album.date_released.is_none());
        assert_eq!(album.artwork, None);
    }

    #[test_log::test]
    fn test_album_to_tidal_album_roundtrip() {
        let original_album = Album {
            id: Id::from(67890_u64),
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::from(12345_u64),
            album_type: moosicbox_music_models::AlbumType::Lp,
            date_released: None,
            date_added: None,
            artwork: Some("https://example.com/cover.jpg".to_string()),
            directory: None,
            blur: false,
            versions: vec![],
            album_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            artist_sources: moosicbox_music_models::ApiSources::default(),
            album_sources: moosicbox_music_models::ApiSources::default(),
        };

        let tidal_album: TidalAlbum = original_album.try_into().unwrap();
        assert_eq!(tidal_album.id, 67890);
        assert_eq!(tidal_album.title, "Test Album");
        assert_eq!(tidal_album.artist, "Test Artist");
        assert_eq!(tidal_album.artist_id, 12345);
        assert_eq!(tidal_album.album_type, TidalAlbumType::Lp);
        assert!(tidal_album.contains_cover);
    }

    #[test_log::test]
    fn test_tidal_artist_to_search_result_conversion() {
        let tidal_artist = TidalArtist {
            id: 12345,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            popularity: 80,
            name: "Test Artist".to_string(),
        };

        let search_result: ApiGlobalSearchResult = tidal_artist.into();
        match search_result {
            ApiGlobalSearchResult::Artist(result) => {
                assert_eq!(result.artist_id, Id::from(12345_u64));
                assert_eq!(result.title, "Test Artist");
                assert!(result.contains_cover);
                assert_eq!(result.api_source, *API_SOURCE);
            }
            _ => panic!("Expected Artist search result"),
        }
    }

    #[test_log::test]
    fn test_tidal_search_artist_to_search_result_conversion() {
        let tidal_search_artist = TidalSearchArtist {
            id: 12345,
            picture: None,
            contains_cover: false,
            r#type: "ARTIST".to_string(),
            name: "Test Artist".to_string(),
        };

        let search_result: ApiGlobalSearchResult = tidal_search_artist.into();
        match search_result {
            ApiGlobalSearchResult::Artist(result) => {
                assert_eq!(result.artist_id, Id::from(12345_u64));
                assert_eq!(result.title, "Test Artist");
                assert!(!result.contains_cover);
            }
            _ => panic!("Expected Artist search result"),
        }
    }

    #[test_log::test]
    fn test_tidal_search_album_to_search_result_conversion() {
        let tidal_search_album = TidalSearchAlbum {
            id: 67890,
            artists: vec![TidalSearchArtist {
                id: 12345,
                picture: None,
                contains_cover: false,
                r#type: "ARTIST".to_string(),
                name: "Test Artist".to_string(),
            }],
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            cover: Some("xyz-abc-def".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: Some("2024-01-15".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        let search_result: ApiGlobalSearchResult = tidal_search_album.into();
        match search_result {
            ApiGlobalSearchResult::Album(result) => {
                assert_eq!(result.album_id, Id::from(67890_u64));
                assert_eq!(result.title, "Test Album");
                assert_eq!(result.artist_id, Id::from(12345_u64));
                assert_eq!(result.artist, "Test Artist");
                assert!(result.contains_cover);
            }
            _ => panic!("Expected Album search result"),
        }
    }

    #[test_log::test]
    fn test_tidal_search_track_to_search_result_conversion() {
        let tidal_search_track = TidalSearchTrack {
            id: 98765,
            track_number: 3,
            artists: vec![TidalSearchArtist {
                id: 12345,
                picture: None,
                contains_cover: false,
                r#type: "ARTIST".to_string(),
                name: "Test Artist".to_string(),
            }],
            artist_cover: None,
            album_id: 67890,
            album: "Test Album".to_string(),
            album_cover: Some("xyz-abc-def".to_string()),
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            duration: 180,
            explicit: false,
            isrc: "USRC12345678".to_string(),
            popularity: 85,
            title: "Test Track".to_string(),
            media_metadata_tags: vec![],
        };

        let search_result: ApiGlobalSearchResult = tidal_search_track.into();
        match search_result {
            ApiGlobalSearchResult::Track(result) => {
                assert_eq!(result.track_id, Id::from(98765_u64));
                assert_eq!(result.title, "Test Track");
                assert_eq!(result.artist_id, Id::from(12345_u64));
                assert_eq!(result.artist, "Test Artist");
                assert_eq!(result.album_id, Id::from(67890_u64));
                assert_eq!(result.album, "Test Album");
                assert!(result.contains_cover);
            }
            _ => panic!("Expected Track search result"),
        }
    }

    #[test_log::test]
    fn test_tidal_search_track_without_album_cover() {
        let tidal_search_track = TidalSearchTrack {
            id: 98765,
            track_number: 1,
            artists: vec![TidalSearchArtist {
                id: 12345,
                picture: None,
                contains_cover: false,
                r#type: "ARTIST".to_string(),
                name: "Test Artist".to_string(),
            }],
            artist_cover: None,
            album_id: 67890,
            album: "Test Album".to_string(),
            album_cover: None,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 240,
            explicit: true,
            isrc: "USRC87654321".to_string(),
            popularity: 50,
            title: "Test Track 2".to_string(),
            media_metadata_tags: vec![],
        };

        let search_result: ApiGlobalSearchResult = tidal_search_track.into();
        match search_result {
            ApiGlobalSearchResult::Track(result) => {
                assert!(!result.contains_cover);
            }
            _ => panic!("Expected Track search result"),
        }
    }

    #[test_log::test]
    fn test_search_results_concatenates_all_types() {
        let search_results = TidalSearchResults {
            albums: TidalSearchResultList {
                items: vec![TidalSearchAlbum {
                    id: 1,
                    artists: vec![TidalSearchArtist {
                        id: 100,
                        picture: None,
                        contains_cover: false,
                        r#type: "ARTIST".to_string(),
                        name: "Artist 1".to_string(),
                    }],
                    contains_cover: true,
                    audio_quality: "LOSSLESS".to_string(),
                    copyright: None,
                    cover: None,
                    duration: 3600,
                    explicit: false,
                    number_of_tracks: 10,
                    popularity: 80,
                    release_date: None,
                    title: "Album 1".to_string(),
                    media_metadata_tags: vec![],
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
            artists: TidalSearchResultList {
                items: vec![TidalArtist {
                    id: 200,
                    picture: None,
                    contains_cover: false,
                    popularity: 70,
                    name: "Artist 2".to_string(),
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
            tracks: TidalSearchResultList {
                items: vec![TidalSearchTrack {
                    id: 300,
                    track_number: 1,
                    artists: vec![TidalSearchArtist {
                        id: 100,
                        picture: None,
                        contains_cover: false,
                        r#type: "ARTIST".to_string(),
                        name: "Artist 1".to_string(),
                    }],
                    artist_cover: None,
                    album_id: 1,
                    album: "Album 1".to_string(),
                    album_cover: None,
                    audio_quality: "LOSSLESS".to_string(),
                    copyright: None,
                    duration: 180,
                    explicit: false,
                    isrc: "TEST12345".to_string(),
                    popularity: 90,
                    title: "Track 1".to_string(),
                    media_metadata_tags: vec![],
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
            offset: 0,
            limit: 10,
        };

        let api_response: ApiSearchResultsResponse = search_results.into();
        assert_eq!(api_response.results.len(), 3);
    }

    #[test_log::test]
    fn test_tidal_search_album_cover_url_none_when_no_cover() {
        let album = TidalSearchAlbum {
            id: 67890,
            artists: vec![],
            contains_cover: false,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: None,
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 90,
            release_date: None,
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(album.cover_url(TidalAlbumImageSize::Max), None);
    }

    #[test_log::test]
    fn test_tidal_search_artist_picture_url_none_when_no_picture() {
        let artist = TidalSearchArtist {
            id: 12345,
            picture: None,
            contains_cover: false,
            r#type: "ARTIST".to_string(),
            name: "Test Artist".to_string(),
        };

        assert_eq!(artist.picture_url(TidalArtistImageSize::Max), None);
    }
}
