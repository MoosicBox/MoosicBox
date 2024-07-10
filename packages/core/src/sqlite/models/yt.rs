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
pub struct YtArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub popularity: u32,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub enum YtArtistImageSize {
    Max,    // 750
    Large,  // 480
    Medium, // 320
    Small,  // 160
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
            0..=160 => YtArtistImageSize::Small,
            161..=320 => YtArtistImageSize::Medium,
            321..=480 => YtArtistImageSize::Large,
            _ => YtArtistImageSize::Max,
        }
    }
}

impl Display for YtArtistImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl YtArtist {
    pub fn picture_url(&self, size: YtArtistImageSize) -> Option<String> {
        self.picture.as_ref().map(|picture| {
            let picture_path = picture.replace('-', "/");
            format!("https://resources.yt.com/images/{picture_path}/{size}x{size}.jpg")
        })
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchArtist {
    pub id: u64,
    pub picture: Option<String>,
    pub contains_cover: bool,
    pub r#type: String,
    pub name: String,
}

impl YtSearchArtist {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtAlbum {
    pub id: String,
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
pub enum YtAlbumImageSize {
    Max,       // 1280
    Large,     // 640
    Medium,    // 320
    Small,     // 160
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
            0..=80 => YtAlbumImageSize::Thumbnail,
            81..=160 => YtAlbumImageSize::Small,
            161..=320 => YtAlbumImageSize::Medium,
            321..=640 => YtAlbumImageSize::Large,
            _ => YtAlbumImageSize::Max,
        }
    }
}

impl Display for YtAlbumImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtSearchAlbum {
    pub id: u64,
    pub artists: Vec<YtSearchArtist>,
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

impl YtSearchAlbum {
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
pub struct YtTrack {
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
pub struct YtSearchTrack {
    pub id: u64,
    pub track_number: u32,
    pub artists: Vec<YtSearchArtist>,
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

impl From<YtArtist> for Artist {
    fn from(value: YtArtist) -> Self {
        Artist::Yt(value)
    }
}

impl From<YtAlbum> for Album {
    fn from(value: YtAlbum) -> Self {
        Album::Yt(value)
    }
}

impl From<YtTrack> for Track {
    fn from(value: YtTrack) -> Self {
        Track::Yt(value)
    }
}
impl From<&YtArtist> for Artist {
    fn from(value: &YtArtist) -> Self {
        Artist::Yt(value.clone())
    }
}

impl From<&YtAlbum> for Album {
    fn from(value: &YtAlbum) -> Self {
        Album::Yt(value.clone())
    }
}

impl From<&YtTrack> for Track {
    fn from(value: &YtTrack) -> Self {
        Track::Yt(value.clone())
    }
}

#[derive(Serialize, Deserialize)]
pub struct YtSearchResultList<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
}

impl<'a, T> ToValueType<YtSearchResultList<T>> for &'a Value
where
    Value: AsModelResult<YtSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<YtSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<YtSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Value: ToValueType<T>,
    for<'a> &'a Value: ToValueType<usize>,
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

#[derive(Serialize, Deserialize)]
pub struct YtSearchResults {
    pub albums: YtSearchResultList<YtSearchAlbum>,
    pub artists: YtSearchResultList<YtArtist>,
    pub tracks: YtSearchResultList<YtSearchTrack>,
}

impl ToValueType<YtSearchResults> for &Value {
    fn to_value_type(self) -> Result<YtSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<YtSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<YtSearchResults, ParseError> {
        Ok(YtSearchResults {
            albums: self.to_value("albums")?,
            artists: self.to_value("artists")?,
            tracks: self.to_value("tracks")?,
        })
    }
}
