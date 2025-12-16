//! Data models for audio output API responses.
//!
//! This module defines the serializable data structures used in HTTP API responses
//! for audio output information.

use serde::{Deserialize, Serialize};

use crate::{AudioOutputFactory, SignalSpec};

/// API representation of an audio output device.
///
/// Contains identifying information and signal specifications for an audio output.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAudioOutput {
    /// Unique identifier for the audio output
    pub id: String,
    /// Human-readable name of the audio output
    pub name: String,
    /// Audio signal specification
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

/// Signal specification describing audio stream characteristics.
///
/// Contains the sample rate and channel configuration for an audio signal.
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
