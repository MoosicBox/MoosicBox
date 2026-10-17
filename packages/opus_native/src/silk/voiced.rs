// Fixed-point voiced history/rewhitening from pinned libopus decode_core.c.
// Copyright (c) 2006-2011, Skype Limited. BSD-3-Clause; bundled COPYING.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
use super::decoder::SubframeParams;
use crate::{Error, Result};
#[derive(Debug, Clone, Default)]
pub(super) struct VoicedHistory {
    pcm: Vec<i16>,
    ltp_q15: Vec<i32>,
    previous_gain: i32,
}
fn wb(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b as i16)) >> 16) as i32
}
fn ww(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 16) as i32
}
pub(super) fn inverse_q(gain: i32, q: u32) -> i32 {
    let head = gain.leading_zeros() - 1;
    let normalized = gain << head;
    let reciprocal = (i32::MAX >> 2) / (normalized >> 16);
    let error = ((1 << 29) - wb(normalized, reciprocal)).wrapping_shl(3);
    let result = reciprocal
        .wrapping_shl(16)
        .wrapping_add(ww(error, reciprocal));
    let shift = 61 - head.cast_signed() - q.cast_signed();
    if shift <= 0 {
        (i64::from(result) << (-shift)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
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
    let shift = 29 + ah.cast_signed() - bh.cast_signed() - 16;
    if shift >= 0 {
        result >> shift
    } else {
        (i64::from(result) << (-shift)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}
impl VoicedHistory {
    pub(super) fn pcm(&self) -> &[i16] {
        &self.pcm
    }

    pub(super) fn reset_pcm(&mut self) {
        self.pcm.fill(0);
    }
    pub(super) fn start_frame(&mut self) {
        self.ltp_q15.clear();
    }
    pub(super) fn append(&mut self, samples: &[i16], memory: usize) {
        self.pcm.extend_from_slice(samples);
        if self.pcm.len() > memory {
            self.pcm.drain(..self.pcm.len() - memory);
        }
    }
    pub(super) fn residual(
        &mut self,
        excitation: &[i32],
        params: &SubframeParams,
        subframe: usize,
        rewhiten: bool,
        memory: usize,
    ) -> Result<Vec<i32>> {
        let lag = usize::try_from(params.pitch_lag)
            .map_err(|_| Error::SilkDecoder("Invalid voiced pitch lag".into()))?;
        if lag < 3 || lag + params.lpc_coeffs_q12.len() + 2 >= memory || params.gain_q16 <= 0 {
            return Err(Error::SilkDecoder(
                "Invalid voiced synthesis parameters".into(),
            ));
        }
        if subframe == 0 || rewhiten {
            let mut pcm = vec![0; memory];
            let count = self.pcm.len().min(memory);
            pcm[memory - count..].copy_from_slice(&self.pcm[self.pcm.len() - count..]);
            let mut inv = inverse_q(params.gain_q16, 47);
            if subframe == 0 {
                inv = wb(inv, i32::from(params.ltp_scale_q14)).wrapping_shl(2);
            }
            self.ltp_q15 = vec![0; memory];
            for index in memory - lag - 2..memory {
                let mut prediction = 0_i32;
                for (j, coef) in params.lpc_coeffs_q12.iter().enumerate() {
                    prediction =
                        prediction.wrapping_add(i32::from(pcm[index - j - 1]) * i32::from(*coef));
                }
                let residual = (i32::from(pcm[index]) << 12).wrapping_sub(prediction);
                let residual =
                    (((residual >> 11) + 1) >> 1).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
                self.ltp_q15[index] = wb(inv, residual);
            }
        } else if params.gain_q16 != self.previous_gain {
            let ratio = gain_ratio(self.previous_gain, params.gain_q16);
            let end = self.ltp_q15.len();
            for value in &mut self.ltp_q15[end - lag - 2..] {
                *value = ww(*value, ratio);
            }
        }
        self.previous_gain = params.gain_q16;
        let mut result = Vec::with_capacity(excitation.len());
        for value in excitation {
            let mut prediction = 2_i32;
            let end = self.ltp_q15.len();
            for j in 0..5 {
                prediction = prediction.wrapping_add(wb(
                    self.ltp_q15[end - lag + 2 - j],
                    i32::from(params.ltp_filter_q7[j]) << 7,
                ));
            }
            let residual = value.wrapping_add(prediction.wrapping_shl(1));
            result.push(residual);
            self.ltp_q15.push(residual.wrapping_shl(1));
        }
        Ok(result)
    }
}
