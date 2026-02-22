//! Data models for `YouTube` Music API entities.
//!
//! This module contains type definitions for artists, albums, tracks, search results,
//! and internal API response structures used when interacting with `YouTube` Music.

use std::fmt::Display;

use moosicbox_date_utils::chrono::parse_date_time;
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::AsModelResult,
    serde_json::{ToNestedValue as _, ToValue as _},
};
use moosicbox_music_api::models::search::api::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
};
use moosicbox_music_models::{
    Album, ApiSources, Artist, Track,
    api::{ApiAlbum, ApiArtist},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{API_SOURCE, YtAlbumType};

/// `YouTube` Music artist entity.
///
/// Represents an artist from the `YouTube` Music API with cover image and popularity information.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtArtist {
    /// `YouTube` Music artist ID
    pub id: String,
    /// Artist profile picture URL
    pub picture: Option<String>,
    /// Whether the artist has a cover image
    pub contains_cover: bool,
    /// Artist popularity score
    pub popularity: u32,
    /// Artist name
    pub name: String,
}

impl From<YtArtist> for ApiGlobalSearchResult {
    fn from(value: YtArtist) -> Self {
        Self::Artist(ApiGlobalArtistSearchResult {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.contains_cover,
            blur: false,
            api_source: API_SOURCE.clone(),
        })
    }
}

impl From<YtArtist> for Artist {
    fn from(value: YtArtist) -> Self {
        Self {
            id: value.id.as_str().into(),
            title: value.name,
            cover: value.picture,
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<YtArtist> for ApiArtist {
    fn from(value: YtArtist) -> Self {
        Self {
            artist_id: value.id.clone().into(),
            title: value.name,
            contains_cover: value.contains_cover,
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

/// Size options for `YouTube` Music artist profile images.
///
/// Different resolution options for retrieving artist thumbnails, ranging from 160px to 750px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YtArtistImageSize {
    /// Maximum resolution (750x750 pixels)
    Max, // 750
    /// Large resolution (480x480 pixels)
    Large, // 480
    /// Medium resolution (320x320 pixels)
    Medium, // 320
    /// Small resolution (160x160 pixels)
    Small, // 160
}

impl From<YtArtistImageSize> for u16 {
    fn from(value: YtArtistImageSize) -> Self {
        match value {
            YtArtistImageSize::Max => 750,
            YtArtistImageSize::Large => 480,
            YtArtistImageSize::Medium => 320,
            YtArtistImageSize::Small => 160,
        }
    }
}

impl From<u16> for YtArtistImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=160 => Self::Small,
            161..=320 => Self::Medium,
            321..=480 => Self::Large,
            _ => Self::Max,
        }
    }
}

impl Display for YtArtistImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl ToValueType<YtArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(YtArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            popularity: self.to_value("popularity")?,
            name: self.to_value("name")?,
        })
    }
}

/// `YouTube` Music artist entity from search results.
///
/// Represents an artist returned from search operations, with slightly different fields
/// than the standard `YtArtist` type.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchArtist {
    /// `YouTube` Music artist ID
    pub id: u64,
    /// Artist profile picture URL
    pub picture: Option<String>,
    /// Whether the artist has a cover image
    pub contains_cover: bool,
    /// Artist type classification
    pub r#type: String,
    /// Artist name
    pub name: String,
}

impl YtSearchArtist {
    /// Constructs the full URL for the artist's profile picture at the specified size.
    ///
    /// Returns `None` if the artist has no profile picture.
    #[must_use]
    pub fn picture_url(&self, size: YtArtistImageSize) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.yt.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtSearchArtist> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchArtist, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchArtist, ParseError> {
        let picture: Option<String> = self.to_value("picture")?;

        Ok(YtSearchArtist {
            id: self.to_value("id")?,
            contains_cover: picture.is_some(),
            picture,
            r#type: self.to_value("type")?,
            name: self.to_value("name")?,
        })
    }
}

/// `YouTube` Music album entity.
///
/// Represents a complete album from the `YouTube` Music API with metadata including
/// audio quality, duration, and track information.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtAlbum {
    /// `YouTube` Music album ID
    pub id: String,
    /// Album artist name
    pub artist: String,
    /// `YouTube` Music artist ID
    pub artist_id: String,
    /// Album type classification (LP, EPs/singles, compilations)
    pub album_type: YtAlbumType,
    /// Whether the album has cover artwork
    pub contains_cover: bool,
    /// Audio quality level (e.g., "LOSSLESS", "HIGH")
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Album cover artwork URL
    pub cover: Option<String>,
    /// Total album duration in seconds
    pub duration: u32,
    /// Whether the album contains explicit content
    pub explicit: bool,
    /// Number of tracks in the album
    pub number_of_tracks: u32,
    /// Album popularity score
    pub popularity: u32,
    /// Release date (ISO 8601 format)
    pub release_date: Option<String>,
    /// Album title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
}

impl TryFrom<YtAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: YtAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.as_str().into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.as_str().into(),
            album_type: value.album_type.into(),
            date_released: value
                .release_date
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            date_added: None,
            artwork: value.cover,
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

impl TryFrom<YtAlbum> for ApiAlbum {
    type Error = <YtAlbum as TryInto<Album>>::Error;

    fn try_from(value: YtAlbum) -> Result<Self, Self::Error> {
        let album: Album = value.try_into()?;
        Ok(album.into())
    }
}

