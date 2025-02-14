#![allow(clippy::module_name_repetitions)]

use moosicbox_music_models::api::ApiTrack;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PlaybackState {
    pub session_id: u64,
    pub playing: bool,
    pub position: u16,
    pub seek: f64,
    pub volume: f64,
    pub tracks: Vec<ApiTrack>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PlayerState {
    pub playback: Option<PlaybackState>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct State {
    pub player: PlayerState,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl<'a> TryFrom<&'a str> for State {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}
