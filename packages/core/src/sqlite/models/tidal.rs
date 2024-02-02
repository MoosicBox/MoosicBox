use std::fmt::Display;

use moosicbox_json_utils::{
    serde_json::{ToNestedValue, ToValue},
    MissingValue, ParseError, ToValueType,
};
use serde::{Deserialize, Serialize};

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

impl MissingValue<TidalArtist> for &serde_json::Value {}
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

impl TidalAlbum {
    pub fn cover_url(&self, size: TidalAlbumImageSize) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.tidal.com/images/{cover_path}/{size}x{size}.jpg")
        })
    }
}

impl MissingValue<TidalAlbum> for &serde_json::Value {}
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

impl MissingValue<TidalTrack> for &serde_json::Value {}
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

impl From<TidalArtist> for Artist {
    fn from(value: TidalArtist) -> Self {
        Artist::Tidal(value)
    }
}

impl From<TidalAlbum> for Album {
    fn from(value: TidalAlbum) -> Self {
        Album::Tidal(value)
    }
}

impl From<TidalTrack> for Track {
    fn from(value: TidalTrack) -> Self {
        Track::Tidal(value)
    }
}
impl From<&TidalArtist> for Artist {
    fn from(value: &TidalArtist) -> Self {
        Artist::Tidal(value.clone())
    }
}

impl From<&TidalAlbum> for Album {
    fn from(value: &TidalAlbum) -> Self {
        Album::Tidal(value.clone())
    }
}

impl From<&TidalTrack> for Track {
    fn from(value: &TidalTrack) -> Self {
        Track::Tidal(value.clone())
    }
}