impl From<YtAlbum> for ApiGlobalSearchResult {
    fn from(value: YtAlbum) -> Self {
        Self::Album(ApiGlobalAlbumSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
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

/// Size options for `YouTube` Music album cover images.
///
/// Different resolution options for retrieving album artwork, ranging from 80px thumbnails to 1280px maximum resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YtAlbumImageSize {
    /// Maximum resolution (1280x1280 pixels)
    Max, // 1280
    /// Large resolution (640x640 pixels)
    Large, // 640
    /// Medium resolution (320x320 pixels)
    Medium, // 320
    /// Small resolution (160x160 pixels)
    Small, // 160
    /// Thumbnail resolution (80x80 pixels)
    Thumbnail, // 80
}

impl From<YtAlbumImageSize> for u16 {
    fn from(value: YtAlbumImageSize) -> Self {
        match value {
            YtAlbumImageSize::Max => 1280,
            YtAlbumImageSize::Large => 640,
            YtAlbumImageSize::Medium => 320,
            YtAlbumImageSize::Small => 160,
            YtAlbumImageSize::Thumbnail => 80,
        }
    }
}

impl From<u16> for YtAlbumImageSize {
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

impl Display for YtAlbumImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

/// `YouTube` Music album entity from search results.
///
/// Represents an album returned from search operations, with multiple artist associations.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchAlbum {
    /// `YouTube` Music album ID
    pub id: u64,
    /// List of artists associated with this album
    pub artists: Vec<YtSearchArtist>,
    /// Whether the album has cover artwork
    pub contains_cover: bool,
    /// Audio quality level
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Album cover artwork URL
    pub cover: Option<String>,
    /// Total album duration in seconds
    pub duration: u32,
    /// Whether the album contains explicit content
    pub explicit: bool,
    /// Number of tracks in the album
    pub number_of_tracks: u32,
    /// Album popularity score
    pub popularity: u32,
    /// Release date (ISO 8601 format)
    pub release_date: Option<String>,
    /// Album title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
}

impl YtSearchAlbum {
    /// Constructs the full URL for the album's cover artwork at the specified size.
    ///
    /// Returns `None` if the album has no cover artwork.
    #[must_use]
    pub fn cover_url(&self, size: YtAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.yt.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtSearchAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchAlbum, ParseError> {
        Ok(YtSearchAlbum {
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

impl YtAlbum {
    /// Constructs the full URL for the album's cover artwork at the specified size.
    ///
    /// Returns `None` if the album has no cover artwork.
    #[must_use]
    pub fn cover_url(&self, size: YtAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.yt.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl ToValueType<YtAlbum> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtAlbum, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtAlbum, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtAlbum, ParseError> {
        Ok(YtAlbum {
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

/// `YouTube` Music track entity.
///
/// Represents a music track from the `YouTube` Music API with complete metadata including
/// album, artist, and audio quality information.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtTrack {
    /// `YouTube` Music track ID
    pub id: String,
    /// Track number within the album
    pub track_number: u32,
    /// `YouTube` Music artist ID
    pub artist_id: String,
    /// Artist name
    pub artist: String,
    /// Artist profile image URL
    pub artist_cover: Option<String>,
    /// `YouTube` Music album ID
    pub album_id: String,
    /// Album title
    pub album: String,
    /// Album type classification
    pub album_type: YtAlbumType,
    /// Album cover artwork URL
    pub album_cover: Option<String>,
    /// Audio quality level
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Track duration in seconds
    pub duration: u32,
    /// Whether the track contains explicit content
    pub explicit: bool,
    /// International Standard Recording Code
    pub isrc: String,
    /// Track popularity score
    pub popularity: u32,
    /// Track title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
}

impl From<YtTrack> for Track {
    fn from(value: YtTrack) -> Self {
        Self {
            id: value.id.as_str().into(),
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
            artwork: value.album_cover,
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

impl From<YtTrack> for ApiGlobalSearchResult {
    fn from(value: YtTrack) -> Self {
        Self::Track(ApiGlobalTrackSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
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

impl ToValueType<YtTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtTrack, ParseError> {
        Ok(YtTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            album_type: self.to_nested_value(&["album", "type"])?,
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

/// `YouTube` Music video entity.
///
/// Represents a music video (user-generated content) distinct from official track recordings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtVideo {
    /// `YouTube` video ID
    pub id: String,
    /// `YouTube` Music artist ID
    pub artist_id: u64,
    /// Artist name
    pub artist: String,
    /// Artist profile image URL
    pub artist_cover: Option<String>,
    /// `YouTube` Music album ID
    pub album_id: u64,
    /// Album title
    pub album: String,
    /// Album cover artwork URL
    pub album_cover: Option<String>,
    /// Audio quality level
    pub audio_quality: String,
    /// Video duration in seconds
    pub duration: u32,
    /// Whether the video contains explicit content
    pub explicit: bool,
    /// Video title
    pub title: String,
}

impl ToValueType<YtVideo> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtVideo, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtVideo, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtVideo, ParseError> {
        Ok(YtVideo {
            id: self.to_value("id")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
            artist_cover: self.to_nested_value(&["artist", "picture"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            album: self.to_nested_value(&["album", "title"])?,
            album_cover: self.to_nested_value(&["album", "cover"])?,
            audio_quality: self.to_value("audioQuality")?,
            duration: self.to_value("duration")?,
            explicit: self.to_value("explicit")?,
            title: self.to_value("title")?,
        })
    }
}

/// `YouTube` Music track entity from search results.
///
/// Represents a track returned from search operations, with multiple artist associations.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchTrack {
    /// `YouTube` Music track ID
    pub id: u64,
    /// Track number within the album
    pub track_number: u32,
    /// List of artists associated with this track
    pub artists: Vec<YtSearchArtist>,
    /// Artist profile image URL
    pub artist_cover: Option<String>,
    /// `YouTube` Music album ID
    pub album_id: u64,
    /// Album title
    pub album: String,
    /// Album cover artwork URL
    pub album_cover: Option<String>,
    /// Audio quality level
    pub audio_quality: String,
    /// Copyright information
    pub copyright: Option<String>,
    /// Track duration in seconds
    pub duration: u32,
    /// Whether the track contains explicit content
    pub explicit: bool,
    /// International Standard Recording Code
    pub isrc: String,
    /// Track popularity score
    pub popularity: u32,
    /// Track title
    pub title: String,
    /// Media metadata tags
    pub media_metadata_tags: Vec<String>,
}

impl ToValueType<YtSearchTrack> for &serde_json::Value {
    fn to_value_type(self) -> Result<YtSearchTrack, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchTrack, ParseError> for serde_json::Value {
    fn as_model(&self) -> Result<YtSearchTrack, ParseError> {
        Ok(YtSearchTrack {
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

/// Generic paginated search result list from `YouTube` Music API.
///
/// Container for any type of search results with pagination information.
#[derive(Debug, Serialize, Deserialize)]
pub struct YtSearchResultList<T> {
    /// List of result items
    pub items: Vec<T>,
    /// Pagination offset
    pub offset: usize,
    /// Maximum number of results per page
    pub limit: usize,
    /// Total number of available results
    pub total: usize,
}

impl<T> ToValueType<YtSearchResultList<T>> for &Value
where
    Value: AsModelResult<YtSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<YtSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<YtSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Self: ToValueType<T>,
    for<'a> &'a Self: ToValueType<usize>,
{
    fn as_model(&self) -> Result<YtSearchResultList<T>, ParseError> {
        Ok(YtSearchResultList {
            items: self.to_value("items")?,
            offset: self.to_value("offset")?,
            limit: self.to_value("limit")?,
            total: self.to_value("totalNumberOfItems")?,
        })
    }
}

/// `YouTube` Music search result item renderer (internal API structure).
///
/// Represents a single item in the search results list from the `YouTube` Music API response.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRenderer {
    tracking_params: String,
    thumbnail: YtSearchResultsContentsListItemRendererThumbnail,
    flex_columns: Vec<YtSearchResultsContentsListItemRendererFlexColumns>,
    menu: YtSearchResultsContentsListItemRendererMenu,
    flex_column_display_style: String,
    navigation_endpoint: YtSearchResultsContentsSearchRendererRunNavigationEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRenderer {
            tracking_params: self.to_value("trackingParams")?,
            thumbnail: self.to_value("thumbnail")?,
            flex_columns: self.to_value("flexColumns")?,
            menu: self.to_value("menu")?,
            flex_column_display_style: self.to_value("flexColumnDisplayStyle")?,
            navigation_endpoint: self.to_value("navigationEndpoint")?,
        })
    }
}

/// Flex columns container for search result item renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererFlexColumns {
    music_responsive_list_item_flex_column_renderer:
        YtSearchResultsContentsListItemRendererFlexColumnsRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererFlexColumns> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererFlexColumns, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererFlexColumns {
            music_responsive_list_item_flex_column_renderer: self
                .to_value("musicResponsiveListItemFlexColumnRenderer")?,
        })
    }
}

/// Flex column renderer for search result items (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererFlexColumnsRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    display_priority: String,
}

impl ToValueType<YtSearchResultsContentsListItemRendererFlexColumnsRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererFlexColumnsRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererFlexColumnsRenderer {
            text: self.to_value("text")?,
            display_priority: self.to_value("displayPriority")?,
        })
    }
}

/// Menu container for search result item (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenu {
    menu_renderer: YtSearchResultsContentsListItemRendererMenuRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenu> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRendererMenu, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenu, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererMenu, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenu {
            menu_renderer: self.to_value("menuRenderer")?,
        })
    }
}

/// Menu renderer for search result items (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRenderer {
    items: Vec<YtSearchResultsContentsListItemRendererMenuRendererItem>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererMenuRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenuRenderer {
            items: self.to_value("items")?,
        })
    }
}

/// Menu item in search result renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    menu_navigation_item_renderer:
        Option<YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    menu_service_item_renderer:
        Option<YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItem> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererMenuRendererItem, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererMenuRendererItem {
            menu_navigation_item_renderer: self.to_value("menuNavigationItemRenderer")?,
            menu_service_item_renderer: self.to_value("menuServiceItemRenderer")?,
        })
    }
}

/// Navigation menu item renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
    icon: YtSearchResultsContentsSearchRendererIcon,
    navigation_endpoint: YtSearchResultsContentsSearchRendererRunNavigationEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsListItemRendererMenuRendererItemNavigationItemRenderer {
                text: self.to_value("text")?,
                tracking_params: self.to_value("trackingParams")?,
                icon: self.to_value("icon")?,
                navigation_endpoint: self.to_value("navigationEndpoint")?,
            },
        )
    }
}

/// Service menu item renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer {
    text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
    icon: YtSearchResultsContentsSearchRendererIcon,
    service_endpoint: YtSearchResultsContentsSearchRendererRunServiceEndpoint,
}

impl ToValueType<YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsListItemRendererMenuRendererItemServiceItemRenderer {
                text: self.to_value("text")?,
                tracking_params: self.to_value("trackingParams")?,
                icon: self.to_value("icon")?,
                service_endpoint: self.to_value("serviceEndpoint")?,
            },
        )
    }
}

/// Thumbnail container for search result items (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnail {
    music_thumbnail_renderer: YtSearchResultsContentsListItemRendererThumbnailRenderer,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnail> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsListItemRendererThumbnail, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnail, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsListItemRendererThumbnail, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererThumbnail {
            music_thumbnail_renderer: self.to_value("musicThumbnailRenderer")?,
        })
    }
}

/// Thumbnail renderer for search result items (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRenderer {
    thumbnail: YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail,
    thumbnail_crop: String,
    thumbnail_scale: String,
    tracking_params: String,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRenderer, ParseError> {
        Ok(YtSearchResultsContentsListItemRendererThumbnailRenderer {
            thumbnail: self.to_value("thumbnail")?,
            thumbnail_crop: self.to_value("thumbnailCrop")?,
            thumbnail_scale: self.to_value("thumbnailScale")?,
            tracking_params: self.to_value("trackingParams")?,
        })
    }
}

