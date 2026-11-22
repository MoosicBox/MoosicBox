//! Integer pitch search and decoder-matched LTP reconstruction.
//! The reconstruction arithmetic follows pinned libopus `silk/decode_core.c`
//! (Copyright 2006-2011 Skype Limited, BSD-3-Clause; see bundled COPYING).
use crate::silk;

#[derive(Clone, Debug)]
pub(super) struct Pitch {
    pub lag: usize,
    pub periodicity: usize,
    pub filters: Vec<usize>,
}

impl Pitch {
    pub fn coefficients(&self, subframe: usize) -> [i8; 5] {
        books(self.periodicity)[self.filters[subframe]]
    }
}

const fn books(periodicity: usize) -> &'static [[i8; 5]] {
    match periodicity {
        0 => silk::LTP_FILTER_CB_0,
        1 => silk::LTP_FILTER_CB_1,
        _ => silk::LTP_FILTER_CB_2,
    }
}

// Search the LPC residual, not raw PCM: otherwise a formant can be mistaken
// for the fundamental. Compare positive normalized correlations in integers.
pub(super) fn analyze(input: &[i32], previous: &[i16], lpc: &[i16], rate: usize) -> Option<Pitch> {
    let memory = rate / 50;
    let mut pcm = vec![0_i32; memory];
    let count = previous.len().min(memory);
    for (dst, &src) in pcm[memory - count..]
        .iter_mut()
        .zip(&previous[previous.len() - count..])
    {
        *dst = i32::from(src);
    }
    pcm.extend_from_slice(input);
    let mut residual = vec![0_i32; pcm.len()];
    for i in lpc.len()..pcm.len() {
        let prediction = lpc
            .iter()
            .enumerate()
            .map(|(j, &a)| i64::from(a) * i64::from(pcm[i - j - 1]))
            .sum::<i64>()
            >> 12;
        residual[i] = (i64::from(pcm[i]) - prediction).clamp(-65536, 65535) as i32;
    }
    let analysis_start = memory + input.len() / 2;
    let energy = residual[analysis_start..]
        .iter()
        .map(|&x| i128::from(x) * i128::from(x))
        .sum::<i128>();
    if energy < input.len() as i128 {
        return None;
    }
    let mut best = 0_i128;
    let mut lag = 0;
    // The absolute lag syntax has 32 high symbols and rate/2000 low
    // symbols. The 18 ms endpoint would require high symbol 32, outside
    // that alphabet; search only representable lags (RFC 6716 Table 29).
    for candidate in rate / 500..rate * 18 / 1000 {
        let mut cross = 0_i128;
        let mut delayed = 1_i128;
        for i in analysis_start..residual.len() {
            let x = i128::from(residual[i]);
            let y = i128::from(residual[i - candidate]);
            cross += x * y;
            delayed += y * y;
        }
        // A slight short-lag bias avoids octave errors on exactly periodic input.
        let score = cross.max(0) * cross.max(0) / delayed;
        if score > best + best / 100 {
            best = score;
            lag = candidate;
        }
    }
    if best * 100 < energy * 45 {
        return None;
    }
    let subframe = rate / 200;
    let mut best_error = i128::MAX;
    let mut selected = None;
    for periodicity in 0..3 {
        let mut filters = Vec::new();
        let mut total = 0_i128;
        for start in (memory..residual.len()).step_by(subframe) {
            let mut best = (i128::MAX, 0);
            for (index, filter) in books(periodicity).iter().enumerate() {
                let evaluation_start = start.max(analysis_start);
                let error = (evaluation_start..start + subframe)
                    .map(|i| {
                        let prediction = filter
                            .iter()
                            .enumerate()
                            .map(|(j, &a)| i64::from(a) * i64::from(residual[i - lag + 2 - j]))
                            .sum::<i64>()
                            >> 7;
                        let difference = i128::from(residual[i]) - i128::from(prediction);
                        difference * difference
                    })
                    .sum::<i128>();
                if error < best.0 {
                    best = (error, index);
                }
            }
            total += best.0;
            filters.push(best.1);
        }
        if total < best_error {
            best_error = total;
            selected = Some(Pitch {
                lag,
                periodicity,
                filters,
            });
        }
    }
    // Pitch syntax costs bits. Require meaningful residual energy reduction.
    selected.filter(|_| best_error * 100 < energy * 80)
}

pub(super) fn wb(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b as i16)) >> 16) as i32
}

pub(super) fn inverse_q(gain: i32, q: u32) -> i32 {
    let head = gain.leading_zeros() - 1;
    let normalized = gain << head;
    let reciprocal = (i32::MAX >> 2) / (normalized >> 16);
    let error = ((1 << 29) - wb(normalized, reciprocal)).wrapping_shl(3);
    let result = reciprocal
        .wrapping_shl(16)
        .wrapping_add(((i64::from(error) * i64::from(reciprocal)) >> 16) as i32);
    let shift = 61 - head as i32 - q as i32;
    if shift <= 0 {
        (i64::from(result) << -shift).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    } else if shift < 32 {
        result >> shift
    } else {
        0
    }
}

pub(super) fn gain_ratio(a: i32, b: i32) -> i32 {
    let ah = a.leading_zeros() - 1;
    let bh = b.leading_zeros() - 1;
    let mut an = a << ah;
    let bn = b << bh;
    let reciprocal = (i32::MAX >> 2) / (bn >> 16);
    let mut result = wb(an, reciprocal);
    an = an.wrapping_sub((((i64::from(bn) * i64::from(result)) >> 32) as i32).wrapping_shl(3));
    result = result.wrapping_add(wb(an, reciprocal));
    let shift = 29 + ah as i32 - bh as i32 - 16;
    if shift >= 0 {
        result >> shift
    } else {
        (i64::from(result) << -shift).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

pub(super) fn rewhiten(
    previous: &[i16],
    lpc: &[i16],
    gain: i32,
    lag: usize,
    memory: usize,
) -> Vec<i32> {
    let mut pcm = vec![0_i16; memory];
    let count = previous.len().min(memory);
    pcm[memory - count..].copy_from_slice(&previous[previous.len() - count..]);
    let inv = wb(inverse_q(gain, 47), 15565).wrapping_shl(2);
    let mut residual = vec![0; memory];
    for i in memory - lag - 2..memory {
        let mut prediction = 0_i32;
        for (j, &a) in lpc.iter().enumerate() {
            prediction = prediction.wrapping_add(i32::from(pcm[i - j - 1]) * i32::from(a));
        }
        let value = (i32::from(pcm[i]) << 12).wrapping_sub(prediction);
        residual[i] = wb(inv, (((value >> 11) + 1) >> 1).clamp(-32768, 32767));
    }
    residual
}
