use std::fmt::Display;

use moosicbox_core::sqlite::models::{
    Album, AlbumSource, ApiAlbum, ApiArtist, ApiSource, ApiSources, Artist, AsModelResult, Track,
    TrackApiSource,
};
use moosicbox_json_utils::{
    serde_json::{ToNestedValue, ToValue},
    ParseError, ToValueType,
};
use moosicbox_music_api::models::ImageCoverSize;
use moosicbox_search::models::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{format_title, QobuzAlbumReleaseType};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzImage {
    pub thumbnail: Option<String>,
    pub small: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
    pub extralarge: Option<String>,
    pub mega: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum QobuzImageSize {
    Mega,       // 4800
    ExtraLarge, // 2400
    Large,      // 1200
    Medium,     // 600
    Small,      // 300
    Thumbnail,  // 100
}

impl From<ImageCoverSize> for QobuzImageSize {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => QobuzImageSize::Mega,
            ImageCoverSize::Large => QobuzImageSize::Large,
            ImageCoverSize::Medium => QobuzImageSize::Medium,
            ImageCoverSize::Small => QobuzImageSize::Small,
            ImageCoverSize::Thumbnail => QobuzImageSize::Thumbnail,
        }
    }
}

impl From<QobuzImageSize> for u16 {
    fn from(value: QobuzImageSize) -> Self {
        match value {
            QobuzImageSize::Mega => 4800,
            QobuzImageSize::ExtraLarge => 2400,
            QobuzImageSize::Large => 1200,
            QobuzImageSize::Medium => 600,
            QobuzImageSize::Small => 300,
            QobuzImageSize::Thumbnail => 100,
        }
    }
}

impl From<u16> for QobuzImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=100 => QobuzImageSize::Thumbnail,
            101..=300 => QobuzImageSize::Small,
            301..=600 => QobuzImageSize::Medium,
            601..=1200 => QobuzImageSize::Large,
            1201..=2400 => QobuzImageSize::ExtraLarge,
            _ => QobuzImageSize::Mega,
        }
    }
}

impl Display for QobuzImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl QobuzImage {
    pub fn cover_url(&self) -> Option<String> {
        self.cover_url_for_size(QobuzImageSize::Mega)
    }

    pub fn cover_url_for_size(&self, size: QobuzImageSize) -> Option<String> {
        match size {
            QobuzImageSize::Thumbnail => self
                .thumbnail
                .clone()
                .or(self.small.clone())
                .or(self.medium.clone())
                .or(self.large.clone())
                .or(self.extralarge.clone())
                .or(self.mega.clone()),

            QobuzImageSize::Small => self
                .small
                .clone()
                .or(self.medium.clone())
                .or(self.large.clone())
                .or(self.extralarge.clone())
                .or(self.mega.clone())
                .or(self.thumbnail.clone()),
            QobuzImageSize::Medium => self
                .medium
                .clone()
                .or(self.large.clone())
                .or(self.extralarge.clone())
                .or(self.mega.clone())
                .or(self.small.clone())
                .or(self.thumbnail.clone()),

            QobuzImageSize::Large => self
                .large
                .clone()
                .or(self.extralarge.clone())
                .or(self.mega.clone())
                .or(self.medium.clone())
                .or(self.small.clone())
                .or(self.thumbnail.clone()),

            QobuzImageSize::ExtraLarge => self
                .extralarge
                .clone()
                .or(self.mega.clone())
                .or(self.large.clone())
                .or(self.medium.clone())
                .or(self.small.clone())
                .or(self.thumbnail.clone()),

            QobuzImageSize::Mega => self
                .mega
                .clone()
                .or(self.extralarge.clone())
                .or(self.large.clone())
                .or(self.medium.clone())
                .or(self.small.clone())
                .or(self.thumbnail.clone()),
        }
    }
}