/// Thumbnail data container (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail {
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnails: Option<Vec<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData>>,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail, ParseError> {
        Ok(
            YtSearchResultsContentsListItemRendererThumbnailRendererThumbnail {
                thumbnails: self.to_value("thumbnails")?,
            },
        )
    }
}

/// Individual thumbnail image data with dimensions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData {
    url: String,
    width: u16,
    height: u16,
}

impl ToValueType<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData, ParseError>
    {
        Ok(
            YtSearchResultsContentsListItemRendererThumbnailRendererThumbnailData {
                url: self.to_value("url")?,
                width: self.to_value("width")?,
                height: self.to_value("height")?,
            },
        )
    }
}

/// Search suggestion renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchSuggestionRenderer {
    suggestion: YtSearchResultsContentsSearchRendererRuns,
}

impl ToValueType<YtSearchResultsContentsSearchSuggestionRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchSuggestionRenderer, ParseError> {
        Ok(YtSearchResultsContentsSearchSuggestionRenderer {
            suggestion: self.to_value("suggestion")?,
        })
    }
}

/// Text runs container for search results (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRuns {
    #[serde(skip_serializing_if = "Option::is_none")]
    runs: Option<Vec<YtSearchResultsContentsSearchRendererRun>>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRuns> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererRuns, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRuns, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererRuns, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRuns {
            runs: self.to_value("runs")?,
        })
    }
}

/// Individual text run with optional formatting and navigation (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRun {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    navigation_endpoint: Option<YtSearchResultsContentsSearchRendererRunNavigationEndpoint>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRun> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererRun, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRun, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererRun, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRun {
            text: self.to_value("text")?,
            bold: self.to_value("bold")?,
            navigation_endpoint: self.to_value("navigationEndpoint")?,
        })
    }
}

/// Icon data for search renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererIcon {
    icon_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererIcon> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSearchRendererIcon, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererIcon, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSearchRendererIcon, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererIcon {
            icon_type: self.to_value("iconType")?,
        })
    }
}

/// Navigation endpoint for search results (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpoint {
    click_tracking_params: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    browse_endpoint:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_endpoint: Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_playlist_endpoint:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpoint> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpoint, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRunNavigationEndpoint {
            click_tracking_params: self.to_value("clickTrackingParams")?,
            browse_endpoint: self.to_value("browseEndpoint")?,
            watch_endpoint: self.to_value("watchEndpoint")?,
            watch_playlist_endpoint: self.to_value("watchPlaylistEndpoint")?,
        })
    }
}

/// Service endpoint for search renderer actions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpoint {
    click_tracking_params: String,
    queue_add_endpoint: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpoint> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpoint, ParseError> {
        Ok(YtSearchResultsContentsSearchRendererRunServiceEndpoint {
            click_tracking_params: self.to_value("clickTrackingParams")?,
            queue_add_endpoint: self.to_value("queueAddEndpoint")?,
        })
    }
}

/// Queue add endpoint for service actions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint {
    queue_target: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget,
    queue_insert_position: String,
    commands: Vec<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddEndpoint {
                queue_target: self.to_value("queueTarget")?,
                queue_insert_position: self.to_value("queueInsertPosition")?,
                commands: self.to_value("commands")?,
            },
        )
    }
}

/// Queue target configuration (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    on_empty_queue: YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueTarget {
                playlist_id: self.to_value("playlistId")?,
                on_empty_queue: self.to_value("onEmptyQueue")?,
            },
        )
    }
}

/// Queue command for service endpoint (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand {
    click_tracking_params: String,
    add_to_toast_action:
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueCommand {
                click_tracking_params: self.to_value("clickTrackingParams")?,
                add_to_toast_action: self.to_value("addToToastAction")?,
            },
        )
    }
}

/// Toast action for queue additions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction {
    item: YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastAction {
                item: self.to_value("item")?,
            },
        )
    }
}

/// Toast action item with notification renderer (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem {
    notification_text_renderer: YtSearchResultsContentsSearchNotificationTextRenderer,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunServiceEndpointQueueAddToToastActionItem {
                notification_text_renderer: self.to_value("notificationTextRenderer")?,
            },
        )
    }
}

/// Notification text renderer for search actions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchNotificationTextRenderer {
    success_response_text: YtSearchResultsContentsSearchRendererRuns,
    tracking_params: String,
}

impl ToValueType<YtSearchResultsContentsSearchNotificationTextRenderer> for &Value {
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> for Value {
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchNotificationTextRenderer, ParseError> {
        Ok(YtSearchResultsContentsSearchNotificationTextRenderer {
            success_response_text: self.to_value("successResponseText")?,
            tracking_params: self.to_value("trackingParams")?,
        })
    }
}

/// Browse endpoint for navigating to artists/albums (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint {
    browse_id: String,
    browse_endpoint_context_supported_configs:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpoint {
                browse_id: self.to_value("browseId")?,
                browse_endpoint_context_supported_configs: self
                    .to_value("browseEndpointContextSupportedConfigs")?,
            },
        )
    }
}

/// Watch endpoint for playing videos/tracks (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watch_endpoint_music_supported_configs:
        Option<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint, ParseError>
    {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint, ParseError>
    {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpoint {
                video_id: self.to_value("videoId")?,
                playlist_id: self.to_value("playlistId")?,
                params: self.to_value("params")?,
                watch_endpoint_music_supported_configs: self
                    .to_value("watchEndpointMusicSupportedConfigs")?,
            },
        )
    }
}

/// Watch playlist endpoint (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint {
    playlist_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<String>,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchPlaylistEndpoint {
                playlist_id: self.to_value("playlistId")?,
                params: self.to_value("params")?,
            },
        )
    }
}

/// Watch endpoint configuration (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs {
    watch_endpoint_music_config:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfigs {
                watch_endpoint_music_config: self.to_value("watchEndpointMusicConfig")?,
            },
        )
    }
}

/// Watch endpoint music configuration with video type (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig {
    music_video_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointWatchEndpointConfig {
                music_video_type: self.to_value("musicVideoType")?,
            },
        )
    }
}

/// Browse endpoint configuration (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs {
    browse_endpoint_context_music_config:
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfigs {
                browse_endpoint_context_music_config: self
                    .to_value("browseEndpointContextMusicConfig")?,
            },
        )
    }
}

/// Browse endpoint page type configuration (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig {
    page_type: String,
}

impl ToValueType<YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig>
    for &Value
{
    fn to_value_type(
        self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > {
        self.as_model()
    }
}

impl
    AsModelResult<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > for Value
{
    fn as_model(
        &self,
    ) -> Result<
        YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig,
        ParseError,
    > {
        Ok(
            YtSearchResultsContentsSearchRendererRunNavigationEndpointBrowseEndpointConfig {
                page_type: self.to_value("pageType")?,
            },
        )
    }
}

/// Section renderer containing search results or suggestions (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSectionRenderer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_responsive_list_item_renderer: Option<YtSearchResultsContentsListItemRenderer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_suggestion_renderer: Option<YtSearchResultsContentsSearchSuggestionRenderer>,
}

impl ToValueType<YtSearchResultsContentsSectionRenderer> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSectionRenderer, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSectionRenderer, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSectionRenderer, ParseError> {
        Ok(YtSearchResultsContentsSectionRenderer {
            music_responsive_list_item_renderer: self
                .to_value("musicResponsiveListItemRenderer")?,
            search_suggestion_renderer: self.to_value("searchSuggestionRenderer")?,
        })
    }
}

/// Section containing multiple search result renderers (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContentsSection {
    pub contents: Vec<YtSearchResultsContentsSectionRenderer>,
}

impl ToValueType<YtSearchResultsContentsSection> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContentsSection, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContentsSection, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContentsSection, ParseError> {
        Ok(YtSearchResultsContentsSection {
            contents: self.to_value("contents")?,
        })
    }
}

/// Search results contents container (internal API structure).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsContents {
    pub search_suggestions_section_renderer: YtSearchResultsContentsSection,
}

impl ToValueType<YtSearchResultsContents> for &Value {
    fn to_value_type(self) -> Result<YtSearchResultsContents, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResultsContents, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResultsContents, ParseError> {
        Ok(YtSearchResultsContents {
            search_suggestions_section_renderer: self
                .to_value("searchSuggestionsSectionRenderer")?,
        })
    }
}

