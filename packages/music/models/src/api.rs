use moosicbox_date_utils::chrono::{self, parse_date_time};
use serde::{Deserialize, Serialize};

use crate::{
    Album, AlbumSource, AlbumType, AlbumVersionQuality, ApiSource, ApiSources, Artist, AudioFormat,
    Track, TrackApiSource, id::Id,
};

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiArtist {
    pub artist_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub api_source: ApiSource,
    pub api_sources: ApiSources,
}

impl From<Artist> for ApiArtist {
    fn from(value: Artist) -> Self {
        Self {
            artist_id: value.id,
            title: value.title,
            contains_cover: value.cover.is_some(),
            api_source: value.api_source,
            api_sources: value.api_sources,
        }
    }
}

impl From<ApiArtist> for Artist {
    fn from(value: ApiArtist) -> Self {
        Self {
            id: value.artist_id.clone(),
            title: value.title,
            cover: if value.contains_cover {
                Some(value.artist_id.to_string())
            } else {
                None
            },
            api_source: value.api_source,
            api_sources: value.api_sources,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

impl From<ApiAlbumVersionQuality> for AlbumVersionQuality {
    fn from(value: ApiAlbumVersionQuality) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

impl From<AlbumVersionQuality> for ApiAlbumVersionQuality {
    fn from(value: AlbumVersionQuality) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiTrack {
    pub track_id: Id,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: Id,
    pub album_type: AlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: Id,
    pub contains_cover: bool,
    pub blur: bool,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub track_source: TrackApiSource,
    pub api_source: ApiSource,
    pub sources: ApiSources,
}

impl From<Track> for ApiTrack {
    fn from(value: Track) -> Self {
        Self {
            track_id: value.id,
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id,
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.track_source,
            api_source: value.api_source,
            sources: value.sources,
        }
    }
}

impl From<ApiTrack> for Track {
    fn from(value: ApiTrack) -> Self {
        Self {
            id: value.track_id.clone(),
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id,
            file: None,
            artwork: if value.contains_cover {
                Some(value.track_id.to_string())
            } else {
                None
            },
            blur: value.blur,
            bytes: 0,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.track_source,
            api_source: value.api_source,
            sources: value.sources,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbum {
    pub album_id: Id,
    pub title: String,
    pub artist: String,
    pub artist_id: Id,
    pub album_type: AlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub contains_cover: bool,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub album_source: AlbumSource,
    pub api_source: ApiSource,
    pub artist_sources: ApiSources,
    pub album_sources: ApiSources,
}

impl From<Album> for ApiAlbum {
    fn from(value: Album) -> Self {
        Self {
            album_id: value.id,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released.map(|x| x.and_utc().to_rfc3339()),
            date_added: value.date_added.map(|x| x.and_utc().to_rfc3339()),
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: value.versions,
            album_source: value.album_source,
            api_source: value.api_source,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        }
    }
}

impl TryFrom<ApiAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: ApiAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.album_id.clone(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value
                .date_released
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            date_added: value
                .date_added
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            artwork: if value.contains_cover {
                Some(value.album_id.to_string())
            } else {
                None
            },
            blur: value.blur,
            versions: value.versions,
            album_source: value.album_source,
            api_source: value.api_source,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
            ..Default::default()
        })
    }
}

impl From<&ApiTrack> for ApiAlbum {
    fn from(value: &ApiTrack) -> Self {
        value.clone().into()
    }
}

impl From<ApiTrack> for ApiAlbum {
    fn from(value: ApiTrack) -> Self {
        Self {
            album_id: value.album_id,
            title: value.album,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.contains_cover,
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        }
    }
}

impl From<&Track> for ApiAlbum {
    fn from(value: &Track) -> Self {
        value.clone().into()
    }
}

impl From<Track> for ApiAlbum {
    fn from(value: Track) -> Self {
        Self {
            album_id: value.album_id,
            title: value.album,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        }
    }
}
