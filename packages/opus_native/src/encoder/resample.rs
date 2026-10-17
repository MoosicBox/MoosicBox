//! Stateful integer polyphase interpolation for the Opus input sample rates.
//! A 25-tap, Hann-windowed sinc operates before CELT preemphasis. Coefficients
//! and convolution are integer; only PCM-to-Q12 conversion uses floating point.
use crate::celt::fixed_point::celt_cos_norm;

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
