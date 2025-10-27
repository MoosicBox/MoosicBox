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
