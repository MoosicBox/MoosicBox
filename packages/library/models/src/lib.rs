#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{path::PathBuf, str::FromStr as _};

#[cfg(feature = "db")]
use moosicbox_core::sqlite::db::{get_album_version_qualities, DbError};
use moosicbox_core::{
    sqlite::models::{
        Album, AlbumSource, AlbumVersionQuality, ApiAlbumVersionQuality, ApiSource, ApiSources,
        Artist, Id, ToApi, Track, TrackApiSource,
    },
    types::AudioFormat,
};
#[cfg(feature = "db")]
use moosicbox_database::{AsId, Database, DatabaseValue};
#[cfg(feature = "db")]
use moosicbox_json_utils::database::ToValue as _;
#[cfg(feature = "db")]
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryArtist {
    pub id: u64,
    pub title: String,
    pub cover: Option<String>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl From<LibraryArtist> for Artist {
    fn from(value: LibraryArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            cover: value.cover,
            source: ApiSource::Library,
            sources: ApiSources::default()
                .with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into))
                .with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into))
                .with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into)),
        }
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModel<LibraryArtist> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryArtist {
        moosicbox_core::sqlite::models::AsModelResult::as_model(self).unwrap()
    }
}

#[cfg(feature = "db")]
impl ToValueType<LibraryArtist> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryArtist, ParseError> {
        Ok(LibraryArtist {
            id: self.to_value("id")?,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModelResult<LibraryArtist, ParseError>
    for &moosicbox_database::Row
{
    fn as_model(&self) -> Result<LibraryArtist, ParseError> {
        Ok(LibraryArtist {
            id: self.to_value("id")?,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl AsId for LibraryArtist {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id.try_into().unwrap())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryArtist {
    pub artist_id: u64,
    pub title: String,
    pub contains_cover: bool,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiArtist {
    Library(ApiLibraryArtist),
}

impl ToApi<ApiArtist> for LibraryArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Library(ApiLibraryArtist {
            artist_id: self.id,
            title: self.title.clone(),
            contains_cover: self.cover.is_some(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id,
            yt_id: self.yt_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct LibraryAlbum {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
    pub yt_id: Option<u64>,
    pub tidal_artist_id: Option<u64>,
    pub qobuz_artist_id: Option<u64>,
    pub yt_artist_id: Option<u64>,
}

impl From<LibraryAlbum> for Album {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: value.source,
            artist_sources: ApiSources::default()
                .with_source_opt(ApiSource::Tidal, value.tidal_artist_id.map(Into::into))
                .with_source_opt(ApiSource::Qobuz, value.qobuz_artist_id.map(Into::into))
                .with_source_opt(ApiSource::Yt, value.yt_artist_id.map(Into::into)),
            album_sources: ApiSources::default()
                .with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into))
                .with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into))
                .with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into)),
        }
    }
}

impl From<Album> for LibraryAlbum {
    fn from(value: Album) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: AlbumSource::Local,
            tidal_id: value.album_sources.get(ApiSource::Tidal).map(Into::into),
            qobuz_id: value.album_sources.get(ApiSource::Qobuz).map(Into::into),
            yt_id: value.album_sources.get(ApiSource::Yt).map(Into::into),
            tidal_artist_id: value.artist_sources.get(ApiSource::Tidal).map(Into::into),
            qobuz_artist_id: value.artist_sources.get(ApiSource::Qobuz).map(Into::into),
            yt_artist_id: value.artist_sources.get(ApiSource::Yt).map(Into::into),
        }
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModel<LibraryAlbum> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryAlbum {
        moosicbox_core::sqlite::models::AsModelResult::as_model(self).unwrap()
    }
}

#[cfg(feature = "db")]
impl MissingValue<LibraryAlbum> for &moosicbox_database::Row {}
#[cfg(feature = "db")]
impl ToValueType<LibraryAlbum> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryAlbum, ParseError> {
        Ok(LibraryAlbum {
            id: self.to_value("id")?,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModelResult<LibraryAlbum, ParseError>
    for &moosicbox_database::Row
{
    fn as_model(&self) -> Result<LibraryAlbum, ParseError> {
        Ok(LibraryAlbum {
            id: self.to_value("id")?,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

#[must_use]
pub const fn track_source_to_u8(source: TrackApiSource) -> u8 {
    match source {
        TrackApiSource::Local => 1,
        TrackApiSource::Tidal => 2,
        TrackApiSource::Qobuz => 3,
        TrackApiSource::Yt => 4,
    }
}

pub fn sort_album_versions(versions: &mut [AlbumVersionQuality]) {
    versions.sort_by(|a, b| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a, b| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });
    versions.sort_by(|a, b| track_source_to_u8(a.source).cmp(&track_source_to_u8(b.source)));
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModelResultMapped<LibraryAlbum, DbError>
    for Vec<moosicbox_database::Row>
{
    #[allow(clippy::too_many_lines)]
    fn as_model_mapped(&self) -> Result<Vec<LibraryAlbum>, DbError> {
        let mut results: Vec<LibraryAlbum> = vec![];
        let mut last_album_id = 0;

        for row in self {
            let album_id: u64 = row
                .get("album_id")
                .ok_or(DbError::InvalidRequest)?
                .try_into()
                .map_err(|_| DbError::InvalidRequest)?;

            if album_id != last_album_id {
                if let Some(ref mut album) = results.last_mut() {
                    log::trace!(
                        "Sorting versions for album id={} count={}",
                        album.id,
                        album.versions.len()
                    );
                    sort_album_versions(&mut album.versions);
                }
                match row.to_value_type() {
                    Ok(album) => {
                        results.push(album);
                    }
                    Err(err) => {
                        log::error!("Failed to parse Album for album id={}: {err:?}", album_id);
                        continue;
                    }
                }
                last_album_id = album_id;
            }

            if let Some(album) = results.last_mut() {
                if let Some(_source) = row.get("source") {
                    match row.to_value_type() {
                        Ok(version) => {
                            album.versions.push(version);
                            log::trace!(
                                "Added version to album id={} count={}",
                                album.id,
                                album.versions.len()
                            );
                        }
                        Err(err) => {
                            log::error!(
                                "Failed to parse AlbumVersionQuality for album id={}: {err:?}",
                                album.id
                            );
                        }
                    }
                } else {
                    if album.tidal_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Tidal,
                        });
                        log::trace!(
                            "Added Tidal version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                    if album.qobuz_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Qobuz,
                        });
                        log::trace!(
                            "Added Qobuz version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                    if album.yt_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Yt,
                        });
                        log::trace!(
                            "Added Yt version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                }
            }
        }

        if let Some(ref mut album) = results.last_mut() {
            log::trace!(
                "Sorting versions for last album id={} count={}",
                album.id,
                album.versions.len()
            );
            sort_album_versions(&mut album.versions);
        }

        Ok(results)
    }
}

#[cfg(feature = "db")]
#[async_trait::async_trait]
impl moosicbox_core::sqlite::models::AsModelQuery<LibraryAlbum> for &moosicbox_database::Row {
    async fn as_model_query(
        &self,
        db: std::sync::Arc<Box<dyn Database>>,
    ) -> Result<LibraryAlbum, DbError> {
        let id = self.to_value("id")?;

        Ok(LibraryAlbum {
            id,
            artist: self
                .to_value::<Option<String>>("artist")?
                .unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: get_album_version_qualities(&db.into(), id).await?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl AsId for LibraryAlbum {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id.try_into().unwrap())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiAlbum {
    Library(ApiLibraryAlbum),
}

impl From<ApiAlbum> for Album {
    fn from(value: ApiAlbum) -> Self {
        let ApiAlbum::Library(album) = value;
        album.into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryAlbum {
    pub album_id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub contains_cover: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<ApiAlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
    pub yt_id: Option<u64>,
}

impl From<ApiLibraryAlbum> for Album {
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: if value.contains_cover {
                Some(value.album_id.to_string())
            } else {
                None
            },
            directory: None,
            blur: value.blur,
            versions: vec![],
            source: value.source,
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        }
    }
}

impl ToApi<ApiLibraryAlbum> for LibraryAlbum {
    fn to_api(self) -> ApiLibraryAlbum {
        ApiLibraryAlbum {
            album_id: self.id,
            title: self.title,
            artist: self.artist,
            artist_id: self.artist_id,
            contains_cover: self.artwork.is_some(),
            date_released: self.date_released,
            date_added: self.date_added,
            source: self.source,
            blur: self.blur,
            versions: self
                .versions
                .iter()
                .map(moosicbox_core::sqlite::models::ToApi::to_api)
                .collect(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id,
            yt_id: self.yt_id,
        }
    }
}

impl ToApi<ApiAlbum> for LibraryAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Library(self.to_api())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrack {
    pub id: u64,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: u64,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: u64,
    pub file: Option<String>,
    pub artwork: Option<String>,
    pub blur: bool,
    pub bytes: u64,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl LibraryTrack {
    #[must_use]
    /// # Panics
    ///
    /// Will panic if directory doesn't exist
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

impl From<LibraryTrack> for Track {
    fn from(value: LibraryTrack) -> Self {
        Self {
            id: value.id.into(),
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: value.file,
            artwork: value.artwork,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
            api_source: ApiSource::Library,
            sources: ApiSources::default()
                .with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into))
                .with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into))
                .with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into)),
        }
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModel<LibraryTrack> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryTrack {
        moosicbox_core::sqlite::models::AsModelResult::as_model(self).unwrap()
    }
}

#[cfg(feature = "db")]
impl ToValueType<LibraryTrack> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryTrack, ParseError> {
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_id: self.to_value("tidal_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModelResult<LibraryTrack, ParseError>
    for &moosicbox_database::Row
{
    fn as_model(&self) -> Result<LibraryTrack, ParseError> {
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_id: self.to_value("tidal_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

#[cfg(feature = "db")]
impl AsId for LibraryTrack {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id.try_into().unwrap())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
enum ApiTrackInner {
    Library(ApiLibraryTrack),
    Tidal(serde_json::Value),
    Qobuz(serde_json::Value),
    Yt(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiTrack {
    Library {
        track_id: u64,
        data: ApiLibraryTrack,
    },
    Tidal {
        track_id: u64,
        data: serde_json::Value,
    },
    Qobuz {
        track_id: u64,
        data: serde_json::Value,
    },
    Yt {
        track_id: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Error)]
pub enum TryFromApiTrackError {
    #[error("Missing field")]
    MissingField,
    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),
}

impl TryFrom<&ApiTrack> for Track {
    type Error = TryFromApiTrackError;

    fn try_from(value: &ApiTrack) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

impl TryFrom<ApiTrack> for Track {
    type Error = TryFromApiTrackError;

    #[allow(clippy::too_many_lines)]
    fn try_from(value: ApiTrack) -> Result<Self, Self::Error> {
        Ok(match value {
            ApiTrack::Library { track_id, data } => Self {
                id: track_id.into(),
                number: data.number,
                title: data.title,
                duration: data.duration,
                album: data.album,
                album_id: data.album_id.into(),
                date_released: data.date_released,
                date_added: data.date_added,
                artist: data.artist,
                artist_id: data.artist_id.into(),
                file: None,
                artwork: if data.contains_cover {
                    Some(data.album_id.to_string())
                } else {
                    None
                },
                blur: data.blur,
                bytes: data.bytes,
                format: data.format,
                bit_depth: data.bit_depth,
                audio_bitrate: data.audio_bitrate,
                overall_bitrate: data.overall_bitrate,
                sample_rate: data.sample_rate,
                channels: data.channels,
                source: data.source,
                api_source: ApiSource::Library,
                sources: ApiSources::default(),
            },
            ApiTrack::Tidal { track_id, data } => {
                let album_id = data
                    .get("albumId")
                    .ok_or(TryFromApiTrackError::MissingField)?
                    .as_u64()
                    .ok_or(TryFromApiTrackError::MissingField)?;
                Self {
                    id: track_id.into(),
                    number: u32::try_from(
                        data.get("number")
                            .ok_or(TryFromApiTrackError::MissingField)?
                            .as_u64()
                            .ok_or(TryFromApiTrackError::MissingField)?,
                    )?,
                    title: data
                        .get("title")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    #[allow(clippy::cast_precision_loss)]
                    duration: data
                        .get("duration")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        as f64,
                    album: data
                        .get("album")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    album_id: album_id.into(),
                    date_released: data
                        .get("dateReleased")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    date_added: data
                        .get("dateAdded")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    artist: data
                        .get("artist")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    artist_id: data
                        .get("artistId")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .into(),
                    file: None,
                    artwork: if data
                        .get("containsCover")
                        .is_some_and(|x| x.as_bool().is_some_and(|x| x))
                    {
                        Some(album_id.to_string())
                    } else {
                        None
                    },
                    blur: false,
                    bytes: 0,
                    format: None,
                    bit_depth: None,
                    audio_bitrate: None,
                    overall_bitrate: None,
                    sample_rate: None,
                    channels: None,
                    source: TrackApiSource::Tidal,
                    api_source: ApiSource::Tidal,
                    sources: ApiSources::default().with_source(ApiSource::Tidal, track_id.into()),
                }
            }
            ApiTrack::Qobuz { track_id, data } => {
                let album_id = data
                    .get("albumId")
                    .ok_or(TryFromApiTrackError::MissingField)?
                    .as_str()
                    .ok_or(TryFromApiTrackError::MissingField)?;
                Self {
                    id: track_id.into(),
                    number: u32::try_from(
                        data.get("number")
                            .ok_or(TryFromApiTrackError::MissingField)?
                            .as_u64()
                            .ok_or(TryFromApiTrackError::MissingField)?,
                    )?,
                    title: data
                        .get("title")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    #[allow(clippy::cast_precision_loss)]
                    duration: data
                        .get("duration")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        as f64,
                    album: data
                        .get("album")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    album_id: album_id.into(),
                    date_released: data
                        .get("dateReleased")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    date_added: data
                        .get("dateAdded")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    artist: data
                        .get("artist")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    artist_id: data
                        .get("artistId")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .into(),
                    file: None,
                    artwork: if data
                        .get("containsCover")
                        .is_some_and(|x| x.as_bool().is_some_and(|x| x))
                    {
                        Some(album_id.to_string())
                    } else {
                        None
                    },
                    blur: false,
                    bytes: 0,
                    format: None,
                    bit_depth: None,
                    audio_bitrate: None,
                    overall_bitrate: None,
                    sample_rate: None,
                    channels: None,
                    source: TrackApiSource::Qobuz,
                    api_source: ApiSource::Qobuz,
                    sources: ApiSources::default().with_source(ApiSource::Qobuz, track_id.into()),
                }
            }
            ApiTrack::Yt { track_id, data } => {
                let album_id = data
                    .get("albumId")
                    .ok_or(TryFromApiTrackError::MissingField)?
                    .as_u64()
                    .ok_or(TryFromApiTrackError::MissingField)?;
                Self {
                    id: track_id.clone().into(),
                    number: u32::try_from(
                        data.get("number")
                            .ok_or(TryFromApiTrackError::MissingField)?
                            .as_u64()
                            .ok_or(TryFromApiTrackError::MissingField)?,
                    )?,
                    title: data
                        .get("title")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    #[allow(clippy::cast_precision_loss)]
                    duration: data
                        .get("duration")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        as f64,
                    album: data
                        .get("album")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    album_id: album_id.into(),
                    date_released: data
                        .get("dateReleased")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    date_added: data
                        .get("dateAdded")
                        .and_then(|x| x.as_str().map(str::to_string)),
                    artist: data
                        .get("artist")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_str()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .to_string(),
                    artist_id: data
                        .get("artistId")
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .as_u64()
                        .ok_or(TryFromApiTrackError::MissingField)?
                        .into(),
                    file: None,
                    artwork: if data
                        .get("containsCover")
                        .is_some_and(|x| x.as_bool().is_some_and(|x| x))
                    {
                        Some(album_id.to_string())
                    } else {
                        None
                    },
                    blur: false,
                    bytes: 0,
                    format: None,
                    bit_depth: None,
                    audio_bitrate: None,
                    overall_bitrate: None,
                    sample_rate: None,
                    channels: None,
                    source: TrackApiSource::Yt,
                    api_source: ApiSource::Yt,
                    sources: ApiSources::default().with_source(ApiSource::Yt, track_id.into()),
                }
            }
        })
    }
}

impl Serialize for ApiTrack {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Library { data, .. } => {
                ApiTrackInner::Library(data.clone()).serialize(serializer)
            }
            Self::Tidal { data, .. } => ApiTrackInner::Tidal(data.clone()).serialize(serializer),
            Self::Qobuz { data, .. } => ApiTrackInner::Qobuz(data.clone()).serialize(serializer),
            Self::Yt { data, .. } => ApiTrackInner::Yt(data.clone()).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ApiTrack {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match ApiTrackInner::deserialize(deserializer)? {
            ApiTrackInner::Library(track) => Self::Library {
                track_id: track.track_id,
                data: track,
            },
            ApiTrackInner::Tidal(data) => Self::Tidal {
                track_id: data
                    .get("id")
                    .expect("Failed to get tidal track id")
                    .as_u64()
                    .unwrap(),
                data,
            },
            ApiTrackInner::Qobuz(data) => Self::Qobuz {
                track_id: data
                    .get("id")
                    .expect("Failed to get qobuz track id")
                    .as_u64()
                    .unwrap(),
                data,
            },
            ApiTrackInner::Yt(data) => Self::Yt {
                track_id: data
                    .get("id")
                    .expect("Failed to get yt track id")
                    .as_str()
                    .unwrap()
                    .to_string(),
                data,
            },
        })
    }
}

impl ApiTrack {
    #[must_use]
    pub const fn api_source(&self) -> ApiSource {
        match self {
            Self::Library { .. } => ApiSource::Library,
            Self::Tidal { .. } => ApiSource::Tidal,
            Self::Qobuz { .. } => ApiSource::Qobuz,
            Self::Yt { .. } => ApiSource::Yt,
        }
    }

    #[must_use]
    pub fn track_id(&self) -> Id {
        match self {
            Self::Library { track_id, .. }
            | Self::Tidal { track_id, .. }
            | Self::Qobuz { track_id, .. } => track_id.into(),
            Self::Yt { track_id, .. } => track_id.into(),
        }
    }

    #[must_use]
    pub fn data(&self) -> Option<serde_json::Value> {
        match self {
            Self::Library { .. } => None,
            Self::Tidal { data, .. } | Self::Qobuz { data, .. } | Self::Yt { data, .. } => {
                Some(data.clone())
            }
        }
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
            source: value.source,
            api_source: ApiSource::Library,
            sources: ApiSources::default().with_source(ApiSource::Library, value.track_id.into()),
        }
    }
}

impl ToApi<ApiTrack> for LibraryTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Library {
            track_id: self.id,
            data: ApiLibraryTrack {
                track_id: self.id,
                number: self.number,
                title: self.title.clone(),
                duration: self.duration,
                artist: self.artist.clone(),
                artist_id: self.artist_id,
                date_released: self.date_released.clone(),
                date_added: self.date_added.clone(),
                album: self.album.clone(),
                album_id: self.album_id,
                contains_cover: self.artwork.is_some(),
                blur: self.blur,
                bytes: self.bytes,
                format: self.format,
                bit_depth: self.bit_depth,
                audio_bitrate: self.audio_bitrate,
                overall_bitrate: self.overall_bitrate,
                sample_rate: self.sample_rate,
                channels: self.channels,
                source: self.source,
            },
        }
    }
}
