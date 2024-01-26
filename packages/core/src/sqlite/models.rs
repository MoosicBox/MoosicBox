use std::{path::PathBuf, str::FromStr};

use moosicbox_json_utils::{rusqlite::ToValue, MissingValue, ParseError, ToValueType};
use rusqlite::{
    types::{FromSql, Value},
    Row, Rows,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::types::AudioFormat;

use super::db::{
    get_album_version_qualities, get_players, get_session_active_players, get_session_playlist,
    get_session_playlist_tracks, get_tracks, DbError, SqliteValue,
};

pub trait AsModel<T> {
    fn as_model(&self) -> T;
}

pub trait AsModelResult<T, E> {
    fn as_model(&self) -> Result<T, E>;
}

pub trait AsModelResultMappedMut<T, E> {
    fn as_model_mapped_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        Row<'a>: AsModelResult<T, ParseError>;
}

pub trait AsModelResultMappedQuery<T, E> {
    fn as_model_mapped_query(&self, db: &rusqlite::Connection) -> Result<Vec<T>, E>;
}

pub trait AsModelResultMut<T, E> {
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        Row<'a>: AsModelResult<T, ParseError>;
}

impl<T, E> AsModelResultMut<T, E> for Rows<'_>
where
    E: From<DbError>,
{
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        Row<'a>: AsModelResult<T, ParseError>,
    {
        let mut values = vec![];

        while let Some(row) = self.next().map_err(|e| e.into())? {
            match AsModelResult::as_model(row) {
                Ok(value) => values.push(value),
                Err(err) => log::error!("Row error: {err:?}"),
            }
        }

        Ok(values)
    }
}

pub trait AsId {
    fn as_id(&self) -> SqliteValue;
}

pub trait AsModelQuery<T> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<T, DbError>;
}

pub trait ToApi<T> {
    fn to_api(&self) -> T;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NumberId {
    pub id: i32,
}

impl AsModel<NumberId> for Row<'_> {
    fn as_model(&self) -> NumberId {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<NumberId, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<NumberId, ParseError> {
        Ok(NumberId {
            id: self.to_value("id")?,
        })
    }
}

impl AsId for NumberId {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StringId {
    pub id: String,
}

impl AsModel<StringId> for Row<'_> {
    fn as_model(&self) -> StringId {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<StringId, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<StringId, ParseError> {
        Ok(StringId {
            id: self.to_value("id")?,
        })
    }
}

impl AsId for StringId {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.id.clone())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Track {
    Library(LibraryTrack),
    Tidal(TidalTrack),
    Qobuz(QobuzTrack),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzTrack {
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrack {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: i32,
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
    pub source: TrackSource,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
}

impl LibraryTrack {
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

impl AsModel<LibraryTrack> for Row<'_> {
    fn as_model(&self) -> LibraryTrack {
        AsModel::as_model(&self)
    }
}

impl AsModel<LibraryTrack> for &Row<'_> {
    fn as_model(&self) -> LibraryTrack {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<LibraryTrack, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<LibraryTrack, ParseError> {
        AsModelResult::as_model(&self)
    }
}

impl AsModelResult<LibraryTrack, ParseError> for &Row<'_> {
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
            source: TrackSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_id: self.to_value("tidal_id")?,
        })
    }
}

