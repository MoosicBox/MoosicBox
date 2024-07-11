use std::fmt::Display;

use moosicbox_json_utils::{
    serde_json::{ToNestedValue, ToValue},
    ParseError, ToValueType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Album, Artist, AsModelResult, Track};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub popularity: u32,
    pub name: String,
}

impl From<TidalArtist> for Artist {
    fn from(value: TidalArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.name,
            cover: value.picture,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TidalArtistImageSize {
    Max,    // 750
    Large,  // 480
    Medium, // 320
    Small,  // 160
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
            0..=160 => TidalArtistImageSize::Small,
            161..=320 => TidalArtistImageSize::Medium,
            321..=480 => TidalArtistImageSize::Large,
            _ => TidalArtistImageSize::Max,
        }
    }
}

impl Display for TidalArtistImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl TidalArtist {
    pub fn picture_url(&self, size: TidalArtistImageSize) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.tidal.com/images/{picture_path}/{size}x{size}.jpg")
        })
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub r#type: String,
    pub name: String,
}

impl TidalSearchArtist {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    pub id: u64,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl From<TidalAlbum> for Album {
    fn from(value: TidalAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.release_date,
            date_added: None,
            artwork: value.cover,
            directory: None,
            blur: false,
            versions: vec![],
        }
    }
}

impl From<Album> for TidalAlbum {
    fn from(value: Album) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            contains_cover: value.artwork.is_some(),
            audio_quality: "N/A".to_string(),
            copyright: None,
            cover: value.artwork,
            duration: 0,
            explicit: false,
            number_of_tracks: 0,
            popularity: 0,
            release_date: value.date_released,
            media_metadata_tags: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TidalAlbumImageSize {
    Max,       // 1280
    Large,     // 640
    Medium,    // 320
    Small,     // 160
    Thumbnail, // 80
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
            0..=80 => TidalAlbumImageSize::Thumbnail,
            81..=160 => TidalAlbumImageSize::Small,
            161..=320 => TidalAlbumImageSize::Medium,
            321..=640 => TidalAlbumImageSize::Large,
            _ => TidalAlbumImageSize::Max,
        }
    }
}

impl Display for TidalAlbumImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchAlbum {
    pub id: u64,
    pub artists: Vec<TidalSearchArtist>,
    pub contains_cover: bool,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub cover: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub number_of_tracks: u32,
    pub popularity: u32,
    pub release_date: Option<String>,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl TidalSearchAlbum {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    pub id: u64,
    pub track_number: u32,
    pub artist_id: u64,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub album_id: u64,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
}

impl From<TidalTrack> for Track {
    fn from(value: TidalTrack) -> Self {
        Self {
            id: value.id.into(),
            number: value.track_number as i32,
            title: value.title,
            duration: value.duration as f64,
            album: value.album,
            album_id: value.album_id.into(),
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
            source: super::TrackApiSource::Tidal,
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
        Ok(TidalTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("trackNumber")?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            artist: self.to_nested_value(&["artist", "name"])?,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchTrack {
    pub id: u64,
    pub track_number: u32,
    pub artists: Vec<TidalSearchArtist>,
    pub artist_cover: Option<String>,
    pub album_id: u64,
    pub album: String,
    pub album_cover: Option<String>,
    pub audio_quality: String,
    pub copyright: Option<String>,
    pub duration: u32,
    pub explicit: bool,
    pub isrc: String,
    pub popularity: u32,
    pub title: String,
    pub media_metadata_tags: Vec<String>,
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

#[derive(Serialize, Deserialize)]
pub struct TidalSearchResultList<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
}

impl<'a, T> ToValueType<TidalSearchResultList<T>> for &'a Value
where
    Value: AsModelResult<TidalSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<TidalSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<TidalSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Value: ToValueType<T>,
    for<'a> &'a Value: ToValueType<usize>,
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

#[derive(Serialize, Deserialize)]
pub struct TidalSearchResults {
    pub albums: TidalSearchResultList<TidalSearchAlbum>,
    pub artists: TidalSearchResultList<TidalArtist>,
    pub tracks: TidalSearchResultList<TidalSearchTrack>,
}

impl ToValueType<TidalSearchResults> for &Value {
    fn to_value_type(self) -> Result<TidalSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<TidalSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<TidalSearchResults, ParseError> {
        Ok(TidalSearchResults {
            albums: self.to_value("albums")?,
            artists: self.to_value("artists")?,
            tracks: self.to_value("tracks")?,
        })
    }
}
