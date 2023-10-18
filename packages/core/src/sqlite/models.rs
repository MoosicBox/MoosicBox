use serde::{Deserialize, Serialize};

use crate::{
    slim::player::{ApiTrack, Track},
    ToApi,
};

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylist {
    pub id: i32,
    pub tracks: Vec<Track>,
}

impl ToApi<ApiSessionPlaylist> for SessionPlaylist {
    fn to_api(&self) -> ApiSessionPlaylist {
        ApiSessionPlaylist {
            id: self.id,
            tracks: self.tracks.iter().map(|t| t.to_api()).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylist {
    pub id: i32,
    pub tracks: Vec<ApiTrack>,
}