/// Raw search results from the `YouTube` Music API.
///
/// Contains the unparsed search response structure with pagination information.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResults {
    /// List of search result sections
    pub contents: Vec<YtSearchResultsContents>,
    /// Pagination offset (starting position)
    pub offset: usize,
    /// Maximum number of results per page
    pub limit: usize,
    /// Total number of available results
    pub total: usize,
}

impl ToValueType<YtSearchResults> for &Value {
    fn to_value_type(self) -> Result<YtSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResults, ParseError> {
        let contents: Option<Vec<YtSearchResultsContents>> = self.to_value("contents")?;
        let contents = contents.unwrap_or_default();
        let offset = 0;
        let limit = 3;
        let total = contents.len();

        Ok(YtSearchResults {
            contents,
            offset,
            limit,
            total,
        })
    }
}

fn track_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtTrack> {
    value
        .navigation_endpoint
        .watch_endpoint
        .as_ref()
        .and_then(|endpoint| {
            endpoint
                .watch_endpoint_music_supported_configs
                .as_ref()
                .and_then(|configs| {
                    if configs.watch_endpoint_music_config.music_video_type
                        == "MUSIC_VIDEO_TYPE_ATV"
                    {
                        let album = track_album_from_search_result(value);
                        Some(YtTrack {
                            id: endpoint.video_id.as_deref().unwrap_or("N/A").to_string(),
                            artist: album
                                .as_ref()
                                .map_or("N/A", |x| x.artist.as_str())
                                .to_string(),
                            artist_id: album
                                .as_ref()
                                .map_or("N/A", |x| x.artist_id.as_str())
                                .to_string(),
                            album: album
                                .as_ref()
                                .map_or("N/A", |x| x.title.as_str())
                                .to_string(),
                            album_id: album.as_ref().map_or("N/A", |x| x.id.as_str()).to_string(),
                            album_cover: value
                                .thumbnail
                                .music_thumbnail_renderer
                                .thumbnail
                                .thumbnails
                                .as_ref()
                                .and_then(|x| {
                                    x.iter()
                                        .max_by(|a, b| a.width.cmp(&b.width))
                                        .map(|x| x.url.clone())
                                }),
                            title: value
                                .flex_columns
                                .first()
                                .and_then(|x| {
                                    x.music_responsive_list_item_flex_column_renderer
                                        .text
                                        .runs
                                        .as_ref()
                                        .and_then(|x| x.first().map(|x| x.text.as_str()))
                                })
                                .unwrap_or("N/A")
                                .to_string(),
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                })
        })
}

fn video_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtVideo> {
    value
        .navigation_endpoint
        .watch_endpoint
        .as_ref()
        .and_then(|endpoint| {
            endpoint
                .watch_endpoint_music_supported_configs
                .as_ref()
                .and_then(|configs| {
                    if configs.watch_endpoint_music_config.music_video_type
                        == "MUSIC_VIDEO_TYPE_UGC"
                    {
                        Some(YtVideo {
                            id: endpoint.video_id.as_deref().unwrap_or("N/A").to_string(),
                            album_cover: value
                                .thumbnail
                                .music_thumbnail_renderer
                                .thumbnail
                                .thumbnails
                                .as_ref()
                                .and_then(|x| {
                                    x.iter()
                                        .max_by(|a, b| a.width.cmp(&b.width))
                                        .map(|x| x.url.clone())
                                }),
                            title: value
                                .flex_columns
                                .first()
                                .and_then(|x| {
                                    x.music_responsive_list_item_flex_column_renderer
                                        .text
                                        .runs
                                        .as_ref()
                                        .and_then(|x| x.first().map(|x| x.text.as_str()))
                                })
                                .unwrap_or("N/A")
                                .to_string(),
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                })
        })
}

fn artist_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtArtist> {
    value
        .navigation_endpoint
        .browse_endpoint
        .as_ref()
        .and_then(|endpoint| {
            if endpoint
                .browse_endpoint_context_supported_configs
                .browse_endpoint_context_music_config
                .page_type
                == "MUSIC_PAGE_TYPE_ARTIST"
            {
                Some(YtArtist {
                    id: endpoint.browse_id.clone(),
                    picture: value
                        .thumbnail
                        .music_thumbnail_renderer
                        .thumbnail
                        .thumbnails
                        .as_ref()
                        .and_then(|x| {
                            x.iter()
                                .max_by(|a, b| a.width.cmp(&b.width))
                                .map(|x| x.url.clone())
                        }),
                    contains_cover: value
                        .thumbnail
                        .music_thumbnail_renderer
                        .thumbnail
                        .thumbnails
                        .as_ref()
                        .is_some_and(|x| !x.is_empty()),
                    name: value
                        .flex_columns
                        .first()
                        .and_then(|x| {
                            x.music_responsive_list_item_flex_column_renderer
                                .text
                                .runs
                                .as_ref()
                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                        })
                        .unwrap_or("N/A")
                        .to_string(),
                    ..Default::default()
                })
            } else {
                None
            }
        })
}

fn album_artist_from_search_result(
    value: &YtSearchResultsContentsListItemRenderer,
) -> Option<YtArtist> {
    value.flex_columns.iter().find_map(|col| {
        col.music_responsive_list_item_flex_column_renderer
            .text
            .runs
            .as_ref()
            .and_then(|runs| {
                runs.iter().find_map(|run| {
                    run.navigation_endpoint.as_ref().and_then(|nav| {
                        nav.browse_endpoint.as_ref().and_then(|browse| {
                            if browse
                                .browse_endpoint_context_supported_configs
                                .browse_endpoint_context_music_config
                                .page_type
                                == "MUSIC_PAGE_TYPE_ARTIST"
                            {
                                Some(YtArtist {
                                    id: browse.browse_id.clone(),
                                    picture: value
                                        .thumbnail
                                        .music_thumbnail_renderer
                                        .thumbnail
                                        .thumbnails
                                        .as_ref()
                                        .and_then(|x| {
                                            x.iter()
                                                .max_by(|a, b| a.width.cmp(&b.width))
                                                .map(|x| x.url.clone())
                                        }),
                                    contains_cover: value
                                        .thumbnail
                                        .music_thumbnail_renderer
                                        .thumbnail
                                        .thumbnails
                                        .as_ref()
                                        .is_some_and(|x| !x.is_empty()),
                                    name: run.text.clone(),
                                    ..Default::default()
                                })
                            } else {
                                None
                            }
                        })
                    })
                })
            })
    })
}

fn track_album_from_search_result(
    value: &YtSearchResultsContentsListItemRenderer,
) -> Option<YtAlbum> {
    value.flex_columns.iter().find_map(|col| {
        col.music_responsive_list_item_flex_column_renderer
            .text
            .runs
            .as_ref()
            .and_then(|runs| {
                runs.iter().find_map(|run| {
                    run.navigation_endpoint.as_ref().and_then(|nav| {
                        nav.browse_endpoint.as_ref().and_then(|browse| {
                            if browse
                                .browse_endpoint_context_supported_configs
                                .browse_endpoint_context_music_config
                                .page_type
                                == "MUSIC_PAGE_TYPE_ALBUM"
                            {
                                let artist = album_artist_from_search_result(value);
                                Some(YtAlbum {
                                    id: browse.browse_id.clone(),
                                    artist: artist
                                        .as_ref()
                                        .map_or("N/A", |x| x.name.as_str())
                                        .to_string(),
                                    artist_id: artist
                                        .as_ref()
                                        .map_or("N/A", |x| x.id.as_str())
                                        .to_string(),
                                    contains_cover: artist
                                        .as_ref()
                                        .is_some_and(|x| x.picture.is_some()),
                                    cover: artist
                                        .as_ref()
                                        .and_then(|x| x.picture.as_ref())
                                        .cloned(),
                                    title: value
                                        .flex_columns
                                        .first()
                                        .and_then(|x| {
                                            x.music_responsive_list_item_flex_column_renderer
                                                .text
                                                .runs
                                                .as_ref()
                                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                                        })
                                        .unwrap_or("N/A")
                                        .to_string(),
                                    ..Default::default()
                                })
                            } else {
                                None
                            }
                        })
                    })
                })
            })
    })
}

fn album_from_search_result(value: &YtSearchResultsContentsListItemRenderer) -> Option<YtAlbum> {
    value
        .navigation_endpoint
        .browse_endpoint
        .as_ref()
        .and_then(|endpoint| {
            if endpoint
                .browse_endpoint_context_supported_configs
                .browse_endpoint_context_music_config
                .page_type
                == "MUSIC_PAGE_TYPE_ALBUM"
            {
                let artist = album_artist_from_search_result(value);
                Some(YtAlbum {
                    id: endpoint.browse_id.clone(),
                    artist: artist
                        .as_ref()
                        .map_or("N/A", |x| x.name.as_str())
                        .to_string(),
                    artist_id: artist.as_ref().map_or("N/A", |x| x.id.as_str()).to_string(),
                    contains_cover: artist.as_ref().is_some_and(|x| x.picture.is_some()),
                    cover: artist.as_ref().and_then(|x| x.picture.as_ref()).cloned(),
                    title: value
                        .flex_columns
                        .first()
                        .and_then(|x| {
                            x.music_responsive_list_item_flex_column_renderer
                                .text
                                .runs
                                .as_ref()
                                .and_then(|x| x.first().map(|x| x.text.as_str()))
                        })
                        .unwrap_or("N/A")
                        .to_string(),
                    ..Default::default()
                })
            } else {
                None
            }
        })
}

impl From<&YtSearchResults> for Vec<YtArtist> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .filter_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(artist_from_search_result)
                    })
            })
            .collect()
    }
}

