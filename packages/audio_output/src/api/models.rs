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

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::Channels;

    #[test_log::test]
    fn test_api_signal_spec_from_stereo() {
        // Test that stereo signal spec correctly counts 2 channels
        // The From impl uses channels.count() which converts a bitflags Channels to usize
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let api_spec: ApiSignalSpec = spec.into();

        assert_eq!(api_spec.rate, 44100);
        assert_eq!(api_spec.channels, 2);
    }

    #[test_log::test]
    fn test_api_signal_spec_from_mono() {
        // Test that mono signal spec correctly counts 1 channel
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT);
        let api_spec: ApiSignalSpec = spec.into();

        assert_eq!(api_spec.rate, 48000);
        assert_eq!(api_spec.channels, 1);
    }

    #[test_log::test]
    fn test_api_signal_spec_from_surround() {
        // Test 5.1 surround (6 channels) - verifies counting works for complex channel configs
        let channels = Channels::FRONT_LEFT
            | Channels::FRONT_RIGHT
            | Channels::FRONT_CENTRE
            | Channels::LFE1
            | Channels::REAR_LEFT
            | Channels::REAR_RIGHT;
        let spec = SignalSpec::new(96000, channels);
        let api_spec: ApiSignalSpec = spec.into();

        assert_eq!(api_spec.rate, 96000);
        assert_eq!(api_spec.channels, 6);
    }

    #[test_log::test]
    fn test_api_audio_output_from_factory() {
        // Test the From<AudioOutputFactory> conversion preserves all fields
        // and correctly converts the nested SignalSpec to ApiSignalSpec
        use crate::{AudioOutputError, AudioOutputFactory};

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-device-id".to_string(),
            "Test Device Name".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let api_output: ApiAudioOutput = factory.into();

        assert_eq!(api_output.id, "test-device-id");
        assert_eq!(api_output.name, "Test Device Name");
        assert_eq!(api_output.spec.rate, 44100);
        assert_eq!(api_output.spec.channels, 2);
    }
}
