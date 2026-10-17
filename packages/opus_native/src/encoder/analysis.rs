//! Integer CELT low-overlap MDCT analysis with a mixed-radix FFT.
//! Folding and rotations follow libopus `mdct.c`. Signal values retain twelve
//! fractional bits, coefficients use Q15, and FFT accumulation uses i64.
//! Only the normalized PCM input boundary performs floating-point arithmetic.
use crate::celt::fixed_point::celt_cos_norm;

#[derive(Clone)]
pub(super) struct Analysis {
    overlap: Vec<Vec<i32>>,
    preemphasis: Vec<i32>,
}

fn mul(value: i32, coefficient: i16) -> i32 {
    ((i64::from(value) * i64::from(coefficient)) >> 15) as i32
}
fn cosine(numerator: i32, denominator: i32) -> i16 {
    celt_cos_norm(
        ((i64::from(numerator) * 131_072 + i64::from(denominator / 2)) / i64::from(denominator))
            as i32,
    )
}

impl Analysis {
    pub(super) fn new(channels: usize) -> Self {
        Self {
            overlap: vec![vec![0; 120]; channels],
            preemphasis: vec![0; channels],
        }
    }

    pub(super) fn transform(&mut self, pcm: &[i32]) -> Vec<Vec<i32>> {
        let channels = self.overlap.len();
        let n = pcm.len() / channels;
        let quarter = n / 2;
        let mut result = Vec::with_capacity(channels);
        let window: Vec<i16> = (0..120)
            .map(|i| {
                let s = i64::from(celt_cos_norm(32768 - (2 * i + 1) * 32768 / 240));
                celt_cos_norm(32768 - ((s * s + 16384) >> 15) as i32)
            })
            .collect();
        for channel in 0..channels {
            let mut input = self.overlap[channel].clone();
            for frame in pcm.chunks_exact(channels) {
                let value = frame[channel];
                input.push(value - self.preemphasis[channel]);
                self.preemphasis[channel] = mul(value, 27853);
            }
            self.overlap[channel].copy_from_slice(&input[n..n + 120]);
            let mut folded = vec![0; n];
            for i in 0..quarter {
                let p1 = 60 + 2 * i;
                let p2 = n + 59 - 2 * i;
                if i < 30 {
                    folded[2 * i] =
                        mul(input[p1 + n], window[59 - 2 * i]) + mul(input[p2], window[60 + 2 * i]);
                    folded[2 * i + 1] =
                        mul(input[p1], window[60 + 2 * i]) - mul(input[p2 - n], window[59 - 2 * i]);
                } else if i < quarter - 30 {
                    folded[2 * i] = input[p2];
                    folded[2 * i + 1] = input[p1];
                } else {
                    let j = 2 * (i - (quarter - 30));
                    folded[2 * i] =
                        -mul(input[p1 - n], window[j]) + mul(input[p2], window[119 - j]);
                    folded[2 * i + 1] =
                        mul(input[p1], window[119 - j]) + mul(input[p2 + n], window[j]);
                }
            }
            let mut pre = vec![(0, 0); quarter];
            for i in 0..quarter {
                let t0 = cosine(8 * i as i32 + 1, (16 * n) as i32);
                let t1 = cosine(8 * (quarter + i) as i32 + 1, (16 * n) as i32);
                pre[i] = (
                    (mul(folded[2 * i], t0) - mul(folded[2 * i + 1], t1)) / quarter as i32,
                    (mul(folded[2 * i + 1], t0) + mul(folded[2 * i], t1)) / quarter as i32,
                );
            }
            let transformed = fft(&pre);
            let mut output = vec![0; n];
            for (k, &(re, im)) in transformed.iter().enumerate() {
                let t0 = cosine(8 * k as i32 + 1, (16 * n) as i32);
                let t1 = cosine(8 * (quarter + k) as i32 + 1, (16 * n) as i32);
                output[2 * k] = mul(im, t1) - mul(re, t0);
                output[n - 1 - 2 * k] = mul(re, t1) + mul(im, t0);
            }
            result.push(output);
        }
        result
    }
}

// CELT transform lengths have only factors 2, 3 and 5. Decimation in time
// reduces the direct transform to O(N log N), with deterministic Q15 twiddles.
fn fft(input: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let n = input.len();
    if n == 1 {
        return input.to_vec();
    }
    let radix = [2, 3, 5]
        .into_iter()
        .find(|factor| n.is_multiple_of(*factor))
        .unwrap_or(n);
    let stride = n / radix;
    let lanes: Vec<_> = (0..radix)
        .map(|lane| {
            let lane: Vec<_> = (0..stride).map(|i| input[radix * i + lane]).collect();
            fft(&lane)
        })
        .collect();
    (0..n)
        .map(|frequency| {
            let (mut real, mut imaginary) = (0_i64, 0_i64);
            for (lane, values) in lanes.iter().enumerate() {
                let (r, i) = values[frequency % stride];
                let phase = ((frequency * lane) % n) as i32;
                let cos = cosine(phase, n as i32);
                // Compute sine directly in Q17 phase, including N not divisible by 4.
                let sin = celt_cos_norm(32768 - ((i64::from(phase) * 131_072) / n as i64) as i32);
                real += i64::from(mul(r, cos) + mul(i, sin));
                imaginary += i64::from(mul(i, cos) - mul(r, sin));
            }
            (real as i32, imaginary as i32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{cosine, fft, mul};
    use crate::celt::fixed_point::celt_cos_norm;

    #[test]
    fn mixed_radix_matches_independent_direct_dft() {
        for n in [60, 120, 240, 480] {
            let input: Vec<_> = (0..n)
                .map(|i| {
                    (
                        ((i * 7919 + 17) % 10000) as i32 - 5000,
                        ((i * 3571 + 13) % 12000) as i32 - 6000,
                    )
                })
                .collect();
            let actual = fft(&input);
            for (k, &(actual_r, actual_i)) in actual.iter().enumerate() {
                let (mut real, mut imaginary) = (0_i64, 0_i64);
                for (j, &(r, i)) in input.iter().enumerate() {
                    let phase = ((k * j) % n) as i32;
                    let cos = cosine(phase, n as i32);
                    let sin =
                        celt_cos_norm(32768 - ((i64::from(phase) * 131_072) / n as i64) as i32);
                    real += i64::from(mul(r, cos) + mul(i, sin));
                    imaginary += i64::from(mul(i, cos) - mul(r, sin));
                }
                // Each Q15 butterfly truncates by at most a few signal LSBs;
                // this bound scales with length, not with signal magnitude.
                assert!((i64::from(actual_r) - real).abs() < 2 * n as i64);
                assert!((i64::from(actual_i) - imaginary).abs() < 2 * n as i64);
            }
        }
    }
}
