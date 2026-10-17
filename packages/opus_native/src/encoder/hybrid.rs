//! Integer hybrid analysis with decoder-output phase compensation.
//! The SILK interpolation filter is dispersive near the band edge. A finite
//! inverse-response analysis filter aligns its output with CELT, rather than
//! treating frequency-dependent phase as a constant sample delay.
use crate::Result;
use crate::celt::fixed_point::celt_cos_norm;
use crate::silk::resampler::SilkResampler;

pub(super) const DELAY: usize = 384;
const FFT: usize = 1024;
const TAPS: usize = 769;

#[derive(Clone)]
pub(super) struct LowBand {
    history: Vec<Vec<i32>>,
    coefficients: Vec<i32>,
    celt_delay: Vec<Vec<f32>>,
}

fn cosine(phase: i32) -> i64 {
    i64::from(celt_cos_norm(phase))
}

impl LowBand {
    pub(super) fn new(channels: usize) -> Result<Self> {
        // Characterize the existing integer reference output filter, not a
        // platform codec. The leading input zero includes SILK's mono/MS delay.
        let mut resampler = SilkResampler::new(16000, 48000)?;
        let mut impulse = vec![0_i16; 352];
        impulse[1] = 16384;
        let response = resampler.process(&impulse)?;
        let mut frequencies = Vec::new();
        for frequency in 0..=FFT / 6 {
            let (mut real, mut imaginary) = (0_i64, 0_i64);
            for (i, &sample) in response.iter().take(FFT).enumerate() {
                let phase = ((frequency * i) % FFT) as i32 * 128;
                real += i64::from(sample) * cosine(phase);
                imaginary -= i64::from(sample) * cosine(32768 - phase);
            }
            real >>= 15;
            imaginary >>= 15;
            let norm =
                i128::from(real) * i128::from(real) + i128::from(imaginary) * i128::from(imaginary);
            let taper = if frequency < 164 {
                32768
            } else {
                (FFT / 6 - frequency) as i64 * 32768 / 7
            };
            let numerator = 3_i128 * 16384 * i128::from(taper);
            let inverse_real = (numerator * i128::from(real) / norm.max(1)) as i64;
            let inverse_imaginary = (-numerator * i128::from(imaginary) / norm.max(1)) as i64;
            frequencies.push((inverse_real, inverse_imaginary));
        }
        let mut coefficients = Vec::with_capacity(TAPS);
        for tap in 0..TAPS {
            let mut value = frequencies[0].0;
            for (frequency, &(real, imaginary)) in frequencies.iter().enumerate().skip(1) {
                let phase =
                    ((frequency as i64 * (tap as i64 - DELAY as i64) * 128) % 131_072) as i32;
                value += 2 * ((real * cosine(phase) - imaginary * cosine(32768 - phase)) >> 15);
            }
            coefficients.push((value / FFT as i64) as i32);
        }
        let correction = 32768 - coefficients.iter().sum::<i32>();
        coefficients[DELAY] += correction;
        Ok(Self {
            history: vec![vec![0; TAPS]; channels],
            coefficients,
            celt_delay: vec![vec![0.0; DELAY - 120]; channels],
        })
    }

    pub(super) fn delay_celt(&mut self, pcm: &[f32]) -> Vec<f32> {
        let channels = self.history.len();
        let mut output = Vec::with_capacity(pcm.len());
        for frame in pcm.chunks_exact(channels) {
            for (channel, &sample) in frame.iter().enumerate() {
                output.push(self.celt_delay[channel][0]);
                self.celt_delay[channel].rotate_left(1);
                self.celt_delay[channel][DELAY - 121] = sample;
            }
        }
        output
    }

    pub(super) fn process(&mut self, pcm: &[f32]) -> Vec<f32> {
        let channels = self.history.len();
        let mut output = Vec::with_capacity(pcm.len() / 3);
        for (i, frame) in pcm.chunks_exact(channels).enumerate() {
            for (channel, &sample) in frame.iter().enumerate() {
                self.history[channel].rotate_right(1);
                self.history[channel][0] = (f64::from(sample) * 134_217_728.0).round() as i32;
            }
            if i % 3 == 0 {
                for history in &self.history {
                    let value = (history
                        .iter()
                        .zip(&self.coefficients)
                        .map(|(&x, &c)| i64::from(x) * i64::from(c))
                        .sum::<i64>()
                        >> 15) as i32;
                    output.push((f64::from(value) / 134_217_728.0) as f32);
                }
            }
        }
        output
    }
}