impl From<&YtSearchResults> for Vec<YtAlbum> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .filter_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(album_from_search_result)
                    })
            })
            .collect()
    }
}

impl From<&YtSearchResults> for Vec<YtVideo> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .filter_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(video_from_search_result)
                    })
            })
            .collect()
    }
}

impl From<&YtSearchResults> for Vec<YtTrack> {
    fn from(value: &YtSearchResults) -> Self {
        value
            .contents
            .iter()
            .flat_map(|contents| {
                contents
                    .search_suggestions_section_renderer
                    .contents
                    .iter()
                    .filter_map(|section| {
                        section
                            .music_responsive_list_item_renderer
                            .as_ref()
                            .and_then(track_from_search_result)
                    })
            })
            .collect()
    }
}

/// Formatted search results organized by content type.
///
/// Parses raw `YouTube` Music API search results into separate lists for albums, artists, videos, and tracks.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchResultsFormatted {
    /// Albums matching the search query
    pub albums: Vec<YtAlbum>,
    /// Artists matching the search query
    pub artists: Vec<YtArtist>,
    /// Music videos matching the search query
    pub videos: Vec<YtVideo>,
    /// Tracks matching the search query
    pub tracks: Vec<YtTrack>,
    /// Pagination offset
    pub offset: usize,
    /// Maximum results per page
    pub limit: usize,
    /// Total number of results
    pub total: usize,
}

impl From<YtSearchResults> for YtSearchResultsFormatted {
    fn from(value: YtSearchResults) -> Self {
        Self {
            albums: (&value).into(),
            artists: (&value).into(),
            videos: (&value).into(),
            tracks: (&value).into(),
            offset: value.offset,
            limit: value.limit,
            total: value.total,
        }
    }
}

