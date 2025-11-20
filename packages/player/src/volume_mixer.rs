//! Volume control and mixing utilities.
//!
//! This module provides functions for applying volume adjustments to audio buffers.
//! The volume mixing works with any sample format supported by Symphonia.

use symphonia::core::{
    audio::{AudioBuffer, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

/// Applies a volume multiplier to all channels in an audio buffer.
///
/// # Examples
///
/// ```rust
/// # use symphonia::core::audio::{AudioBuffer, Signal, SignalSpec, Channels, Layout};
/// # use symphonia::core::conv::{IntoSample, FromSample};
/// # use moosicbox_player::volume_mixer::mix_volume;
/// # let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
/// # let mut buffer = AudioBuffer::<f32>::new(1000, spec);
/// // Reduce volume to 50%
/// mix_volume(&mut buffer, 0.5);
/// ```
pub fn mix_volume<S>(input: &mut AudioBuffer<S>, volume: f64)
where
    S: Sample + FromSample<f32> + IntoSample<f32>,
{
    let channels = input.spec().channels.count();
    let frames = input.frames();

    for c in 0..channels {
        let src = input.chan_mut(c);
        for x in src {
            #[allow(clippy::cast_possible_truncation)]
            let s: f32 = (f64::from((*x).into_sample()) * volume) as f32;
            *x = s.into_sample();
        }
    }

    log::trace!(
        "Volume mixer: applied volume {volume:.3} to {frames} frames with {channels} channels"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::{Channels, Layout, SignalSpec};

    #[test_log::test]
    fn test_mix_volume_reduces_amplitude() {
        // Create a test audio buffer with known values
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut buffer = AudioBuffer::<f32>::new(100, spec);

        // Fill buffer with test values (0.5 for all samples)
        for channel in 0..buffer.spec().channels.count() {
            for sample in buffer.chan_mut(channel) {
                *sample = 0.5;
            }
        }

        // Apply 50% volume reduction
        mix_volume(&mut buffer, 0.5);

        // Verify all samples are reduced to 0.25 (0.5 * 0.5)
        for channel in 0..buffer.spec().channels.count() {
            for sample in buffer.chan(channel) {
                assert!(
                    (*sample - 0.25).abs() < 0.001,
                    "Expected ~0.25, got {sample}"
                );
            }
        }
    }

    #[test_log::test]
    fn test_mix_volume_zero_mutes_audio() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
        let mut buffer = AudioBuffer::<f32>::new(50, spec);

        // Fill with non-zero values
        for sample in buffer.chan_mut(0) {
            *sample = 0.8;
        }

        // Apply zero volume (mute)
        mix_volume(&mut buffer, 0.0);

        // Verify all samples are zero
        for sample in buffer.chan(0) {
            assert!(sample.abs() < 0.001, "Expected ~0.0 (muted), got {sample}");
        }
    }

    #[test_log::test]
    fn test_mix_volume_amplification() {
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut buffer = AudioBuffer::<f32>::new(100, spec);

        // Fill with 0.1 values
        for channel in 0..buffer.spec().channels.count() {
            for sample in buffer.chan_mut(channel) {
                *sample = 0.1;
            }
        }

        // Apply 2x amplification
        mix_volume(&mut buffer, 2.0);

        // Verify all samples are doubled to 0.2
        for channel in 0..buffer.spec().channels.count() {
            for sample in buffer.chan(channel) {
                assert!((*sample - 0.2).abs() < 0.001, "Expected ~0.2, got {sample}");
            }
        }
    }

    #[test_log::test]
    fn test_mix_volume_unity_gain_no_change() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
        let mut buffer = AudioBuffer::<f32>::new(50, spec);

        // Fill with test values
        for sample in buffer.chan_mut(0) {
            *sample = 0.7;
        }

        // Apply unity gain (1.0)
        mix_volume(&mut buffer, 1.0);

        // Verify samples unchanged
        for sample in buffer.chan(0) {
            assert!(
                (*sample - 0.7).abs() < 0.001,
                "Expected ~0.7 (unchanged), got {sample}"
            );
        }
    }

    #[test_log::test]
    fn test_mix_volume_multichannel_independence() {
        // Test that volume is applied to all channels equally
        let spec = SignalSpec::new(48000, Layout::FivePointOne.into_channels());
        let mut buffer = AudioBuffer::<f32>::new(100, spec);

        // Fill each channel with different initial values
        let initial_values = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        for (channel_idx, &initial_value) in initial_values.iter().enumerate() {
            if channel_idx < buffer.spec().channels.count() {
                for sample in buffer.chan_mut(channel_idx) {
                    *sample = initial_value;
                }
            }
        }

        // Apply 0.5 volume to all channels
        mix_volume(&mut buffer, 0.5);

        // Verify each channel is reduced by the same factor
        for (channel_idx, &initial_value) in initial_values.iter().enumerate() {
            if channel_idx < buffer.spec().channels.count() {
                let expected = initial_value * 0.5;
                for sample in buffer.chan(channel_idx) {
                    assert!(
                        (*sample - expected).abs() < 0.001,
                        "Channel {channel_idx}: Expected ~{expected}, got {sample}"
                    );
                }
            }
        }
    }

    #[test_log::test]
    fn test_mix_volume_handles_negative_samples() {
        // Audio samples can be negative (e.g., -1.0 to 1.0 range)
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
        let mut buffer = AudioBuffer::<f32>::new(50, spec);

        // Fill with negative values
        for sample in buffer.chan_mut(0) {
            *sample = -0.6;
        }

        // Apply volume reduction
        mix_volume(&mut buffer, 0.5);

        // Verify negative samples are reduced correctly
        for sample in buffer.chan(0) {
            assert!(
                (*sample - (-0.3)).abs() < 0.001,
                "Expected ~-0.3, got {sample}"
            );
        }
    }
}
