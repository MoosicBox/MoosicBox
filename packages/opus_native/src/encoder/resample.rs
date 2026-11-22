//! Stateful integer polyphase interpolation for the Opus input sample rates.
//! A 25-tap, Hann-windowed sinc operates before CELT preemphasis. Coefficients
//! and convolution are integer; only PCM-to-Q12 conversion uses floating point.
use crate::celt::fixed_point::celt_cos_norm;

/// Stateful anti-aliased 24/48 kHz input conversion to the SILK 16 kHz rate.
///
/// A 97-tap symmetric Hann-windowed sinc operates on a common 48 kHz grid,
/// before 3:1 decimation. The 24 kHz path inserts zeros at 48 kHz and applies
/// twice the filter gain. Convolution, coefficients, and phase are integer;
/// floating point is used only at the normalized PCM input/output boundaries.
#[cfg(any(feature = "silk", test))]
#[derive(Clone)]
pub(super) struct SilkInput {
    history: Vec<[i32; 97]>,
    coefficients: [i32; 97],
    upsample: usize,
    phase: usize,
}

#[cfg(any(feature = "silk", test))]
impl SilkInput {
    /// Create an input converter. Other rates/channel layouts are unsupported.
    pub(super) fn new(channels: usize, input_rate: u32) -> crate::Result<Self> {
        if !(1..=2).contains(&channels) || !matches!(input_rate, 24_000 | 48_000) {
            return Err(crate::Error::Unsupported(
                "SILK input conversion requires 24/48 kHz mono/stereo".into(),
            ));
        }
        let upsample = if input_rate == 24_000 { 2 } else { 1 };
        let mut coefficients = [0; 97];
        for (tap, coefficient) in coefficients.iter_mut().enumerate() {
            let position = tap as i32 - 48;
            // Cutoff 7 kHz, leaving a transition band below output Nyquist.
            let sinc = if position == 0 {
                32768 * 7 / 24
            } else {
                let angle = position * 7 * 131_072 / 48;
                let sine = i64::from(celt_cos_norm(32768 - angle));
                (sine * 1_048_576 / (3_294_199 * i64::from(position))) as i32
            };
            let window = 16384 + i32::from(celt_cos_norm(position * 65536 / 48)) / 2;
            *coefficient = ((i64::from(sinc) * i64::from(window)) >> 15) as i32;
        }
        let sum: i32 = coefficients.iter().sum();
        let gain = 32768 * upsample as i32;
        for coefficient in &mut coefficients {
            *coefficient = (i64::from(*coefficient) * i64::from(gain) / i64::from(sum)) as i32;
        }
        coefficients[48] += gain - coefficients.iter().sum::<i32>();
        Ok(Self {
            history: vec![[0; 97]; channels],
            coefficients,
            upsample,
            phase: 0,
        })
    }

    /// Exact linear-phase group delay in input samples (24 at 24 kHz, 48 at
    /// 48 kHz). The caller must drain/trim this delay along with codec delay.
    #[must_use]
    pub(super) const fn lookahead(&self) -> usize {
        48 / self.upsample
    }

    /// Convert complete interleaved input frames, preserving phase across calls.
    ///
    /// # Errors
    /// * Rejects incomplete channel frames, non-finite or out-of-range PCM.
    ///   Validation precedes state mutation, so failures can be retried.
    pub(super) fn process(&mut self, interleaved: &[f32]) -> crate::Result<Vec<f32>> {
        let channels = self.history.len();
        if !interleaved.len().is_multiple_of(channels)
            || interleaved
                .iter()
                .any(|&sample| !sample.is_finite() || !(-1.0..=1.0).contains(&sample))
        {
            return Err(crate::Error::Unsupported(
                "SILK input requires complete normalized PCM frames".into(),
            ));
        }
        let mut output = Vec::with_capacity((interleaved.len() * self.upsample).div_ceil(3));
        for frame in interleaved.chunks_exact(channels) {
            for step in 0..self.upsample {
                for (history, &sample) in self.history.iter_mut().zip(frame) {
                    history.copy_within(..96, 1);
                    history[0] = if step == 0 {
                        (f64::from(sample) * 134_217_728.0).round() as i32
                    } else {
                        0
                    };
                }
                if self.phase == 0 {
                    for history in &self.history {
                        let sample = history
                            .iter()
                            .zip(&self.coefficients)
                            .map(|(&value, &coefficient)| i64::from(value) * i64::from(coefficient))
                            .sum::<i64>()
                            >> 15;
                        output.push((sample as f64 / 134_217_728.0).clamp(-1.0, 1.0) as f32);
                    }
                }
                self.phase = (self.phase + 1) % 3;
            }
        }
        Ok(output)
    }
}

#[derive(Clone)]
pub(super) struct Interpolator {
    factor: usize,
    history: Vec<[i32; 25]>,
    coefficients: Vec<[i32; 25]>,
}

