use std::fmt::Display;

use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(
    Copy, Debug, Clone, Serialize, Deserialize, EnumString, Default, AsRefStr, PartialEq, Eq,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AudioFormat {
    #[cfg(feature = "aac")]
    Aac,
    #[cfg(feature = "flac")]
    Flac,
    #[cfg(feature = "mp3")]
    Mp3,
    #[cfg(feature = "opus")]
    Opus,
    #[default]
    Source,
}

impl Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub fn from_extension_to_audio_format(extension: &str) -> Option<AudioFormat> {
    #[allow(unreachable_code)]
    Some(match extension.to_lowercase().as_str() {
        #[cfg(feature = "flac")]
        "flac" => AudioFormat::Flac,
        #[cfg(feature = "mp3")]
        "mp3" => AudioFormat::Mp3,
        #[cfg(feature = "opus")]
        "opus" => AudioFormat::Opus,
        #[cfg(feature = "aac")]
        "m4a" | "mp4" => AudioFormat::Aac,
        _ => return None,
    })
}

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQuality {
    pub format: AudioFormat,
}
