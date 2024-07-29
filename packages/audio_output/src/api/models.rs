use serde::{Deserialize, Serialize};

use crate::{AudioOutputFactory, SignalSpec};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAudioOutput {
    pub id: String,
    pub name: String,
    pub spec: ApiSignalSpec,
}

impl From<AudioOutputFactory> for ApiAudioOutput {
    fn from(value: AudioOutputFactory) -> Self {
        Self {
            id: value.id,
            name: value.name,
            spec: value.spec.into(),
        }
    }
}

/// `SignalSpec` describes the characteristics of a Signal.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiSignalSpec {
    /// The signal sampling rate in hertz (Hz).
    pub rate: u32,

    /// The channel count
    pub channels: usize,
}

impl From<SignalSpec> for ApiSignalSpec {
    fn from(value: SignalSpec) -> Self {
        Self {
            rate: value.rate,
            channels: value.channels.count(),
        }
    }
}