impl ToValueType<QobuzImage> for &Value {
    fn to_value_type(self) -> Result<QobuzImage, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzImage, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzImage, ParseError> {
        Ok(QobuzImage {
            thumbnail: self.to_value("thumbnail")?,
            small: self.to_value("small")?,
            medium: self.to_value("medium")?,
            large: self.to_value("large")?,
            extralarge: self.to_value("extralarge")?,
            mega: self.to_value("mega")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzGenre {
    pub id: u64,
    pub name: String,
    pub slug: String,
}

impl ToValueType<QobuzGenre> for &Value {
    fn to_value_type(self) -> Result<QobuzGenre, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzGenre, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzGenre, ParseError> {
        Ok(QobuzGenre {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            slug: self.to_value("slug")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbum {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: QobuzAlbumReleaseType,
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub version: Option<String>,
    pub qobuz_id: u64,
    pub released_at: i64,
    pub release_date_original: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub popularity: u32,
    pub tracks_count: u32,
    pub genre: QobuzGenre,
    pub maximum_channel_count: u16,
    pub maximum_sampling_rate: f32,
}

impl From<QobuzAlbum> for Album {
    fn from(value: QobuzAlbum) -> Self {
        let artwork = value.cover_url();
        Self {
            id: value.id.as_str().into(),
            title: format_title(value.title.as_str(), value.version.as_deref()),
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: Some(value.release_date_original),
            date_added: None,
            artwork,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Qobuz,
            api_source: ApiSource::Qobuz,
            artist_sources: ApiSources::default()
                .with_source(ApiSource::Qobuz, value.artist_id.into()),
            album_sources: ApiSources::default().with_source(ApiSource::Qobuz, value.id.into()),
        }
    }
}

impl From<QobuzAlbum> for ApiAlbum {
    fn from(value: QobuzAlbum) -> Self {
        let album: Album = value.into();
        album.into()
    }
}

impl From<QobuzAlbum> for ApiGlobalSearchResult {
    fn from(value: QobuzAlbum) -> Self {
        Self::Album(ApiGlobalAlbumSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
            album_id: value.id.into(),
            title: value.title,
            contains_cover: value.image.is_some(),
            blur: false,
            date_released: Some(value.release_date_original),
            date_added: None,
            versions: vec![],
        })
    }
}

impl From<Album> for QobuzAlbum {
    fn from(value: Album) -> Self {
        Self {
            id: value.id.clone().into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            maximum_bit_depth: 0,
            image: value.artwork.map(|x| QobuzImage {
                thumbnail: None,
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: Some(x),
            }),
            version: None,
            qobuz_id: 0,
            released_at: 0,
            release_date_original: value.date_released.unwrap_or_default(),
            duration: 0,
            parental_warning: false,
            popularity: 0,
            tracks_count: 0,
            genre: QobuzGenre {
                id: 0,
                name: "".to_string(),
                slug: "".to_string(),
            },
            maximum_channel_count: 0,
            maximum_sampling_rate: 0.0,
        }
    }
}

impl QobuzAlbum {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl ToValueType<QobuzAlbum> for &Value {
    fn to_value_type(self) -> Result<QobuzAlbum, ParseError> {
        self.as_model()
    }
}

pub fn magic_qobuz_album_release_type_determinizer(
    duration: u32,
    tracks_count: u32,
) -> QobuzAlbumReleaseType {
    match tracks_count {
        1 => QobuzAlbumReleaseType::Single,
        2..=6 => {
            if duration > 60 * 4 * 5 {
                QobuzAlbumReleaseType::Album
            } else {
                QobuzAlbumReleaseType::EpSingle
            }
        }
        _ => QobuzAlbumReleaseType::Album,
    }
}

impl AsModelResult<QobuzAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzAlbum, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> = self.to_value("release_type")?;
        let duration = self.to_value("duration")?;
        let tracks_count = self.to_value("tracks_count")?;
        Ok(QobuzAlbum {
            id: self.to_value("id")?,
            artist: self
                .to_nested_value::<String>(&["artist", "name"])
                .or_else(|_| self.to_nested_value(&["artist", "name", "display"]))?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            maximum_bit_depth: self
                .to_value("maximum_bit_depth")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_bit_depth"]))?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
            qobuz_id: self.to_value("qobuz_id")?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            released_at: self.to_value("released_at")?,
            release_date_original: self.to_value("release_date_original")?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            popularity: self.to_value("popularity")?,
            tracks_count,
            genre: self.to_value("genre")?,
            maximum_channel_count: self
                .to_value("maximum_channel_count")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_channel_count"]))?,
            maximum_sampling_rate: self
                .to_value("maximum_sampling_rate")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_sampling_rate"]))?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzRelease {
    pub id: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: QobuzAlbumReleaseType,
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub version: Option<String>,
    pub release_date_original: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub tracks_count: u32,
    pub genre: String,
    pub maximum_channel_count: u16,
    pub maximum_sampling_rate: f32,
}

impl QobuzRelease {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl ToValueType<QobuzRelease> for &Value {
    fn to_value_type(self) -> Result<QobuzRelease, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzRelease, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzRelease, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> = self.to_value("release_type")?;
        let duration = self.to_value("duration")?;
        let tracks_count = self.to_value("tracks_count")?;
        Ok(QobuzRelease {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name", "display"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            maximum_bit_depth: self.to_nested_value(&["audio_info", "maximum_bit_depth"])?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
            release_date_original: self.to_nested_value(&["dates", "original"])?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            tracks_count,
            genre: self.to_nested_value(&["genre", "name"])?,
            maximum_channel_count: self
                .to_nested_value(&["audio_info", "maximum_channel_count"])?,
            maximum_sampling_rate: self
                .to_nested_value(&["audio_info", "maximum_sampling_rate"])?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct QobuzTrack {
    pub id: u64,
    pub track_number: u32,
    pub artist: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: String,
    pub album_type: QobuzAlbumReleaseType,
    pub image: Option<QobuzImage>,
    pub copyright: Option<String>,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub title: String,
    pub version: Option<String>,
}

impl From<QobuzTrack> for Track {
    fn from(value: QobuzTrack) -> Self {
        let artwork = value.cover_url();
        Self {
            id: value.id.into(),
            number: value.track_number,
            title: format_title(value.title.as_str(), value.version.as_deref()),
            duration: value.duration as f64,
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: None,
            date_added: None,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: TrackApiSource::Qobuz,
            api_source: ApiSource::Qobuz,
            sources: ApiSources::default().with_source(ApiSource::Qobuz, value.id.into()),
        }
    }
}

impl From<QobuzTrack> for ApiGlobalSearchResult {
    fn from(value: QobuzTrack) -> Self {
        Self::Track(ApiGlobalTrackSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
            album_id: value.album_id.into(),
            album: value.album,
            title: value.title,
            contains_cover: value.image.is_some(),
            blur: false,
            date_released: None,
            date_added: None,
            track_id: value.id.into(),
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Qobuz,
        })
    }
}

impl QobuzTrack {
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(|image| image.cover_url())
    }
}

impl ToValueType<QobuzTrack> for &Value {
    fn to_value_type(self) -> Result<QobuzTrack, ParseError> {
        self.as_model()
    }
}

impl QobuzTrack {
    #[allow(clippy::too_many_arguments)]
    pub fn from_value(
        value: &Value,
        artist: &str,
        artist_id: u64,
        album: &str,
        album_id: &str,
        album_type: QobuzAlbumReleaseType,
        album_version: Option<&str>,
        image: Option<QobuzImage>,
    ) -> Result<QobuzTrack, ParseError> {
        Ok(QobuzTrack {
            id: value.to_value("id")?,
            track_number: value.to_value("track_number")?,
            artist: artist.to_string(),
            artist_id,
            album_type,
            album: format_title(album, album_version),
            album_id: album_id.to_string(),
            image,
            copyright: value.to_value("copyright")?,
            duration: value.to_value("duration")?,
            parental_warning: value.to_value("parental_warning")?,
            isrc: value.to_value("isrc")?,
            title: value.to_value("title")?,
            version: value.to_value("version")?,
        })
    }
}

impl AsModelResult<QobuzTrack, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzTrack, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> =
            self.to_nested_value(&["album", "release_type"])?;
        let duration = self.to_nested_value(&["album", "duration"])?;
        let tracks_count = self.to_nested_value(&["album", "tracks_count"])?;
        Ok(QobuzTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("track_number")?,
            album: self.to_nested_value(&["album", "title"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            artist: self.to_nested_value(&["album", "artist", "name"])?,
            artist_id: self.to_nested_value(&["album", "artist", "id"])?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            image: self.to_value("image")?,
            copyright: self.to_value("copyright")?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            isrc: self.to_value("isrc")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtist {
    pub id: u64,
    pub image: Option<QobuzImage>,
    pub name: String,
}

impl From<QobuzArtist> for Artist {
    fn from(value: QobuzArtist) -> Self {
        let cover = value.cover_url();
        Self {
            id: value.id.into(),
            title: value.name,
            cover,
            api_source: ApiSource::Qobuz,
            api_sources: ApiSources::default().with_source(ApiSource::Qobuz, value.id.into()),
        }
    }
}

impl From<QobuzArtist> for ApiArtist {
    fn from(value: QobuzArtist) -> Self {
        Self {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.image.is_some(),
            api_source: ApiSource::Qobuz,
            api_sources: ApiSources::default().with_source(ApiSource::Qobuz, value.id.into()),
        }
    }
}

impl From<QobuzArtist> for ApiGlobalSearchResult {
    fn from(value: QobuzArtist) -> Self {
        Self::Artist(ApiGlobalArtistSearchResult {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.image.is_some(),
            blur: false,
        })
    }
}

impl QobuzArtist {
    pub fn cover_url(&self) -> Option<String> {
        self.image.clone().and_then(|image| image.cover_url())
    }
}

impl ToValueType<QobuzArtist> for &Value {
    fn to_value_type(self) -> Result<QobuzArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzArtist, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzArtist, ParseError> {
        Ok(QobuzArtist {
            id: self.to_value("id")?,
            image: self.to_value("image")?,
            name: self.to_value("name")?,
        })
    }
}

impl From<QobuzRelease> for QobuzAlbum {
    fn from(value: QobuzRelease) -> Self {
        QobuzAlbum {
            id: value.id,
            artist: value.artist,
            artist_id: value.artist_id,
            maximum_bit_depth: value.maximum_bit_depth,
            image: value.image,
            title: value.title,
            release_date_original: value.release_date_original,
            duration: value.duration,
            parental_warning: value.parental_warning,
            tracks_count: value.tracks_count,
            maximum_channel_count: value.maximum_bit_depth,
            maximum_sampling_rate: value.maximum_sampling_rate,
            ..Default::default()
        }
    }
}

impl From<QobuzRelease> for Album {
    fn from(value: QobuzRelease) -> Self {
        let album: QobuzAlbum = value.into();
        album.into()
    }
}

impl From<QobuzRelease> for ApiAlbum {
    fn from(value: QobuzRelease) -> Self {
        let album: Album = value.into();
        album.into()
    }
}

#[derive(Serialize, Deserialize)]
pub struct QobuzSearchResultList<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
}

impl<'a, T> ToValueType<QobuzSearchResultList<T>> for &'a Value
where
    Value: AsModelResult<QobuzSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<QobuzSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<QobuzSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Value: ToValueType<T>,
    for<'a> &'a Value: ToValueType<usize>,
{
    fn as_model(&self) -> Result<QobuzSearchResultList<T>, ParseError> {
        Ok(QobuzSearchResultList {
            items: self.to_value("items")?,
            offset: self.to_value("offset")?,
            limit: self.to_value("limit")?,
            total: self.to_value("total")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct QobuzSearchResults {
    pub albums: QobuzSearchResultList<QobuzAlbum>,
    pub artists: QobuzSearchResultList<QobuzArtist>,
    pub tracks: QobuzSearchResultList<QobuzTrack>,
}

impl From<QobuzSearchResults> for ApiSearchResultsResponse {
    fn from(value: QobuzSearchResults) -> Self {
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

        let position = value.albums.offset + value.albums.limit;
        let position = if position > value.albums.total {
            value.albums.total
        } else {
            position
        };

        Self {
            position,
            results: [artists, albums, tracks].concat(),
        }
    }
}

impl ToValueType<QobuzSearchResults> for &Value {
    fn to_value_type(self) -> Result<QobuzSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzSearchResults, ParseError> {
        Ok(QobuzSearchResults {
            albums: self.to_value("albums")?,
            artists: self.to_value("artists")?,
            tracks: self.to_value("tracks")?,
        })
    }
}
