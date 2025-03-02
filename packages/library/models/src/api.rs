use moosicbox_date_utils::chrono::{self, parse_date_time};
use moosicbox_music_models::{
    api::{ApiAlbum, ApiAlbumVersionQuality, ApiArtist, ApiTrack},
    Album, AlbumSource, ApiSource, ApiSources, Artist, AudioFormat, Track, TrackApiSource,
};
use serde::{Deserialize, Serialize};

use crate::{LibraryAlbum, LibraryAlbumType, LibraryArtist, LibraryTrack};

impl From<LibraryArtist> for ApiArtist {
    fn from(value: LibraryArtist) -> Self {
        let artist: Artist = value.into();
        artist.into()
    }
}

impl From<&LibraryAlbum> for ApiAlbum {
    fn from(value: &LibraryAlbum) -> Self {
        value.clone().into()
    }
}

impl From<LibraryAlbum> for ApiAlbum {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            album_id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::Library,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryAlbum {
    pub album_id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub contains_cover: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<ApiAlbumVersionQuality>,
    pub album_sources: ApiSources,
    pub artist_sources: ApiSources,
}

impl From<ApiLibraryAlbum> for ApiAlbum {
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            album_id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.contains_cover,
            blur: value.blur,
            versions: value
                .versions
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            album_source: value.source,
            api_source: ApiSource::Library,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl TryFrom<ApiLibraryAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: ApiLibraryAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
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
            directory: None,
            blur: value.blur,
            versions: vec![],
            album_source: value.source,
            api_source: ApiSource::Library,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        })
    }
}

impl From<LibraryAlbum> for ApiLibraryAlbum {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            album_id: value.id,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            contains_cover: value.artwork.is_some(),
            date_released: value.date_released,
            date_added: value.date_added,
            source: value.source,
            blur: value.blur,
            versions: value.versions.into_iter().map(Into::into).collect(),
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl From<LibraryTrack> for ApiTrack {
    fn from(value: LibraryTrack) -> Self {
        let track: Track = value.into();
        track.into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryTrack {
    pub track_id: u64,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub album: String,
    pub album_id: u64,
    pub contains_cover: bool,
    pub blur: bool,
    pub bytes: u64,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
    pub api_source: ApiSource,
}

impl From<&ApiLibraryTrack> for LibraryTrack {
    fn from(value: &ApiLibraryTrack) -> Self {
        value.clone().into()
    }
}

impl From<ApiLibraryTrack> for LibraryTrack {
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id,
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
            artwork: None,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
            api_source: value.api_source,
            qobuz_id: None,
            tidal_id: None,
            yt_id: None,
        }
    }
}

impl From<ApiLibraryTrack> for Track {
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id.into(),
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork: None,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.source,
            api_source: ApiSource::Library,
            sources: ApiSources::default().with_source(ApiSource::Library, value.track_id.into()),
        }
    }
}
