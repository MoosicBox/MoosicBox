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
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
    pub qobuz_id: u64,
    pub released_at: u64,
    pub release_date_original: String,
    pub duration: u32,
    pub parental_warning: bool,
    pub popularity: u32,
    pub tracks_count: u32,
    pub genre: QobuzGenre,
    pub maximum_channel_count: u16,
    pub maximum_sampling_rate: f32,
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

impl AsModelResult<QobuzAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzAlbum, ParseError> {
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
            qobuz_id: self.to_value("qobuz_id")?,
            released_at: self.to_value("released_at")?,
            release_date_original: self.to_value("release_date_original")?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            popularity: self.to_value("popularity")?,
            tracks_count: self.to_value("tracks_count")?,
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
    pub maximum_bit_depth: u16,
    pub image: Option<QobuzImage>,
    pub title: String,
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
        Ok(QobuzRelease {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name", "display"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            maximum_bit_depth: self.to_nested_value(&["audio_info", "maximum_bit_depth"])?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            release_date_original: self.to_nested_value(&["dates", "original"])?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            tracks_count: self.to_value("tracks_count")?,
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
    pub image: Option<QobuzImage>,
    pub copyright: Option<String>,
    pub duration: u32,
    pub parental_warning: bool,
    pub isrc: String,
    pub title: String,
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
    pub fn from_value(
        value: &Value,
        artist: &str,
        artist_id: u64,
        album: &str,
        album_id: &str,
        image: Option<QobuzImage>,
    ) -> Result<QobuzTrack, ParseError> {
        Ok(QobuzTrack {
            id: value.to_value("id")?,
            track_number: value.to_value("track_number")?,
            artist: artist.to_string(),
            artist_id,
            album: album.to_string(),
            album_id: album_id.to_string(),
            image,
            copyright: value.to_value("copyright")?,
            duration: value.to_value("duration")?,
            parental_warning: value.to_value("parental_warning")?,
            isrc: value.to_value("isrc")?,
            title: value.to_value("title")?,
        })
    }
}

impl AsModelResult<QobuzTrack, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzTrack, ParseError> {
        Ok(QobuzTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("track_number")?,
            album: self.to_nested_value(&["album", "title"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            artist: self.to_nested_value(&["album", "artist", "name"])?,
            artist_id: self.to_nested_value(&["album", "artist", "id"])?,
            image: self.to_value("image")?,
            copyright: self.to_value("copyright")?,
            duration: self.to_value("duration")?,
            parental_warning: self.to_value("parental_warning")?,
            isrc: self.to_value("isrc")?,
            title: self.to_value("title")?,
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

impl From<QobuzArtist> for Artist {
    fn from(value: QobuzArtist) -> Self {
        Artist::Qobuz(value)
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

impl From<QobuzAlbum> for Album {
    fn from(value: QobuzAlbum) -> Self {
        Album::Qobuz(value)
    }
}

impl From<QobuzTrack> for Track {
    fn from(value: QobuzTrack) -> Self {
        Track::Qobuz(value)
    }
}
impl From<&QobuzArtist> for Artist {
    fn from(value: &QobuzArtist) -> Self {
        Artist::Qobuz(value.clone())
    }
}

impl From<&QobuzAlbum> for Album {
    fn from(value: &QobuzAlbum) -> Self {
        Album::Qobuz(value.clone())
    }
}

impl From<&QobuzTrack> for Track {
    fn from(value: &QobuzTrack) -> Self {
        Track::Qobuz(value.clone())
    }
}