impl From<YtSearchResults> for ApiSearchResultsResponse {
    fn from(value: YtSearchResults) -> Self {
        let formatted: YtSearchResultsFormatted = value.into();
        formatted.into()
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<YtSearchResultsFormatted> for ApiSearchResultsResponse {
    fn from(value: YtSearchResultsFormatted) -> Self {
        let artists = value
            .artists
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let albums = value
            .albums
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let tracks = value
            .tracks
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();

        let position = value.offset + value.limit;
        let position = if position > value.total {
            value.total
        } else {
            position
        };

        Self {
            position: u32::try_from(position).unwrap(),
            results: [artists, albums, tracks].concat(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_yt_artist_image_size_from_u16() {
        assert_eq!(YtArtistImageSize::from(50u16), YtArtistImageSize::Small);
        assert_eq!(YtArtistImageSize::from(160u16), YtArtistImageSize::Small);
        assert_eq!(YtArtistImageSize::from(161u16), YtArtistImageSize::Medium);
        assert_eq!(YtArtistImageSize::from(320u16), YtArtistImageSize::Medium);
        assert_eq!(YtArtistImageSize::from(321u16), YtArtistImageSize::Large);
        assert_eq!(YtArtistImageSize::from(480u16), YtArtistImageSize::Large);
        assert_eq!(YtArtistImageSize::from(481u16), YtArtistImageSize::Max);
        assert_eq!(YtArtistImageSize::from(1000u16), YtArtistImageSize::Max);
    }

    #[test_log::test]
    fn test_yt_artist_image_size_to_u16() {
        assert_eq!(u16::from(YtArtistImageSize::Small), 160);
        assert_eq!(u16::from(YtArtistImageSize::Medium), 320);
        assert_eq!(u16::from(YtArtistImageSize::Large), 480);
        assert_eq!(u16::from(YtArtistImageSize::Max), 750);
    }

    #[test_log::test]
    fn test_yt_artist_image_size_display() {
        assert_eq!(YtArtistImageSize::Small.to_string(), "160");
        assert_eq!(YtArtistImageSize::Medium.to_string(), "320");
        assert_eq!(YtArtistImageSize::Large.to_string(), "480");
        assert_eq!(YtArtistImageSize::Max.to_string(), "750");
    }

    #[test_log::test]
    fn test_yt_album_image_size_from_u16() {
        assert_eq!(YtAlbumImageSize::from(50u16), YtAlbumImageSize::Thumbnail);
        assert_eq!(YtAlbumImageSize::from(80u16), YtAlbumImageSize::Thumbnail);
        assert_eq!(YtAlbumImageSize::from(81u16), YtAlbumImageSize::Small);
        assert_eq!(YtAlbumImageSize::from(160u16), YtAlbumImageSize::Small);
        assert_eq!(YtAlbumImageSize::from(161u16), YtAlbumImageSize::Medium);
        assert_eq!(YtAlbumImageSize::from(320u16), YtAlbumImageSize::Medium);
        assert_eq!(YtAlbumImageSize::from(321u16), YtAlbumImageSize::Large);
        assert_eq!(YtAlbumImageSize::from(640u16), YtAlbumImageSize::Large);
        assert_eq!(YtAlbumImageSize::from(641u16), YtAlbumImageSize::Max);
        assert_eq!(YtAlbumImageSize::from(2000u16), YtAlbumImageSize::Max);
    }

    #[test_log::test]
    fn test_yt_album_image_size_to_u16() {
        assert_eq!(u16::from(YtAlbumImageSize::Thumbnail), 80);
        assert_eq!(u16::from(YtAlbumImageSize::Small), 160);
        assert_eq!(u16::from(YtAlbumImageSize::Medium), 320);
        assert_eq!(u16::from(YtAlbumImageSize::Large), 640);
        assert_eq!(u16::from(YtAlbumImageSize::Max), 1280);
    }

    #[test_log::test]
    fn test_yt_album_image_size_display() {
        assert_eq!(YtAlbumImageSize::Thumbnail.to_string(), "80");
        assert_eq!(YtAlbumImageSize::Small.to_string(), "160");
        assert_eq!(YtAlbumImageSize::Medium.to_string(), "320");
        assert_eq!(YtAlbumImageSize::Large.to_string(), "640");
        assert_eq!(YtAlbumImageSize::Max.to_string(), "1280");
    }

    #[test_log::test]
    fn test_yt_search_artist_picture_url() {
        let artist = YtSearchArtist {
            id: 123,
            picture: Some("abc-def-ghi".to_string()),
            contains_cover: true,
            r#type: "ARTIST".to_string(),
            name: "Test Artist".to_string(),
        };

        assert_eq!(
            artist.picture_url(YtArtistImageSize::Small),
            Some("https://resources.yt.com/images/abc/def/ghi/160x160.jpg".to_string())
        );
        assert_eq!(
            artist.picture_url(YtArtistImageSize::Max),
            Some("https://resources.yt.com/images/abc/def/ghi/750x750.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_yt_search_artist_picture_url_none() {
        let artist = YtSearchArtist {
            id: 123,
            picture: None,
            contains_cover: false,
            r#type: "ARTIST".to_string(),
            name: "Test Artist".to_string(),
        };

        assert_eq!(artist.picture_url(YtArtistImageSize::Small), None);
    }

    #[test_log::test]
    fn test_yt_search_album_cover_url() {
        let album = YtSearchAlbum {
            id: 456,
            artists: vec![],
            contains_cover: true,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: Some("xyz-123-abc".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 85,
            release_date: Some("2024-01-01".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(
            album.cover_url(YtAlbumImageSize::Medium),
            Some("https://resources.yt.com/images/xyz/123/abc/320x320.jpg".to_string())
        );
        assert_eq!(
            album.cover_url(YtAlbumImageSize::Large),
            Some("https://resources.yt.com/images/xyz/123/abc/640x640.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_yt_search_album_cover_url_none() {
        let album = YtSearchAlbum {
            id: 456,
            artists: vec![],
            contains_cover: false,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: None,
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 85,
            release_date: Some("2024-01-01".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(album.cover_url(YtAlbumImageSize::Medium), None);
    }

    #[test_log::test]
    fn test_yt_album_cover_url() {
        let album = YtAlbum {
            id: "album123".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: "artist456".to_string(),
            album_type: crate::YtAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: None,
            cover: Some("cover-id-test".to_string()),
            duration: 2400,
            explicit: false,
            number_of_tracks: 10,
            popularity: 90,
            release_date: Some("2023-12-01".to_string()),
            title: "Greatest Hits".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(
            album.cover_url(YtAlbumImageSize::Thumbnail),
            Some("https://resources.yt.com/images/cover/id/test/80x80.jpg".to_string())
        );
        assert_eq!(
            album.cover_url(YtAlbumImageSize::Max),
            Some("https://resources.yt.com/images/cover/id/test/1280x1280.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_yt_artist_to_api_global_search_result() {
        let artist = YtArtist {
            id: "artist789".to_string(),
            picture: Some("pic-url".to_string()),
            contains_cover: true,
            popularity: 95,
            name: "Famous Artist".to_string(),
        };

        let result: ApiGlobalSearchResult = artist.into();
        match result {
            ApiGlobalSearchResult::Artist(artist_result) => {
                assert_eq!(artist_result.title, "Famous Artist");
                assert_eq!(artist_result.contains_cover, true);
                assert_eq!(artist_result.blur, false);
            }
            _ => panic!("Expected artist search result"),
        }
    }

    #[test_log::test]
    fn test_yt_track_to_api_global_search_result() {
        let track = YtTrack {
            id: "track123".to_string(),
            track_number: 5,
            artist_id: "artist999".to_string(),
            artist: "Track Artist".to_string(),
            artist_cover: Some("artist-cover".to_string()),
            album_id: "album888".to_string(),
            album: "Track Album".to_string(),
            album_type: crate::YtAlbumType::EpsAndSingles,
            album_cover: Some("album-cover".to_string()),
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 180,
            explicit: false,
            isrc: "USABC1234567".to_string(),
            popularity: 75,
            title: "Great Track".to_string(),
            media_metadata_tags: vec![],
        };

        let result: ApiGlobalSearchResult = track.into();
        match result {
            ApiGlobalSearchResult::Track(track_result) => {
                assert_eq!(track_result.title, "Great Track");
                assert_eq!(track_result.artist, "Track Artist");
                assert_eq!(track_result.album, "Track Album");
                assert_eq!(track_result.contains_cover, true);
                assert_eq!(track_result.blur, false);
            }
            _ => panic!("Expected track search result"),
        }
    }

    #[test_log::test]
    fn test_yt_album_to_api_global_search_result() {
        let album = YtAlbum {
            id: "album_id".to_string(),
            artist: "Album Artist".to_string(),
            artist_id: "artist_id".to_string(),
            album_type: crate::YtAlbumType::Compilations,
            contains_cover: false,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Label".to_string()),
            cover: None,
            duration: 3000,
            explicit: true,
            number_of_tracks: 15,
            popularity: 80,
            release_date: Some("2024-06-15".to_string()),
            title: "Compilation Album".to_string(),
            media_metadata_tags: vec!["tag1".to_string()],
        };

        let result: ApiGlobalSearchResult = album.into();
        match result {
            ApiGlobalSearchResult::Album(album_result) => {
                assert_eq!(album_result.title, "Compilation Album");
                assert_eq!(album_result.artist, "Album Artist");
                assert_eq!(album_result.contains_cover, false);
                assert_eq!(album_result.blur, false);
                assert_eq!(album_result.date_released, Some("2024-06-15".to_string()));
            }
            _ => panic!("Expected album search result"),
        }
    }

    #[test_log::test]
    fn test_yt_artist_to_artist_model() {
        use moosicbox_music_models::Artist;

        let yt_artist = YtArtist {
            id: "artist123".to_string(),
            picture: Some("pic-url".to_string()),
            contains_cover: true,
            popularity: 95,
            name: "Test Artist".to_string(),
        };

        let artist: Artist = yt_artist.into();
        assert_eq!(artist.title, "Test Artist");
        assert_eq!(artist.cover, Some("pic-url".to_string()));
    }

    #[test_log::test]
    fn test_yt_artist_to_api_artist() {
        use moosicbox_music_models::api::ApiArtist;

        let yt_artist = YtArtist {
            id: "artist456".to_string(),
            picture: None,
            contains_cover: false,
            popularity: 50,
            name: "Another Artist".to_string(),
        };

        let api_artist: ApiArtist = yt_artist.into();
        assert_eq!(api_artist.title, "Another Artist");
        assert!(!api_artist.contains_cover);
    }

    #[test_log::test]
    fn test_yt_track_to_track_model() {
        use moosicbox_music_models::Track;

        let yt_track = YtTrack {
            id: "track123".to_string(),
            track_number: 5,
            artist_id: "artist999".to_string(),
            artist: "Track Artist".to_string(),
            artist_cover: Some("artist-cover".to_string()),
            album_id: "album888".to_string(),
            album: "Track Album".to_string(),
            album_type: crate::YtAlbumType::Lp,
            album_cover: Some("album-cover".to_string()),
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 180,
            explicit: false,
            isrc: "USABC1234567".to_string(),
            popularity: 75,
            title: "Great Track".to_string(),
            media_metadata_tags: vec!["tag1".to_string()],
        };

        let track: Track = yt_track.into();
        assert_eq!(track.title, "Great Track");
        assert_eq!(track.number, 5);
        assert!((track.duration - 180.0).abs() < f64::EPSILON);
        assert_eq!(track.artist, "Track Artist");
        assert_eq!(track.album, "Track Album");
        assert_eq!(track.artwork, Some("album-cover".to_string()));
    }

    #[test_log::test]
    fn test_yt_track_to_track_model_without_album_cover() {
        use moosicbox_music_models::Track;

        let yt_track = YtTrack {
            id: "track456".to_string(),
            track_number: 1,
            artist_id: "artist111".to_string(),
            artist: "Artist Name".to_string(),
            artist_cover: None,
            album_id: "album222".to_string(),
            album: "Album Name".to_string(),
            album_type: crate::YtAlbumType::EpsAndSingles,
            album_cover: None,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024".to_string()),
            duration: 240,
            explicit: true,
            isrc: "USXYZ9876543".to_string(),
            popularity: 90,
            title: "Another Track".to_string(),
            media_metadata_tags: vec![],
        };

        let track: Track = yt_track.into();
        assert_eq!(track.title, "Another Track");
        assert_eq!(track.artwork, None);
        assert!(!track.blur);
    }

    #[test_log::test]
    fn test_yt_album_try_from_to_album() {
        use moosicbox_music_models::Album;

        let yt_album = YtAlbum {
            id: "album123".to_string(),
            artist: "Album Artist".to_string(),
            artist_id: "artist456".to_string(),
            album_type: crate::YtAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024 Label".to_string()),
            cover: Some("cover-url".to_string()),
            duration: 3600,
            explicit: false,
            number_of_tracks: 12,
            popularity: 85,
            release_date: Some("2024-06-15".to_string()),
            title: "Test Album".to_string(),
            media_metadata_tags: vec!["tag1".to_string()],
        };

        let album: Album = yt_album.try_into().unwrap();
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.artist, "Album Artist");
        assert_eq!(album.artwork, Some("cover-url".to_string()));
    }

    #[test_log::test]
    fn test_yt_album_try_from_to_album_without_release_date() {
        use moosicbox_music_models::Album;

        let yt_album = YtAlbum {
            id: "album789".to_string(),
            artist: "Another Artist".to_string(),
            artist_id: "artist789".to_string(),
            album_type: crate::YtAlbumType::Compilations,
            contains_cover: false,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: None,
            duration: 2400,
            explicit: true,
            number_of_tracks: 8,
            popularity: 60,
            release_date: None,
            title: "Compilation".to_string(),
            media_metadata_tags: vec![],
        };

        let album: Album = yt_album.try_into().unwrap();
        assert_eq!(album.title, "Compilation");
        assert!(album.date_released.is_none());
        assert!(album.artwork.is_none());
    }

    #[test_log::test]
    fn test_yt_album_cover_url_none() {
        let album = YtAlbum {
            id: "album_no_cover".to_string(),
            artist: "Artist".to_string(),
            artist_id: "artist_id".to_string(),
            album_type: crate::YtAlbumType::Lp,
            contains_cover: false,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            cover: None,
            duration: 1800,
            explicit: false,
            number_of_tracks: 5,
            popularity: 40,
            release_date: None,
            title: "No Cover Album".to_string(),
            media_metadata_tags: vec![],
        };

        assert_eq!(album.cover_url(YtAlbumImageSize::Medium), None);
    }

    #[test_log::test]
    fn test_yt_track_to_api_global_search_result_without_cover() {
        let track = YtTrack {
            id: "track_no_cover".to_string(),
            track_number: 1,
            artist_id: "artist_id".to_string(),
            artist: "Artist".to_string(),
            artist_cover: None,
            album_id: "album_id".to_string(),
            album: "Album".to_string(),
            album_type: crate::YtAlbumType::Lp,
            album_cover: None,
            audio_quality: "HIGH".to_string(),
            copyright: None,
            duration: 200,
            explicit: false,
            isrc: "ISRC123".to_string(),
            popularity: 50,
            title: "Track Without Cover".to_string(),
            media_metadata_tags: vec![],
        };

        let result: ApiGlobalSearchResult = track.into();
        match result {
            ApiGlobalSearchResult::Track(track_result) => {
                assert!(!track_result.contains_cover);
            }
            _ => panic!("Expected track search result"),
        }
    }

    #[test_log::test]
    fn test_yt_search_results_formatted_to_api_response_position_calculation() {
        let formatted = YtSearchResultsFormatted {
            albums: vec![],
            artists: vec![],
            videos: vec![],
            tracks: vec![],
            offset: 0,
            limit: 10,
            total: 100,
        };

        let response: ApiSearchResultsResponse = formatted.into();
        // position = min(offset + limit, total) = min(0 + 10, 100) = 10
        assert_eq!(response.position, 10);
    }

    #[test_log::test]
    fn test_yt_search_results_formatted_to_api_response_position_exceeds_total() {
        let formatted = YtSearchResultsFormatted {
            albums: vec![],
            artists: vec![],
            videos: vec![],
            tracks: vec![],
            offset: 95,
            limit: 10,
            total: 100,
        };

        let response: ApiSearchResultsResponse = formatted.into();
        // position = min(95 + 10, 100) = min(105, 100) = 100
        assert_eq!(response.position, 100);
    }

    #[test_log::test]
    fn test_yt_search_results_formatted_to_api_response_with_items() {
        let formatted = YtSearchResultsFormatted {
            albums: vec![YtAlbum {
                id: "album1".to_string(),
                artist: "Artist1".to_string(),
                artist_id: "artist_id1".to_string(),
                album_type: crate::YtAlbumType::Lp,
                contains_cover: true,
                audio_quality: "HIGH".to_string(),
                copyright: None,
                cover: None,
                duration: 1800,
                explicit: false,
                number_of_tracks: 10,
                popularity: 80,
                release_date: None,
                title: "Album 1".to_string(),
                media_metadata_tags: vec![],
            }],
            artists: vec![YtArtist {
                id: "artist1".to_string(),
                picture: None,
                contains_cover: false,
                popularity: 70,
                name: "Artist 1".to_string(),
            }],
            videos: vec![],
            tracks: vec![YtTrack {
                id: "track1".to_string(),
                track_number: 1,
                artist_id: "artist1".to_string(),
                artist: "Artist 1".to_string(),
                artist_cover: None,
                album_id: "album1".to_string(),
                album: "Album 1".to_string(),
                album_type: crate::YtAlbumType::Lp,
                album_cover: None,
                audio_quality: "HIGH".to_string(),
                copyright: None,
                duration: 180,
                explicit: false,
                isrc: "ISRC1".to_string(),
                popularity: 60,
                title: "Track 1".to_string(),
                media_metadata_tags: vec![],
            }],
            offset: 0,
            limit: 10,
            total: 3,
        };

        let response: ApiSearchResultsResponse = formatted.into();
        // 1 artist + 1 album + 1 track = 3 results
        assert_eq!(response.results.len(), 3);
        // position = min(0 + 10, 3) = 3
        assert_eq!(response.position, 3);
    }

    #[test_log::test]
    fn test_yt_search_results_formatted_empty() {
        let formatted = YtSearchResultsFormatted {
            albums: vec![],
            artists: vec![],
            videos: vec![],
            tracks: vec![],
            offset: 0,
            limit: 10,
            total: 0,
        };

        let response: ApiSearchResultsResponse = formatted.into();
        assert_eq!(response.results.len(), 0);
        assert_eq!(response.position, 0);
    }

    #[test_log::test]
    fn test_yt_artist_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "artist_123",
            "picture": "some-picture-url",
            "popularity": 85,
            "name": "Test Artist Name"
        });

        let artist: YtArtist = json.as_model().unwrap();
        assert_eq!(artist.id, "artist_123");
        assert_eq!(artist.picture, Some("some-picture-url".to_string()));
        assert!(artist.contains_cover);
        assert_eq!(artist.popularity, 85);
        assert_eq!(artist.name, "Test Artist Name");
    }

    #[test_log::test]
    fn test_yt_artist_as_model_from_json_without_picture() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "artist_456",
            "picture": null,
            "popularity": 50,
            "name": "No Picture Artist"
        });

        let artist: YtArtist = json.as_model().unwrap();
        assert_eq!(artist.id, "artist_456");
        assert!(artist.picture.is_none());
        assert!(!artist.contains_cover);
        assert_eq!(artist.popularity, 50);
        assert_eq!(artist.name, "No Picture Artist");
    }

    #[test_log::test]
    fn test_yt_artist_as_model_missing_required_field() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "artist_789",
            "popularity": 50
            // missing "name" field
        });

        let result: Result<YtArtist, _> = json.as_model();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_yt_album_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "album_123",
            "artist": { "name": "Album Artist", "id": "artist_456" },
            "type": "LP",
            "audioQuality": "LOSSLESS",
            "copyright": "2024 Test Label",
            "cover": "cover-image-url",
            "duration": 3600,
            "explicit": true,
            "numberOfTracks": 12,
            "popularity": 90,
            "releaseDate": "2024-06-15",
            "title": "Test Album Title",
            "mediaMetadata": { "tags": ["lossless", "hires"] }
        });

        let album: YtAlbum = json.as_model().unwrap();
        assert_eq!(album.id, "album_123");
        assert_eq!(album.artist, "Album Artist");
        assert_eq!(album.artist_id, "artist_456");
        assert_eq!(album.album_type, crate::YtAlbumType::Lp);
        assert!(album.contains_cover);
        assert_eq!(album.audio_quality, "LOSSLESS");
        assert_eq!(album.copyright, Some("2024 Test Label".to_string()));
        assert_eq!(album.cover, Some("cover-image-url".to_string()));
        assert_eq!(album.duration, 3600);
        assert!(album.explicit);
        assert_eq!(album.number_of_tracks, 12);
        assert_eq!(album.popularity, 90);
        assert_eq!(album.release_date, Some("2024-06-15".to_string()));
        assert_eq!(album.title, "Test Album Title");
        assert_eq!(album.media_metadata_tags, vec!["lossless", "hires"]);
    }

    #[test_log::test]
    fn test_yt_album_as_model_with_optional_fields_null() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "album_789",
            "artist": { "name": "Minimal Artist", "id": "artist_minimal" },
            "type": "EPSANDSINGLES",
            "audioQuality": "HIGH",
            "copyright": null,
            "cover": null,
            "duration": 1200,
            "explicit": false,
            "numberOfTracks": 4,
            "popularity": 60,
            "releaseDate": null,
            "title": "EP Title",
            "mediaMetadata": { "tags": [] }
        });

        let album: YtAlbum = json.as_model().unwrap();
        assert_eq!(album.id, "album_789");
        assert_eq!(album.album_type, crate::YtAlbumType::EpsAndSingles);
        assert!(album.copyright.is_none());
        assert!(album.cover.is_none());
        assert!(album.release_date.is_none());
        assert!(album.media_metadata_tags.is_empty());
    }

    #[test_log::test]
    fn test_yt_track_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "track_123",
            "trackNumber": 5,
            "artist": { "id": "artist_456", "name": "Track Artist", "picture": "artist-pic" },
            "album": { "id": "album_789", "title": "Track Album", "cover": "album-cover", "type": "LP" },
            "audioQuality": "HI_RES_LOSSLESS",
            "copyright": "2024 Music Label",
            "duration": 240,
            "explicit": true,
            "isrc": "USABC1234567",
            "popularity": 85,
            "title": "Track Title",
            "mediaMetadata": { "tags": ["24bit", "hires"] }
        });

