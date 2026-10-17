//! Fixed-point standard CELT deemphasis and integer output-rate decimation.
//! Matches pinned `celt_decoder.c` deemphasis (non-custom modes, accum = 0).
//! The verified `CMake` reference archive is non-RES24: Q12 rounds directly to i16.
//! RES24 builds instead round Q12→Q8→i16 and require a different corpus contract.
use crate::{Error, Result};

#[derive(Default)]
pub struct CeltOutput {
    memory: [i32; 2],
}

impl CeltOutput {
    pub fn process(
        &mut self,
        signal: &[i32],
        frame_samples: usize,
        channels: usize,
        rate: u32,
        output: &mut [i16],
    ) -> Result<usize> {
        if !matches!(channels, 1 | 2) || ![8000, 12000, 16000, 24000, 48000].contains(&rate) {
            return Err(Error::DecodeFailed(
                "invalid CELT output configuration".into(),
            ));
        }
        let downsample = 48000 / rate as usize;
        let expected = frame_samples
            .checked_mul(channels)
            .ok_or_else(|| Error::DecodeFailed("CELT synthesis size overflow".into()))?;
        if frame_samples == 0
            || !frame_samples.is_multiple_of(downsample)
            || signal.len() != expected
        {
            return Err(Error::DecodeFailed(format!(
                "CELT synthesis shape: expected {frame_samples} * {channels}, got {}",
                signal.len()
            )));
        }
        let samples = frame_samples / downsample;
        if output.len() < samples * channels {
            return Err(Error::DecodeFailed("CELT output buffer too small".into()));
        }
        for (index, frame) in signal.chunks_exact(channels).enumerate() {
            for (channel, &sample) in frame.iter().enumerate() {
                // Use a wide sum so hostile upstream values cannot overflow before
                // reference SIG_SAT. Normal synthesis already fits this range.
                let filtered = (i64::from(sample) + i64::from(self.memory[channel]))
                    .clamp(-536_870_911, 536_870_911);
                self.memory[channel] =
                    i32::try_from((27853 * filtered) >> 15).expect("Q15 bounded by SIG_SAT");
                if index % downsample == 0 {
                    output[index / downsample * channels + channel] =
                        i16::try_from(((filtered + 2048) >> 12).clamp(-32768, 32767))
                            .expect("clamped PCM");
                }
            }
        }
        Ok(samples)
    }
}
