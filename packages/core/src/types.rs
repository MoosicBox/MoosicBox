use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(
    Copy, Debug, Clone, Serialize, Deserialize, EnumString, Default, AsRefStr, PartialEq, Eq,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AudioFormat {
    #[cfg(feature = "aac")]
    Aac,
    #[cfg(feature = "mp3")]
    Mp3,
    #[cfg(feature = "opus")]
    Opus,
    #[default]
    Source,
}

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQuality {
    pub format: AudioFormat,
}
