#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{path::PathBuf, str::FromStr as _};

#[cfg(feature = "db")]
use moosicbox_core::sqlite::db::{get_album_version_qualities, DbError};
use moosicbox_core::{
    sqlite::models::{
        Album, AlbumSource, AlbumVersionQuality, ApiAlbum, ApiAlbumVersionQuality, ApiSource,
        ApiSources, ApiTrack, Artist, ToApi, Track, TrackApiSource,
    },
    types::AudioFormat,
};
#[cfg(feature = "db")]
use moosicbox_database::{AsId, Database, DatabaseValue};
#[cfg(feature = "db")]
use moosicbox_json_utils::database::ToValue as _;
#[cfg(feature = "db")]
use moosicbox_json_utils::MissingValue;
use moosicbox_json_utils::{ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryArtist {
    pub id: u64,
    pub title: String,
    pub cover: Option<String>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl From<LibraryArtist> for moosicbox_core::sqlite::models::ApiArtist {
    fn from(value: LibraryArtist) -> Self {
        let artist: Artist = value.into();
        artist.into()
    }
}

impl From<LibraryArtist> for Artist {
    fn from(value: LibraryArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            cover: value.cover,
            api_source: ApiSource::Library,
            api_sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default();
                #[cfg(feature = "tidal")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into));
                }
                #[cfg(feature = "qobuz")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into));
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into));
                }
                sources
            },
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

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumType {
    #[default]
    Lp,
    Live,
    Compilations,
    EpsAndSingles,
    Other,
}

impl From<moosicbox_core::sqlite::models::AlbumType> for LibraryAlbumType {
    fn from(value: moosicbox_core::sqlite::models::AlbumType) -> Self {
        match value {
            moosicbox_core::sqlite::models::AlbumType::Lp => Self::Lp,
            moosicbox_core::sqlite::models::AlbumType::Live => Self::Live,
            moosicbox_core::sqlite::models::AlbumType::Compilations => Self::Compilations,
            moosicbox_core::sqlite::models::AlbumType::EpsAndSingles => Self::EpsAndSingles,
            moosicbox_core::sqlite::models::AlbumType::Other
            | moosicbox_core::sqlite::models::AlbumType::Download => Self::Other,
        }
    }
}

impl From<LibraryAlbumType> for moosicbox_core::sqlite::models::AlbumType {
    fn from(value: LibraryAlbumType) -> Self {
        match value {
            LibraryAlbumType::Lp => Self::Lp,
            LibraryAlbumType::Live => Self::Live,
            LibraryAlbumType::Compilations => Self::Compilations,
            LibraryAlbumType::EpsAndSingles => Self::EpsAndSingles,
            LibraryAlbumType::Other => Self::Other,
        }
    }
}

#[cfg(feature = "db")]
impl MissingValue<LibraryAlbumType> for &moosicbox_database::Row {}
#[cfg(feature = "db")]
impl ToValueType<LibraryAlbumType> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        self.get("album_type")
            .ok_or_else(|| ParseError::MissingValue("album_type".into()))?
            .to_value_type()
    }
}
#[cfg(feature = "db")]
impl ToValueType<LibraryAlbumType> for DatabaseValue {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        LibraryAlbumType::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("AlbumType".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("AlbumType".into()))
    }
}

impl ToValueType<LibraryAlbumType> for &serde_json::Value {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        LibraryAlbumType::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("AlbumType".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("AlbumType".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct LibraryAlbum {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub album_sources: ApiSources,
    pub artist_sources: ApiSources,
}

impl From<&LibraryAlbum> for ApiAlbum {
    fn from(value: &LibraryAlbum) -> Self {
        let album: Album = value.clone().into();
        album.into()
    }
}

impl From<LibraryAlbum> for ApiAlbum {
    fn from(value: LibraryAlbum) -> Self {
        let album: Album = value.into();
        album.into()
    }
}

impl From<LibraryAlbum> for Album {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::Library,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
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
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: AlbumSource::Local,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
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
        #[cfg(any(feature = "tidal", feature = "qobuz", feature = "yt"))]
        use moosicbox_core::sqlite::models::Id;

        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        #[cfg(feature = "tidal")]
        let tidal_id: Option<Id> = self.to_value("tidal_id")?;
        #[cfg(feature = "tidal")]
        let tidal_artist_id: Option<Id> = self.to_value("tidal_artist_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_id: Option<Id> = self.to_value("qobuz_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_artist_id: Option<Id> = self.to_value("qobuz_artist_id")?;
        #[cfg(feature = "yt")]
        let yt_id: Option<Id> = self.to_value("yt_id")?;
        #[cfg(feature = "yt")]
        let yt_artist_id: Option<Id> = self.to_value("yt_artist_id")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;

        Ok(LibraryAlbum {
            id,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            album_sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default().with_source(ApiSource::Library, id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_id);
                }

                sources
            },
            artist_sources: {
                #[allow(unused_mut)]
                let mut sources =
                    ApiSources::default().with_source(ApiSource::Library, artist_id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_artist_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_artist_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_artist_id);
                }

                sources
            },
        })
    }
}

