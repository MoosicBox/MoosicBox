use std::str::FromStr;

use rusqlite::{types::FromSql, Row};
use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

use super::db::{
    get_players, get_session_active_players, get_session_playlist, get_session_playlist_tracks,
    DbError, SqliteValue,
};

pub trait AsModel<T> {
    fn as_model(&self) -> T;
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
        NumberId {
            id: self.get("id").unwrap(),
        }
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
        StringId {
            id: self.get("id").unwrap(),
        }
    }
}

impl AsId for StringId {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.id.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub bytes: u64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: i32,
    pub file: Option<String>,
    pub artwork: Option<String>,
    pub blur: bool,
}

impl AsModel<Track> for Row<'_> {
    fn as_model(&self) -> Track {
        Track {
            id: self.get("id").unwrap(),
            number: self.get("number").unwrap(),
            title: self.get("title").unwrap(),
            duration: self.get("duration").unwrap(),
            bytes: self.get("bytes").unwrap(),
            album: self.get("album").unwrap_or_default(),
            album_id: self.get("album_id").unwrap(),
            date_released: self.get("date_released").unwrap_or_default(),
            artist: self.get("artist").unwrap_or_default(),
            artist_id: self.get("artist_id").unwrap_or_default(),
            file: self.get("file").unwrap(),
            artwork: self.get("artwork").unwrap_or_default(),
            blur: self.get::<_, u16>("blur").unwrap_or_default() == 1,
        }
    }
}

impl AsId for Track {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
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

impl AsModel<Artist> for Row<'_> {
    fn as_model(&self) -> Artist {
        Artist {
            id: self.get("id").unwrap(),
            title: self.get("title").unwrap(),
            cover: self.get("cover").unwrap(),
        }
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

impl AsModel<Album> for Row<'_> {
    fn as_model(&self) -> Album {
        Album {
            id: self.get("id").unwrap(),
            artist: self.get("artist").unwrap_or_default(),
            artist_id: self.get("artist_id").unwrap(),
            title: self.get("title").unwrap(),
            date_released: self.get("date_released").unwrap(),
            date_added: self.get("date_added").unwrap(),
            artwork: self.get("artwork").unwrap(),
            directory: self.get("directory").unwrap(),
            source: AlbumSource::Local,
            blur: self.get::<_, u16>("blur").unwrap() == 1,
        }
    }
}

impl AsId for Album {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
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
    pub session_playlist_id: i32,
    pub tracks: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSession {
    pub session_id: i32,
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
    pub active_players: Vec<Player>,
    pub playlist: SessionPlaylist,
}

impl AsModel<Session> for Row<'_> {
    fn as_model(&self) -> Session {
        Session {
            id: self.get("id").unwrap(),
            name: self.get("name").unwrap(),
            active: self.get::<_, u16>("active").unwrap() == 1,
            playing: self.get::<_, u16>("playing").unwrap() == 1,
            position: self.get("position").unwrap(),
            seek: self.get("seek").unwrap(),
            ..Default::default()
        }
    }
}

impl AsModelQuery<Session> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<Session, DbError> {
        let id = self.get("id").unwrap();
        match get_session_playlist(db, id)? {
            Some(playlist) => Ok(Session {
                id,
                name: self.get("name").unwrap(),
                active: self.get::<_, u16>("active").unwrap() == 1,
                playing: self.get::<_, u16>("playing").unwrap() == 1,
                position: self.get("position").unwrap(),
                seek: self.get("seek").unwrap(),
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
            active_players: self.active_players.iter().map(|p| p.to_api()).collect(),
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

impl AsModel<SessionPlaylist> for Row<'_> {
    fn as_model(&self) -> SessionPlaylist {
        SessionPlaylist {
            id: self.get("id").unwrap(),
            ..Default::default()
        }
    }
}

impl AsModelQuery<SessionPlaylist> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<SessionPlaylist, DbError> {
        let id = self.get("id").unwrap();
        Ok(SessionPlaylist {
            id,
            tracks: get_session_playlist_tracks(db, id)?,
        })
    }
}

impl AsId for SessionPlaylist {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl ToApi<ApiSessionPlaylist> for SessionPlaylist {
    fn to_api(&self) -> ApiSessionPlaylist {
        ApiSessionPlaylist {
            session_playlist_id: self.id,
            tracks: self.tracks.iter().map(|t| t.to_api()).collect(),
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
        Connection {
            id: self.get::<_, String>("id").unwrap(),
            name: self.get("name").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
            ..Default::default()
        }
    }
}

impl AsModelQuery<Connection> for Row<'_> {
    fn as_model_query(&self, db: &rusqlite::Connection) -> Result<Connection, DbError> {
        let id = self.get::<_, String>("id").unwrap();
        let players = get_players(db, &id)?;
        Ok(Connection {
            id,
            name: self.get("name").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
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
    pub players: Vec<ApiPlayer>,
}

impl ToApi<ApiConnection> for Connection {
    fn to_api(&self) -> ApiConnection {
        ApiConnection {
            connection_id: self.id.clone(),
            name: self.name.clone(),
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
        Player {
            id: self.get("id").unwrap(),
            name: self.get("name").unwrap(),
            r#type: self.get("type").unwrap(),
            playing: self.get::<_, u16>("playing").unwrap() == 1,
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
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

impl AsModel<ActivePlayer> for Row<'_> {
    fn as_model(&self) -> ActivePlayer {
        ActivePlayer {
            id: self.get("id").unwrap(),
            session_id: self.get("session_id").unwrap(),
            player_id: self.get("player_id").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
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
        ClientAccessToken {
            token: self.get("token").unwrap(),
            client_id: self.get("client_id").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
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
        MagicToken {
            magic_token: self.get("magic_token").unwrap(),
            client_id: self.get("client_id").unwrap(),
            access_token: self.get("access_token").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
    }
}

impl AsId for MagicToken {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.magic_token.clone())
    }
}
