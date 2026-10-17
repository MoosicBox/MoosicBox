//! Fixed-point standard CELT deemphasis and integer output-rate decimation.
//! Matches pinned `celt_decoder.c` deemphasis (non-custom modes).
//! The verified `CMake` reference archive is non-RES24: Q12 rounds directly to i16.
//! RES24 builds instead round Q12→Q8→i16 and require a different corpus contract.
use crate::{Error, Result};

#[derive(Default)]
pub struct CeltOutput {
    memory: [i32; 2],
}

impl CeltOutput {
    /// Deemphasize/decimate interleaved 48-kHz Q12 into non-RES24 i16 PCM.
    /// Overwrites the used output region; advances persistent history at 48 kHz.
    ///
    /// # Errors
    /// * Rejects invalid channels/rate, zero or non-divisible frame size, size
    ///   overflow, signal shape mismatch or insufficient output. No state/output
    ///   changes occur on validation errors.
    pub fn process(
        &mut self,
        signal: &[i32],
        frame_samples: usize,
        channels: usize,
        rate: u32,
        output: &mut [i16],
    ) -> Result<usize> {
        self.process_inner(signal, frame_samples, channels, rate, output, false)
    }

    /// Deemphasize/decimate CELT and saturating-add it to existing SILK PCM.
    ///
    /// Matches non-RES24 `accum=1`: each CELT sample is rounded and saturated
    /// to i16 before addition. Input is interleaved 48-kHz Q12. History advances
    /// at 48 kHz even for samples discarded by decimation. Never add SILK to Q12.
    ///
    /// # Errors
    /// * Rejects channels other than 1/2, unsupported output rates, zero or
    ///   non-divisible frame sizes, size overflow, mismatched signal length and
    ///   undersized output. All validation precedes history/output mutation.
    #[allow(dead_code)] // Core transition integration consumes this additive API.
    pub fn process_accumulate(
        &mut self,
        signal: &[i32],
        frame_samples: usize,
        channels: usize,
        rate: u32,
        output: &mut [i16],
    ) -> Result<usize> {
        self.process_inner(signal, frame_samples, channels, rate, output, true)
    }

    /// Clear deemphasis history when the owning CELT decoder is reset.
    /// Does not change any packet, channel or output-rate configuration.
    #[allow(dead_code)] // Core transition integration consumes this additive API.
    pub const fn reset(&mut self) {
        self.memory = [0; 2];
    }

    #[allow(clippy::too_many_arguments)]
    fn process_inner(
        &mut self,
        signal: &[i32],
        frame_samples: usize,
        channels: usize,
        rate: u32,
        output: &mut [i16],
        accumulate: bool,
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
                    let sample = i16::try_from(((filtered + 2048) >> 12).clamp(-32768, 32767))
                        .expect("clamped PCM");
                    let target = &mut output[index / downsample * channels + channel];
                    *target = if accumulate {
                        target.saturating_add(sample)
                    } else {
                        sample
                    };
                }
            }
        }
        Ok(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::CeltOutput;

    #[test]
    fn accumulation_and_history_match_overwrite_then_saturated_add() {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for channels in [1, 2] {
                let mut overwrite = CeltOutput::default();
                let mut accumulate = CeltOutput::default();
                for frame in 0..3 {
                    let signal: Vec<_> = (0..240 * channels)
                        .map(|i| {
                            if (i + frame) % 7 == 0 {
                                i32::MAX
                            } else {
                                -800_000
                            }
                        })
                        .collect();
                    let count = 240 * rate as usize / 48000 * channels;
                    let mut celtonly = vec![0; count];
                    let silk: Vec<i16> = (0..count)
                        .map(|i| if i % 2 == 0 { 30000 } else { -30000 })
                        .collect();
                    let mut mixed = silk.clone();
                    overwrite
                        .process(&signal, 240, channels, rate, &mut celtonly)
                        .unwrap();
                    accumulate
                        .process_accumulate(&signal, 240, channels, rate, &mut mixed)
                        .unwrap();
                    let expected: Vec<_> = silk
                        .iter()
                        .zip(celtonly)
                        .map(|(&a, b)| a.saturating_add(b))
                        .collect();
                    assert_eq!(mixed, expected);
                    assert_eq!(accumulate.memory, overwrite.memory);
                }
                accumulate.reset();
                assert_eq!(accumulate.memory, [0; 2]);
            }
        }
    }

    #[test]
    fn accumulation_validation_is_atomic() {
        let mut stage = CeltOutput {
            memory: [12345, -6789],
        };
        let mut output = [42; 240];
        for (size, channels, rate) in [
            (0, 1, 48000),
            (240, 3, 48000),
            (240, 1, 44100),
            (239, 1, 8000),
            (usize::MAX, 2, 48000),
            (240, 2, 48000),
        ] {
            assert!(
                stage
                    .process_accumulate(&[0; 240], size, channels, rate, &mut output)
                    .is_err()
            );
            assert_eq!(output, [42; 240]);
            assert_eq!(stage.memory, [12345, -6789]);
        }
        assert!(
            stage
                .process_accumulate(&[0; 240], 240, 1, 48000, &mut output[..1])
                .is_err()
        );
        assert_eq!(stage.memory, [12345, -6789]);
        assert_eq!(output, [42; 240]);
    }
}