        let track: YtTrack = json.as_model().unwrap();
        assert_eq!(track.id, "track_123");
        assert_eq!(track.track_number, 5);
        assert_eq!(track.artist_id, "artist_456");
        assert_eq!(track.artist, "Track Artist");
        assert_eq!(track.artist_cover, Some("artist-pic".to_string()));
        assert_eq!(track.album_id, "album_789");
        assert_eq!(track.album, "Track Album");
        assert_eq!(track.album_cover, Some("album-cover".to_string()));
        assert_eq!(track.album_type, crate::YtAlbumType::Lp);
        assert_eq!(track.audio_quality, "HI_RES_LOSSLESS");
        assert_eq!(track.copyright, Some("2024 Music Label".to_string()));
        assert_eq!(track.duration, 240);
        assert!(track.explicit);
        assert_eq!(track.isrc, "USABC1234567");
        assert_eq!(track.popularity, 85);
        assert_eq!(track.title, "Track Title");
        assert_eq!(track.media_metadata_tags, vec!["24bit", "hires"]);
    }

    #[test_log::test]
    fn test_yt_track_as_model_with_null_optional_fields() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "track_456",
            "trackNumber": 1,
            "artist": { "id": "artist_id", "name": "Artist", "picture": null },
            "album": { "id": "album_id", "title": "Album", "cover": null, "type": "COMPILATIONS" },
            "audioQuality": "HIGH",
            "copyright": null,
            "duration": 180,
            "explicit": false,
            "isrc": "ISRC12345",
            "popularity": 50,
            "title": "Simple Track",
            "mediaMetadata": { "tags": [] }
        });

        let track: YtTrack = json.as_model().unwrap();
        assert!(track.artist_cover.is_none());
        assert!(track.album_cover.is_none());
        assert!(track.copyright.is_none());
        assert_eq!(track.album_type, crate::YtAlbumType::Compilations);
    }

    #[test_log::test]
    fn test_yt_album_try_into_api_album() {
        use moosicbox_music_models::api::ApiAlbum;

        let yt_album = YtAlbum {
            id: "album_api".to_string(),
            artist: "API Artist".to_string(),
            artist_id: "artist_api".to_string(),
            album_type: crate::YtAlbumType::Lp,
            contains_cover: true,
            audio_quality: "LOSSLESS".to_string(),
            copyright: Some("2024".to_string()),
            cover: Some("cover-url".to_string()),
            duration: 3000,
            explicit: false,
            number_of_tracks: 10,
            popularity: 80,
            release_date: Some("2024-06-15".to_string()),
            title: "API Album".to_string(),
            media_metadata_tags: vec![],
        };

        let api_album: ApiAlbum = yt_album.try_into().unwrap();
        assert_eq!(api_album.title, "API Album");
        assert_eq!(api_album.artist, "API Artist");
    }

    #[test_log::test]
    fn test_yt_search_artist_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": 12345,
            "picture": "search-artist-pic",
            "type": "ARTIST",
            "name": "Search Artist Name"
        });

        let artist: YtSearchArtist = json.as_model().unwrap();
        assert_eq!(artist.id, 12345);
        assert_eq!(artist.picture, Some("search-artist-pic".to_string()));
        assert!(artist.contains_cover);
        assert_eq!(artist.r#type, "ARTIST");
        assert_eq!(artist.name, "Search Artist Name");
    }

    #[test_log::test]
    fn test_yt_search_artist_as_model_without_picture() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": 67890,
            "picture": null,
            "type": "ARTIST",
            "name": "No Pic Search Artist"
        });

        let artist: YtSearchArtist = json.as_model().unwrap();
        assert!(artist.picture.is_none());
        assert!(!artist.contains_cover);
    }

    #[test_log::test]
    fn test_yt_search_album_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": 98765,
            "artists": [
                { "id": 111, "picture": "artist1-pic", "type": "ARTIST", "name": "Artist 1" },
                { "id": 222, "picture": null, "type": "ARTIST", "name": "Artist 2" }
            ],
            "audioQuality": "LOSSLESS",
            "copyright": "2024 Label",
            "cover": "search-album-cover",
            "duration": 2700,
            "explicit": true,
            "numberOfTracks": 9,
            "popularity": 75,
            "releaseDate": "2024-03-20",
            "title": "Search Album Title",
            "mediaMetadata": { "tags": ["dolby_atmos"] }
        });

        let album: YtSearchAlbum = json.as_model().unwrap();
        assert_eq!(album.id, 98765);
        assert_eq!(album.artists.len(), 2);
        assert_eq!(album.artists[0].name, "Artist 1");
        assert_eq!(album.artists[1].name, "Artist 2");
        assert!(album.contains_cover);
        assert_eq!(album.audio_quality, "LOSSLESS");
        assert_eq!(album.copyright, Some("2024 Label".to_string()));
        assert_eq!(album.cover, Some("search-album-cover".to_string()));
        assert_eq!(album.duration, 2700);
        assert!(album.explicit);
        assert_eq!(album.number_of_tracks, 9);
        assert_eq!(album.popularity, 75);
        assert_eq!(album.release_date, Some("2024-03-20".to_string()));
        assert_eq!(album.title, "Search Album Title");
        assert_eq!(album.media_metadata_tags, vec!["dolby_atmos"]);
    }

    #[test_log::test]
    fn test_yt_video_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": "video_123",
            "artist": { "id": 456, "name": "Video Artist", "picture": "video-artist-pic" },
            "album": { "id": 789, "title": "Video Album", "cover": "video-album-cover" },
            "audioQuality": "HIGH",
            "duration": 300,
            "explicit": false,
            "title": "Music Video Title"
        });

        let video: YtVideo = json.as_model().unwrap();
        assert_eq!(video.id, "video_123");
        assert_eq!(video.artist_id, 456);
        assert_eq!(video.artist, "Video Artist");
        assert_eq!(video.artist_cover, Some("video-artist-pic".to_string()));
        assert_eq!(video.album_id, 789);
        assert_eq!(video.album, "Video Album");
        assert_eq!(video.album_cover, Some("video-album-cover".to_string()));
        assert_eq!(video.audio_quality, "HIGH");
        assert_eq!(video.duration, 300);
        assert!(!video.explicit);
        assert_eq!(video.title, "Music Video Title");
    }

    #[test_log::test]
    fn test_yt_search_track_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "id": 54321,
            "trackNumber": 3,
            "artists": [
                { "id": 111, "picture": "artist-pic", "type": "ARTIST", "name": "Main Artist" }
            ],
            "artist": { "picture": "artist-picture-url" },
            "album": { "id": 222, "title": "Search Track Album", "cover": "track-album-cover" },
            "audioQuality": "HI_RES_LOSSLESS",
            "copyright": "2024 Music Co",
            "duration": 195,
            "explicit": false,
            "isrc": "USXYZ9876543",
            "popularity": 70,
            "title": "Search Track Title",
            "mediaMetadata": { "tags": ["mqa"] }
        });

        let track: YtSearchTrack = json.as_model().unwrap();
        assert_eq!(track.id, 54321);
        assert_eq!(track.track_number, 3);
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Main Artist");
        assert_eq!(track.artist_cover, Some("artist-picture-url".to_string()));
        assert_eq!(track.album_id, 222);
        assert_eq!(track.album, "Search Track Album");
        assert_eq!(track.album_cover, Some("track-album-cover".to_string()));
        assert_eq!(track.audio_quality, "HI_RES_LOSSLESS");
        assert_eq!(track.copyright, Some("2024 Music Co".to_string()));
        assert_eq!(track.duration, 195);
        assert!(!track.explicit);
        assert_eq!(track.isrc, "USXYZ9876543");
        assert_eq!(track.popularity, 70);
        assert_eq!(track.title, "Search Track Title");
        assert_eq!(track.media_metadata_tags, vec!["mqa"]);
    }

    #[test_log::test]
    fn test_yt_search_result_list_as_model_from_json() {
        use moosicbox_json_utils::database::AsModelResult;

        let json = serde_json::json!({
            "items": [
                {
                    "id": "artist1",
                    "picture": null,
                    "popularity": 80,
                    "name": "Artist One"
                },
                {
                    "id": "artist2",
                    "picture": "pic-url",
                    "popularity": 90,
                    "name": "Artist Two"
                }
            ],
            "offset": 0,
            "limit": 10,
            "totalNumberOfItems": 2
        });

        let result: YtSearchResultList<YtArtist> = json.as_model().unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 10);
        assert_eq!(result.total, 2);
        assert_eq!(result.items[0].name, "Artist One");
        assert_eq!(result.items[1].name, "Artist Two");
    }
}