impl Interpolator {
    pub(super) fn new(channels: usize, factor: usize) -> Self {
        let coefficients = (0..factor)
            .map(|phase| {
                let mut coefficients = [0; 25];
                for (tap, coefficient) in coefficients.iter_mut().enumerate() {
                    let position = (tap as i32 - 12) * factor as i32 + phase as i32;
                    let sinc = if position == 0 {
                        32768
                    } else {
                        let angle = position * 65536 / factor as i32;
                        let sine = i64::from(celt_cos_norm(32768 - angle));
                        // pi in Q20, sinc in Q15.
                        (sine * factor as i64 * 1_048_576 / (3_294_199 * i64::from(position)))
                            as i32
                    };
                    let window =
                        16384 + i32::from(celt_cos_norm((tap as i32 - 12) * 65536 / 12)) / 2;
                    *coefficient = ((i64::from(sinc) * i64::from(window)) >> 15) as i32;
                }
                let sum: i32 = coefficients.iter().sum();
                for coefficient in &mut coefficients {
                    *coefficient = (i64::from(*coefficient) * 32768 / i64::from(sum)) as i32;
                }
                let remainder = 32768 - coefficients.iter().sum::<i32>();
                coefficients[12] += remainder;
                coefficients
            })
            .collect();
        Self {
            factor,
            history: vec![[0; 25]; channels],
            coefficients,
        }
    }

    pub(super) fn process(&mut self, pcm: &[f32]) -> Vec<i32> {
        if self.factor == 1 {
            return pcm
                .iter()
                .map(|&sample| (f64::from(sample) * 134_217_728.0).round() as i32)
                .collect();
        }
        let channels = self.history.len();
        let mut output = Vec::with_capacity(pcm.len() * self.factor);
        for frame in pcm.chunks_exact(channels) {
            for (channel, &sample) in frame.iter().enumerate() {
                self.history[channel].copy_within(..24, 1);
                self.history[channel][0] = (f64::from(sample) * 134_217_728.0).round() as i32;
            }
            for coefficients in &self.coefficients {
                for history in &self.history {
                    output.push(
                        (history
                            .iter()
                            .zip(coefficients)
                            .map(|(&x, &c)| i64::from(x) * i64::from(c))
                            .sum::<i64>()
                            >> 15) as i32,
                    );
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::SilkInput;

    #[test]
    fn silk_input_counts_and_chunk_partitions_are_exact() {
        for rate in [24_000, 48_000] {
            for channels in [1, 2] {
                for milliseconds in [10, 20] {
                    let frames = rate as usize * milliseconds / 1000;
                    let input: Vec<_> = (0..frames * channels)
                        .map(|i| ((i * 7919 % 10000) as f32 - 5000.0) / 10000.0)
                        .collect();
                    let mut whole = SilkInput::new(channels, rate).unwrap();
                    let expected = whole.process(&input).unwrap();
                    assert_eq!(expected.len(), 16 * milliseconds * channels);
                    let mut split = SilkInput::new(channels, rate).unwrap();
                    let actual: Vec<_> = input
                        .chunks(7 * channels)
                        .flat_map(|chunk| split.process(chunk).unwrap())
                        .collect();
                    assert_eq!(actual, expected);
                    let tail = vec![0.0; 99 * channels];
                    assert_eq!(split.process(&tail).unwrap(), whole.process(&tail).unwrap());
                }
            }
        }
    }

    #[test]
    fn silk_input_passband_stopband_and_impulse_delay() {
        for rate in [24_000, 48_000] {
            for frequency in [1000.0, 4000.0, 10000.0] {
                let input: Vec<_> = (0..rate / 10)
                    .map(|i| {
                        0.25 * (std::f32::consts::TAU * frequency * i as f32 / rate as f32).sin()
                    })
                    .collect();
                let mut filter = SilkInput::new(1, rate).unwrap();
                let output = filter.process(&input).unwrap();
                let output = &output[160..];
                let power =
                    output.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / output.len() as f64;
                let gain = (power / (0.25_f64.powi(2) / 2.0)).sqrt();
                if frequency < 7000.0 {
                    assert!(
                        (0.97..=1.03).contains(&gain),
                        "rate={rate} frequency={frequency} gain={gain}"
                    );
                } else {
                    assert!(gain < 0.01, "rate={rate} stopband gain={gain}");
                }
            }
            let mut filter = SilkInput::new(1, rate).unwrap();
            assert_eq!(filter.lookahead(), rate as usize / 1000);
            let mut impulse = vec![0.0; rate as usize / 100];
            impulse[0] = 0.5;
            let response = filter.process(&impulse).unwrap();
            let peak = response
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap()
                .0;
            assert_eq!(peak, 16, "one millisecond group delay at 16 kHz");
        }
    }

    #[test]
    fn silk_input_invalid_pcm_does_not_advance_history() {
        let mut filter = SilkInput::new(2, 24_000).unwrap();
        let mut clean = filter.clone();
        assert!(filter.process(&[0.1]).is_err());
        assert!(filter.process(&[f32::NAN, 0.0]).is_err());
        assert!(filter.process(&[1.1, 0.0]).is_err());
        let input = vec![0.25; 480];
        assert_eq!(
            filter.process(&input).unwrap(),
            clean.process(&input).unwrap()
        );
        assert!(SilkInput::new(0, 24_000).is_err());
        assert!(SilkInput::new(1, 16_000).is_err());
    }
}
