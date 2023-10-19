use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlite::{Connection, Row};

use super::db::{get_session_playlist, get_session_playlist_tracks, DbError};

pub trait AsModel<T> {
    fn as_model(&self) -> T;
}

pub trait AsModelQuery<T> {
    fn as_model_query(&self, db: &Connection) -> Result<T, DbError>;
}

pub trait ToApi<T> {
    fn to_api(&self) -> T;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: i32,
    pub file: Option<String>,
    pub artwork: Option<String>,
    pub blur: bool,
}

impl AsModel<Track> for Row {
    fn as_model(&self) -> Track {
        Track {
            id: self.read::<i64, _>("id") as i32,
            number: self.read::<i64, _>("number") as i32,
            title: self.read::<&str, _>("title").to_string(),
            duration: self.read::<f64, _>("duration"),
            album: self.read::<&str, _>("album").to_string(),
            album_id: self.read::<i64, _>("album_id") as i32,
            date_released: self
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            artist: self.read::<&str, _>("artist").to_string(),
            artist_id: self.read::<i64, _>("artist_id") as i32,
            file: self.read::<Option<&str>, _>("file").map(|f| f.to_string()),
            artwork: self
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            blur: self.read::<i64, _>("blur") == 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiTrack {
    pub track_id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub album: String,
    pub album_id: i32,
    pub contains_artwork: bool,
    pub blur: bool,
}

impl ToApi<ApiTrack> for Track {
    fn to_api(&self) -> ApiTrack {
        ApiTrack {
            track_id: self.id,
            number: self.number,
            title: self.title.clone(),
            duration: self.duration,
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            date_released: self.date_released.clone(),
            album: self.album.clone(),
            album_id: self.album_id,
            contains_artwork: self.artwork.is_some(),
            blur: self.blur,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Artist {
    pub id: i32,
    pub title: String,
    pub cover: Option<String>,
}

impl AsModel<Artist> for Row {
    fn as_model(&self) -> Artist {
        Artist {
            id: self.read::<i64, _>("id") as i32,
            title: self.read::<&str, _>("title").to_string(),
            cover: self.read::<Option<&str>, _>("cover").map(|c| c.to_string()),
        }
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiArtist {
    pub artist_id: i32,
    pub title: String,
    pub contains_cover: bool,
}

impl ToApi<ApiArtist> for Artist {
    fn to_api(&self) -> ApiArtist {
        ApiArtist {
            artist_id: self.id,
            title: self.title.clone(),
            contains_cover: self.cover.is_some(),
        }
    }
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
}

impl AsModel<Album> for Row {
    fn as_model(&self) -> Album {
        Album {
            id: self.read::<i64, _>("id") as i32,
            artist: self.read::<&str, _>("artist").to_string(),
            artist_id: self.read::<i64, _>("artist_id") as i32,
            title: self.read::<&str, _>("title").to_string(),
            date_released: self
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            date_added: self
                .read::<Option<&str>, _>("date_added")
                .map(|date| date.to_string()),
            artwork: self
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            directory: self
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            source: AlbumSource::Local,
            blur: self.read::<i64, _>("blur") == 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbum {
    pub album_id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub contains_artwork: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
}

impl ToApi<ApiAlbum> for Album {
    fn to_api(&self) -> ApiAlbum {
        ApiAlbum {
            album_id: self.id,
            title: self.title.clone(),
            artist: self.artist.clone(),
            artist_id: self.artist_id,
            contains_artwork: self.artwork.is_some(),
            date_released: self.date_released.clone(),
            date_added: self.date_added.clone(),
            source: self.source.clone(),
            blur: self.blur,
        }
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
pub struct CreateSession {
    pub name: String,
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
    pub id: i32,
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
    pub playlist: Option<UpdateSessionPlaylist>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylist {
    pub id: i32,
    pub tracks: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSession {
    pub id: i32,
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
    pub playlist: Option<ApiUpdateSessionPlaylist>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylist {
    pub id: i32,
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
    pub playlist: SessionPlaylist,
}

impl AsModelQuery<Session> for Row {
    fn as_model_query(&self, db: &Connection) -> Result<Session, DbError> {
        let id = self.read::<i64, _>("id") as i32;
        match get_session_playlist(db, id)? {
            Some(playlist) => Ok(Session {
                id,
                active: self.read::<i64, _>("active") == 1,
                playing: self.read::<i64, _>("playing") == 1,
                position: self.read::<Option<i64>, _>("position").map(|x| x as i32),
                seek: self.read::<Option<i64>, _>("seek").map(|x| x as i32),
                name: self.read::<&str, _>("name").to_string(),
                playlist,
            }),
            None => Err(DbError::InvalidRequest),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSession {
    pub id: i32,
    pub name: String,
    pub active: bool,
    pub playing: bool,
    pub position: Option<i32>,
    pub seek: Option<i32>,
    pub playlist: ApiSessionPlaylist,
}

impl ToApi<ApiSession> for Session {
    fn to_api(&self) -> ApiSession {
        ApiSession {
            id: self.id,
            name: self.name.clone(),
            active: self.active,
            playing: self.playing,
            position: self.position,
            seek: self.seek,
            playlist: self.playlist.to_api(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylist {
    pub id: i32,
    pub tracks: Vec<Track>,
}

impl AsModelQuery<SessionPlaylist> for Row {
    fn as_model_query(&self, db: &Connection) -> Result<SessionPlaylist, DbError> {
        let id = self.read::<i64, _>("id") as i32;
        Ok(SessionPlaylist {
            id,
            tracks: get_session_playlist_tracks(db, id)?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylist {
    pub id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl ToApi<ApiSessionPlaylist> for SessionPlaylist {
    fn to_api(&self) -> ApiSessionPlaylist {
        ApiSessionPlaylist {
            id: self.id,
            tracks: self.tracks.iter().map(|t| t.to_api()).collect(),
        }
    }
}