impl AsId for LibraryTrack {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiTrack {
    Library(ApiLibraryTrack),
    Tidal(serde_json::Value),
    Qobuz(serde_json::Value),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryTrack {
    pub track_id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub album: String,
    pub album_id: i32,
    pub contains_cover: bool,
    pub blur: bool,
    pub bytes: u64,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackSource,
}

impl ToApi<ApiTrack> for LibraryTrack {
    fn to_api(&self) -> ApiTrack {
        ApiTrack::Library(ApiLibraryTrack {
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
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ArtistId {
    Library(i32),
    Tidal(u64),
    Qobuz(u64),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Artist {
    pub id: i32,
    pub title: String,
    pub cover: Option<String>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
}

impl AsModel<Artist> for Row<'_> {
    fn as_model(&self) -> Artist {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<Artist, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<Artist, ParseError> {
        Ok(Artist {
            id: self.to_value("id")?,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
        })
    }
}

impl AsId for Artist {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ArtistSort {
    NameAsc,
    NameDesc,
}

impl FromStr for ArtistSort {
    type Err = ();

    fn from_str(input: &str) -> Result<ArtistSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "name-asc" | "name" => Ok(ArtistSort::NameAsc),
            "name-desc" => Ok(ArtistSort::NameDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryArtist {
    pub artist_id: i32,
    pub title: String,
    pub contains_cover: bool,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Library(ApiLibraryArtist),
}

impl ToApi<ApiArtist> for Artist {
    fn to_api(&self) -> ApiArtist {
        ApiArtist::Library(ApiLibraryArtist {
            artist_id: self.id,
            title: self.title.clone(),
            contains_cover: self.cover.is_some(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct AlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackSource,
}

impl ToApi<ApiAlbumVersionQuality> for AlbumVersionQuality {
    fn to_api(&self) -> ApiAlbumVersionQuality {
        ApiAlbumVersionQuality {
            format: self.format,
            bit_depth: self.bit_depth,
            sample_rate: self.sample_rate,
            channels: self.channels,
            source: self.source,
        }
    }
}

impl AsModel<AlbumVersionQuality> for Row<'_> {
    fn as_model(&self) -> AlbumVersionQuality {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<AlbumVersionQuality, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<AlbumVersionQuality, ParseError> {
        Ok(AlbumVersionQuality {
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate")?,
            channels: self.to_value("channels")?,
            source: TrackSource::from_str(&self.to_value::<String>("source")?)
                .map_err(|e| ParseError::ConvertType(format!("Invalid source: {e:?}")))?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum AlbumId {
    Library(i32),
    Tidal(u64),
    Qobuz(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Album {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
    pub tidal_artist_id: Option<u64>,
    pub qobuz_artist_id: Option<u64>,
}

impl AsModel<Album> for Row<'_> {
    fn as_model(&self) -> Album {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<Album, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<Album, ParseError> {
        Ok(Album {
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
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
        })
    }
}

pub fn track_source_to_u8(source: TrackSource) -> u8 {
    match source {
        TrackSource::Local => 1,
        TrackSource::Tidal => 2,
        TrackSource::Qobuz => 3,
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

impl AsModelResultMappedMut<Album, DbError> for Rows<'_> {
    fn as_model_mapped_mut<'a>(&'a mut self) -> Result<Vec<Album>, DbError>
    where
        Row<'a>: AsModelResult<Album, ParseError>,
    {
        let mut results: Vec<Album> = vec![];
        let mut last_album_id = 0;

        while let Some(row) = self.next()? {
            let album_id: i32 = row.get("album_id")?;

            if album_id != last_album_id {
                if let Some(ref mut album) = results.last_mut() {
                    log::trace!(
                        "Sorting versions for album id={} count={}",
                        album.id,
                        album.versions.len()
                    );
                    sort_album_versions(&mut album.versions);
                }
                match AsModelResult::as_model(row) {
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
                if let Some(_source) = row.get::<_, Option<String>>("source")? {
                    match AsModelResult::<AlbumVersionQuality, ParseError>::as_model(row) {
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
                            source: TrackSource::Tidal,
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
                            source: TrackSource::Qobuz,
                        });
                        log::trace!(
                            "Added Qobuz version to album id={} count={}",
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

impl AsModelQuery<Album> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<Album, DbError> {
        let id = self.to_value("id")?;

        Ok(Album {
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
            versions: get_album_version_qualities(db, id)?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
        })
    }
}

impl AsId for Album {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Library(ApiLibraryAlbum),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryAlbum {
    pub album_id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub contains_cover: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<ApiAlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
}

impl ToApi<ApiAlbum> for Album {
    fn to_api(&self) -> ApiAlbum {
        ApiAlbum::Library(ApiLibraryAlbum {
            album_id: self.id,
            title: self.title.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_cover: self.artwork.is_some(),
            date_released: self.date_released.clone(),
            date_added: self.date_added.clone(),
            source: self.source.clone(),
            blur: self.blur,
            versions: self.versions.iter().map(|v| v.to_api()).collect(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default)]
pub enum AlbumSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
}

impl FromStr for AlbumSource {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSource, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(AlbumSource::Local),
            "tidal" => Ok(AlbumSource::Tidal),
            "qobuz" => Ok(AlbumSource::Qobuz),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum AlbumSort {
    ArtistAsc,
    ArtistDesc,
    NameAsc,
    NameDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
    DateAddedAsc,
    DateAddedDesc,
}

impl FromStr for AlbumSort {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(AlbumSort::ArtistAsc),
            "artist-desc" => Ok(AlbumSort::ArtistDesc),
            "name-asc" | "name" => Ok(AlbumSort::NameAsc),
            "name-desc" => Ok(AlbumSort::NameDesc),
            "release-date-asc" | "release-date" => Ok(AlbumSort::ReleaseDateAsc),
            "release-date-desc" => Ok(AlbumSort::ReleaseDateDesc),
            "date-added-asc" | "date-added" => Ok(AlbumSort::DateAddedAsc),
            "date-added-desc" => Ok(AlbumSort::DateAddedDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionActivePlayers {
    pub session_id: i32,
    pub players: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSession {
    pub name: String,
    pub active_players: Vec<i32>,
    pub playlist: CreateSessionPlaylist,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPlaylist {
    pub tracks: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSession {
    pub session_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<UpdateSessionPlaylist>,
}

impl ToApi<ApiUpdateSession> for UpdateSession {
    fn to_api(&self) -> ApiUpdateSession {
        ApiUpdateSession {
            session_id: self.session_id,
            play: self.play,
            stop: self.stop,
            name: self.name.clone(),
            active: self.active,
            playing: self.playing,
            position: self.position,
            seek: self.seek,
            volume: self.volume,
            playlist: self.playlist.as_ref().map(|p| p.to_api()),
        }
    }
}

#[derive(Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiSource {
    Library,
    Tidal,
    Qobuz,
}

impl MissingValue<ApiSource> for &Row<'_> {}
impl MissingValue<ApiSource> for Value {}
impl ToValueType<ApiSource> for Value {
    fn to_value_type(self) -> Result<ApiSource, ParseError> {
        match self {
            Value::Text(str) => ApiSource::from_str(&str)
                .map_err(|_| ParseError::ConvertType(format!("ApiSource: {str}"))),
            _ => Err(ParseError::ConvertType(format!("ApiSource: {self:?}"))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<UpdateSessionPlaylistTrack>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylistTrack {
    pub id: u64,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl From<UpdateSessionPlaylistTrack> for SessionPlaylistTrack {
    fn from(value: UpdateSessionPlaylistTrack) -> Self {
        SessionPlaylistTrack {
            id: value.id,
            r#type: value.r#type,
            data: value.data,
        }
    }
}

impl ToApi<ApiTrack> for SessionPlaylistTrack {
    fn to_api(&self) -> ApiTrack {
        match self.r#type {
            ApiSource::Library => ApiTrack::Library(ApiLibraryTrack {
                track_id: self.id as i32,
                ..Default::default()
            }),
            ApiSource::Tidal => match &self.data {
                Some(data) => ApiTrack::Tidal(
                    serde_json::from_str(data)
                        .expect("Failed to parse UpdateSessionPlaylistTrack data"),
                ),
                None => ApiTrack::Tidal(serde_json::json!({
                    "id": self.id,
                    "type": self.r#type,
                })),
            },
            ApiSource::Qobuz => match &self.data {
                Some(data) => ApiTrack::Qobuz(
                    serde_json::from_str(data)
                        .expect("Failed to parse UpdateSessionPlaylistTrack data"),
                ),
                None => ApiTrack::Qobuz(serde_json::json!({
                    "id": self.id,
                    "type": self.r#type,
                })),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylistTrack {
    pub id: u64,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl ToApi<ApiUpdateSessionPlaylistTrack> for UpdateSessionPlaylistTrack {
    fn to_api(&self) -> ApiUpdateSessionPlaylistTrack {
        ApiUpdateSessionPlaylistTrack {
            id: self.id,
            r#type: self.r#type,
            data: self.data.clone(),
        }
    }
}

impl ToApi<ApiUpdateSessionPlaylist> for UpdateSessionPlaylist {
    fn to_api(&self) -> ApiUpdateSessionPlaylist {
        ApiUpdateSessionPlaylist {
            session_playlist_id: self.session_playlist_id,
            tracks: self
                .tracks
                .iter()
                .map(|t| From::<UpdateSessionPlaylistTrack>::from(t.clone()))
                .map(|track: SessionPlaylistTrack| track.to_api())
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSession {
    pub session_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<ApiUpdateSessionPlaylist>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<ApiTrack>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSession {
    pub session_id: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i32,
    pub name: String,
    pub active: bool,
    pub playing: bool,
    pub position: Option<i32>,
    pub seek: Option<i32>,
    pub volume: Option<f64>,
    pub active_players: Vec<Player>,
    pub playlist: SessionPlaylist,
}

impl AsModel<Session> for Row<'_> {
    fn as_model(&self) -> Session {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<Session, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<Session, ParseError> {
        Ok(Session {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            active: self.to_value("active")?,
            playing: self.to_value("playing")?,
            position: self.to_value("position")?,
            seek: self.to_value("seek")?,
            volume: self.to_value("volume")?,
            ..Default::default()
        })
    }
}

impl AsModelQuery<Session> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<Session, DbError> {
        let id = self.to_value("id")?;
        match get_session_playlist(db, id)? {
            Some(playlist) => Ok(Session {
                id,
                name: self.to_value("name")?,
                active: self.to_value("active")?,
                playing: self.to_value("playing")?,
                position: self.to_value("position")?,
                seek: self.to_value("seek")?,
                volume: self.to_value("volume")?,
                active_players: get_session_active_players(db, id)?,
                playlist,
            }),
            None => Err(DbError::InvalidRequest),
        }
    }
}

impl AsId for Session {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSession {
    pub session_id: i32,
    pub name: String,
    pub active: bool,
    pub playing: bool,
    pub position: Option<i32>,
    pub seek: Option<i32>,
    pub volume: Option<f64>,
    pub active_players: Vec<ApiPlayer>,
    pub playlist: ApiSessionPlaylist,
}

impl ToApi<ApiSession> for Session {
    fn to_api(&self) -> ApiSession {
        ApiSession {
            session_id: self.id,
            name: self.name.clone(),
            active: self.active,
            playing: self.playing,
            position: self.position,
            seek: self.seek,
            volume: self.volume,
            active_players: self.active_players.iter().map(|p| p.to_api()).collect(),
            playlist: self.playlist.to_api(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylist {
    pub id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl AsModel<SessionPlaylist> for Row<'_> {
    fn as_model(&self) -> SessionPlaylist {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<SessionPlaylist, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<SessionPlaylist, ParseError> {
        Ok(SessionPlaylist {
            id: self.to_value("id")?,
            ..Default::default()
        })
    }
}

impl AsModelResultMappedQuery<ApiTrack, DbError> for Vec<SessionPlaylistTrack> {
    fn as_model_mapped_query(&self, db: &rusqlite::Connection) -> Result<Vec<ApiTrack>, DbError> {
        let tracks = self;
        log::trace!("Mapping tracks to ApiTracks: {tracks:?}");

        let library_track_ids = tracks
            .iter()
            .filter(|t| t.r#type == ApiSource::Library)
            .map(|t| t.id as i32)
            .collect::<Vec<_>>();

        log::trace!("Fetching tracks by ids: {library_track_ids:?}");
        let library_tracks = get_tracks(db, Some(&library_track_ids))?;

        Ok(tracks
            .iter()
            .map(|t| match t.r#type {
                ApiSource::Library => library_tracks
                    .iter()
                    .find(|lib| (lib.id as u64) == t.id)
                    .expect("Missing Library track")
                    .to_api(),
                ApiSource::Tidal => t.to_api(),
                ApiSource::Qobuz => t.to_api(),
            })
            .collect::<Vec<_>>())
    }
}

impl AsModelQuery<SessionPlaylist> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<SessionPlaylist, DbError> {
        let id = self.to_value("id")?;
        let tracks = get_session_playlist_tracks(db, id)?.as_model_mapped_query(db)?;
        log::trace!("Got SessionPlaylistTracks for session_playlist {id}: {tracks:?}");

        Ok(SessionPlaylist { id, tracks })
    }
}

impl AsId for SessionPlaylist {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylistTrack {
    pub id: u64,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl AsModelResult<SessionPlaylistTrack, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<SessionPlaylistTrack, ParseError> {
        Ok(SessionPlaylistTrack {
            id: self.to_value("track_id")?,
            r#type: self.to_value("type")?,
            data: self.to_value("data")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylistTrack {
    pub id: u64,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl ToApi<ApiSessionPlaylistTrack> for SessionPlaylistTrack {
    fn to_api(&self) -> ApiSessionPlaylistTrack {
        ApiSessionPlaylistTrack {
            id: self.id,
            r#type: self.r#type,
            data: self.data.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl ToApi<ApiSessionPlaylist> for SessionPlaylist {
    fn to_api(&self) -> ApiSessionPlaylist {
        ApiSessionPlaylist {
            session_playlist_id: self.id,
            tracks: self.tracks.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnection {
    pub connection_id: String,
    pub name: String,
    pub players: Vec<RegisterPlayer>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
    pub players: Vec<Player>,
}

impl AsModel<Connection> for Row<'_> {
    fn as_model(&self) -> Connection {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<Connection, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<Connection, ParseError> {
        Ok(Connection {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
            ..Default::default()
        })
    }
}

impl AsModelQuery<Connection> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<Connection, DbError> {
        let id = self.to_value::<String>("id")?;
        let players = get_players(db, &id)?;
        Ok(Connection {
            id,
            name: self.to_value("name")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
            players,
        })
    }
}

impl AsId for Connection {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.id.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiConnection {
    pub connection_id: String,
    pub name: String,
    pub alive: bool,
    pub players: Vec<ApiPlayer>,
}

impl ToApi<ApiConnection> for Connection {
    fn to_api(&self) -> ApiConnection {
        ApiConnection {
            connection_id: self.id.clone(),
            name: self.name.clone(),
            alive: false,
            players: self.players.iter().map(|p| p.to_api()).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayer {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerType {
    Symphonia,
    Howler,
    #[default]
    Unknown,
}

impl MissingValue<PlayerType> for &Row<'_> {}
impl MissingValue<PlayerType> for Value {}
impl ToValueType<PlayerType> for Value {
    fn to_value_type(self) -> Result<PlayerType, ParseError> {
        match self {
            Value::Text(str) => Ok(PlayerType::from_str(&str).unwrap_or(PlayerType::Unknown)),
            _ => Err(ParseError::ConvertType("PlayerType".into())),
        }
    }
}

impl FromSql for PlayerType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(PlayerType::from_str(value.as_str()?).unwrap_or(PlayerType::Unknown))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub r#type: PlayerType,
    pub playing: bool,
    pub created: String,
    pub updated: String,
}

impl AsModel<Player> for Row<'_> {
    fn as_model(&self) -> Player {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<Player, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<Player, ParseError> {
        Ok(Player {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            r#type: self.to_value("type")?,
            playing: self.to_value("playing")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for Player {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlayer {
    pub id: i32,
    pub session_id: i32,
    pub player_id: i32,
    pub created: String,
    pub updated: String,
}

impl MissingValue<ActivePlayer> for &Row<'_> {}
impl ToValueType<ActivePlayer> for &Row<'_> {
    fn to_value_type(self) -> Result<ActivePlayer, ParseError> {
        Ok(ActivePlayer {
            id: self.to_value("id")?,
            session_id: self.to_value("session_id")?,
            player_id: self.to_value("player_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsModelResult<ActivePlayer, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<ActivePlayer, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<ActivePlayer> for Row<'_> {
    fn as_model(&self) -> ActivePlayer {
        self.to_value_type().unwrap()
    }
}

impl AsId for ActivePlayer {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayer {
    pub player_id: i32,
    pub name: String,
    pub r#type: PlayerType,
    pub playing: bool,
}

impl ToApi<ApiPlayer> for Player {
    fn to_api(&self) -> ApiPlayer {
        ApiPlayer {
            player_id: self.id,
            name: self.name.clone(),
            r#type: self.r#type.clone(),
            playing: self.playing,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSeek {
    pub session_id: i32,
    pub seek: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientAccessToken {
    pub token: String,
    pub client_id: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<ClientAccessToken> for Row<'_> {
    fn as_model(&self) -> ClientAccessToken {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<ClientAccessToken, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<ClientAccessToken, ParseError> {
        Ok(ClientAccessToken {
            token: self.to_value("token")?,
            client_id: self.to_value("client_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for ClientAccessToken {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.token.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MagicToken {
    pub magic_token: String,
    pub client_id: String,
    pub access_token: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<MagicToken> for Row<'_> {
    fn as_model(&self) -> MagicToken {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<MagicToken, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<MagicToken, ParseError> {
        Ok(MagicToken {
            magic_token: self.to_value("magic_token")?,
            client_id: self.to_value("client_id")?,
            access_token: self.to_value("access_token")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for MagicToken {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.magic_token.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct TrackSize {
    pub id: i32,
    pub track_id: i32,
    pub bytes: u64,
    pub format: String,
}

impl AsModel<TrackSize> for Row<'_> {
    fn as_model(&self) -> TrackSize {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<TrackSize, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsId for TrackSize {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