#[cfg(feature = "db")]
impl moosicbox_core::sqlite::models::AsModelResult<LibraryAlbum, ParseError>
    for &moosicbox_database::Row
{
    fn as_model(&self) -> Result<LibraryAlbum, ParseError> {
        #[cfg(any(feature = "tidal", feature = "qobuz", feature = "yt"))]
        use moosicbox_core::sqlite::models::Id;

        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        #[cfg(feature = "tidal")]
        let tidal_id: Option<Id> = self.to_value("tidal_id")?;
        #[cfg(feature = "tidal")]
        let tidal_artist_id: Option<Id> = self.to_value("tidal_artist_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_id: Option<Id> = self.to_value("qobuz_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_artist_id: Option<Id> = self.to_value("qobuz_artist_id")?;
        #[cfg(feature = "yt")]
        let yt_id: Option<Id> = self.to_value("yt_id")?;
        #[cfg(feature = "yt")]
        let yt_artist_id: Option<Id> = self.to_value("yt_artist_id")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;

        Ok(LibraryAlbum {
            id,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            album_sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default().with_source(ApiSource::Library, id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_id);
                }

                sources
            },
            artist_sources: {
                #[allow(unused_mut)]
                let mut sources =
                    ApiSources::default().with_source(ApiSource::Library, artist_id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_artist_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_artist_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_artist_id);
                }

                sources
            },
        })
    }
}

#[must_use]
pub const fn track_source_to_u8(source: TrackApiSource) -> u8 {
    match source {
        TrackApiSource::Local => 1,
        #[cfg(feature = "tidal")]
        TrackApiSource::Tidal => 2,
        #[cfg(feature = "qobuz")]
        TrackApiSource::Qobuz => 3,
        #[cfg(feature = "yt")]
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
                    #[cfg(feature = "tidal")]
                    if album
                        .album_sources
                        .iter()
                        .any(|x| x.source == ApiSource::Tidal)
                    {
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
                    #[cfg(feature = "qobuz")]
                    if album
                        .album_sources
                        .iter()
                        .any(|x| x.source == ApiSource::Qobuz)
                    {
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
                    #[cfg(feature = "yt")]
                    if album
                        .album_sources
                        .iter()
                        .any(|x| x.source == ApiSource::Yt)
                    {
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
        #[cfg(any(feature = "tidal", feature = "qobuz", feature = "yt"))]
        use moosicbox_core::sqlite::models::Id;

        #[cfg(feature = "tidal")]
        let tidal_id: Option<Id> = self.to_value("tidal_id")?;
        #[cfg(feature = "tidal")]
        let tidal_artist_id: Option<Id> = self.to_value("tidal_artist_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_id: Option<Id> = self.to_value("qobuz_id")?;
        #[cfg(feature = "qobuz")]
        let qobuz_artist_id: Option<Id> = self.to_value("qobuz_artist_id")?;
        #[cfg(feature = "yt")]
        let yt_id: Option<Id> = self.to_value("yt_id")?;
        #[cfg(feature = "yt")]
        let yt_artist_id: Option<Id> = self.to_value("yt_artist_id")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        Ok(LibraryAlbum {
            id,
            artist: self
                .to_value::<Option<String>>("artist")?
                .unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: get_album_version_qualities(&db.into(), id).await?,
            album_sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default().with_source(ApiSource::Library, id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_id);
                }

                sources
            },
            artist_sources: {
                #[allow(unused_mut)]
                let mut sources =
                    ApiSources::default().with_source(ApiSource::Library, artist_id.into());

                #[cfg(feature = "tidal")]
                {
                    sources = sources.with_source_opt(ApiSource::Tidal, tidal_artist_id);
                }
                #[cfg(feature = "qobuz")]
                {
                    sources = sources.with_source_opt(ApiSource::Qobuz, qobuz_artist_id);
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, yt_artist_id);
                }

                sources
            },
        })
    }
}

#[cfg(feature = "db")]
impl AsId for LibraryAlbum {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id.try_into().unwrap())
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

impl From<ApiLibraryAlbum> for moosicbox_core::sqlite::models::ApiAlbum {
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

impl From<ApiLibraryAlbum> for Album {
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
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
            album_source: value.source,
            api_source: ApiSource::Library,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
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
            album_type: self.album_type,
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
            album_sources: self.album_sources,
            artist_sources: self.artist_sources,
        }
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
    pub album_type: LibraryAlbumType,
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
    pub api_source: ApiSource,
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

impl From<LibraryTrack> for ApiTrack {
    fn from(value: LibraryTrack) -> Self {
        let track: Track = value.into();
        track.into()
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
            album_type: value.album_type.into(),
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
            track_source: value.source,
            api_source: ApiSource::Library,
            sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default();
                #[cfg(feature = "tidal")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into));
                }
                #[cfg(feature = "qobuz")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into));
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into));
                }
                sources
            },
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
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            album_type: album_type.unwrap_or_default(),
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
            api_source: ApiSource::Library,
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
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            album_type: album_type.unwrap_or_default(),
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
            api_source: ApiSource::Library,
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
