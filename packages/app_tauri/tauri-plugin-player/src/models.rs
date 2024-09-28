use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub album: String,
    pub album_cover: Option<String>,
    pub artist: String,
    pub artist_cover: Option<String>,
    pub duration: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateResponse {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitChannel {
    pub channel: Channel,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitChannelResponse {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_track: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_track: Option<bool>,
}
