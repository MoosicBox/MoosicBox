//! Integer SILK reference-compatible output conversion, including same-rate delay.
// Port of libopus silk/resampler*.c at 34bba701ae97c913de719b1f7c10686f62cddb15.
// Copyright (c) 2010-2011, Skype Limited. BSD-3-Clause; see bundled COPYING.
// Casts implement bounded reference fixed-point narrowing, including low-word semantics.
#![allow(clippy::cast_possible_truncation)]
use crate::{Error, Result};
#[path = "resampler_tables.rs"]
mod tables;
use tables::{DOWN_1_2, DOWN_2_3, DOWN_3_4, FRAC};

/// One persistent mono-channel SILK decoder output resampler.
#[derive(Clone, Debug)]
pub struct SilkResampler {
    input_khz: usize,
    output_khz: usize,
    delay: usize,
    delay_buffer: Vec<i16>,
    iir: [i32; 6],
    fir: Vec<i32>,
    increment: usize,
}
fn wb(a: i32, b: i16) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 16) as i32
}
const fn rounded(a: i32, shift: u32) -> i32 {
    ((a >> (shift - 1)).wrapping_add(1)) >> 1
}
fn sat(a: i32) -> i16 {
    a.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}
impl SilkResampler {
    /// Initializes a decoder-side converter. Use separate instances for each channel.
    ///
    /// # Errors
    /// * Rejects input outside 8/12/16 kHz or output outside 8/12/16/24/48 kHz.
    pub fn new(input_rate: u32, output_rate: u32) -> Result<Self> {
        let row = [8000, 12000, 16000]
            .iter()
            .position(|r| *r == input_rate)
            .ok_or_else(|| Error::SilkDecoder("Invalid resampler input rate".into()))?;
        let col = [8000, 12000, 16000, 24000, 48000]
            .iter()
            .position(|r| *r == output_rate)
            .ok_or_else(|| Error::SilkDecoder("Invalid resampler output rate".into()))?;
        let input_khz = input_rate as usize / 1000;
        let output_khz = output_rate as usize / 1000;
        let up = output_rate > input_rate && output_rate != 2 * input_rate;
        let numerator = u64::from(input_rate) << (16 + u32::from(up));
        let increment = numerator.div_ceil(u64::from(output_rate)) as usize;
        Ok(Self {
            input_khz,
            output_khz,
            delay: [[4, 0, 2, 0, 0], [0, 9, 4, 7, 4], [0, 3, 12, 7, 7]][row][col],
            delay_buffer: vec![0; input_khz],
            iir: [0; 6],
            fir: vec![
                0;
                if output_rate < input_rate {
                    if output_rate * 2 == input_rate {
                        24
                    } else {
                        18
                    }
                } else {
                    8
                }
            ],
            increment,
        })
    }
    /// Converts a whole number of milliseconds, preserving filter and delay state.
    /// Same-rate calls still apply the reference decoder delay. No trimming or flush.
    ///
    /// # Errors
    /// * Rejects empty, fractional-millisecond, or over-120-ms input blocks.
    pub fn process(&mut self, input: &[i16]) -> Result<Vec<i16>> {
        if input.is_empty()
            || !input.len().is_multiple_of(self.input_khz)
            || input.len() > self.input_khz * 120
        {
            return Err(Error::SilkDecoder(
                "Resampler needs 1..120 whole milliseconds".into(),
            ));
        }
        let n = self.input_khz - self.delay;
        self.delay_buffer[self.delay..].copy_from_slice(&input[..n]);
        let first = self.delay_buffer.clone();
        let mut output = self.convert(&first);
        output.extend(self.convert(&input[n..input.len() - self.delay]));
        self.delay_buffer[..self.delay].copy_from_slice(&input[input.len() - self.delay..]);
        debug_assert_eq!(output.len(), input.len() / self.input_khz * self.output_khz);
        Ok(output)
    }
    fn up2(&mut self, input: &[i16]) -> Vec<i16> {
        let mut out = Vec::with_capacity(input.len() * 2);
        for sample in input {
            for (offset, coefficients) in [(0, [1746, 14986, -26453]), (3, [6854, 25769, -9994])] {
                let mut value = i32::from(*sample) << 10;
                for (j, coefficient) in coefficients.into_iter().enumerate() {
                    let y = value.wrapping_sub(self.iir[offset + j]);
                    let x = if j == 2 {
                        y.wrapping_add(wb(y, coefficient))
                    } else {
                        wb(y, coefficient)
                    };
                    let next = self.iir[offset + j].wrapping_add(x);
                    self.iir[offset + j] = value.wrapping_add(x);
                    value = next;
                }
                out.push(sat(rounded(value, 10)));
            }
        }
        out
    }
    fn convert(&mut self, input: &[i16]) -> Vec<i16> {
        if self.input_khz == self.output_khz {
            return input.to_vec();
        }
        if self.output_khz == self.input_khz * 2 {
            return self.up2(input);
        }
        let mut output = Vec::new();
        for block in input.chunks(self.input_khz * 10) {
            if self.output_khz > self.input_khz {
                let mut buffer: Vec<i16> = self.fir.iter().map(|v| *v as i16).collect();
                buffer.extend(self.up2(block));
                for index in (0..block.len() * 2 * 65536).step_by(self.increment) {
                    let frac = ((index & 65535) * 12) >> 16;
                    let ptr = index >> 16;
                    let mut sum = 0_i32;
                    for j in 0..4 {
                        sum =
                            sum.wrapping_add(i32::from(buffer[ptr + j]) * i32::from(FRAC[frac][j]));
                        sum = sum.wrapping_add(
                            i32::from(buffer[ptr + 7 - j]) * i32::from(FRAC[11 - frac][j]),
                        );
                    }
                    output.push(sat(rounded(sum, 15)));
                }
                self.fir = buffer[block.len() * 2..]
                    .iter()
                    .map(|v| i32::from(*v))
                    .collect();
            } else {
                let (coefs, fractions): (&[i16], usize) = if self.output_khz * 2 == self.input_khz {
                    (&DOWN_1_2, 1)
                } else if self.output_khz * 3 == self.input_khz * 2 {
                    (&DOWN_2_3, 2)
                } else {
                    (&DOWN_3_4, 3)
                };
                let order = self.fir.len();
                let mut buffer = self.fir.clone();
                for sample in block {
                    let value = self.iir[0].wrapping_add(i32::from(*sample) << 8);
                    buffer.push(value);
                    let q10 = value.wrapping_shl(2);
                    self.iir[0] = self.iir[1].wrapping_add(wb(q10, coefs[0]));
                    self.iir[1] = wb(q10, coefs[1]);
                }
                for index in (0..block.len() * 65536).step_by(self.increment) {
                    let ptr = index >> 16;
                    let frac = ((index & 65535) * fractions) >> 16;
                    let mut sum = 0_i32;
                    for j in 0..order / 2 {
                        if order == 18 {
                            sum = sum.wrapping_add(wb(buffer[ptr + j], coefs[2 + frac * 9 + j]));
                            sum = sum.wrapping_add(wb(
                                buffer[ptr + order - 1 - j],
                                coefs[2 + (fractions - 1 - frac) * 9 + j],
                            ));
                        } else {
                            sum = sum.wrapping_add(wb(
                                buffer[ptr + j].wrapping_add(buffer[ptr + order - 1 - j]),
                                coefs[2 + j],
                            ));
                        }
                    }
                    output.push(sat(rounded(sum, 6)));
                }
                self.fir.copy_from_slice(&buffer[block.len()..]);
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_rate_mb_matches_raw_api_without_trimming() {
        let read = |bytes: &[u8]| {
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>()
        };
        let lr = read(include_bytes!("reference/mb_mixed_stereo.lr.pcm"));
        let expected = read(include_bytes!("reference/mb_mixed_stereo.api.pcm"));
        let mut channels = [
            SilkResampler::new(12000, 12000).unwrap(),
            SilkResampler::new(12000, 12000).unwrap(),
        ];
        let left = channels[0]
            .process(&lr.iter().step_by(2).copied().collect::<Vec<_>>())
            .unwrap();
        let right = channels[1]
            .process(&lr.iter().skip(1).step_by(2).copied().collect::<Vec<_>>())
            .unwrap();
        let actual: Vec<_> = left
            .into_iter()
            .zip(right)
            .flat_map(<[i16; 2]>::from)
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_blocks_do_not_change_state_and_millisecond_partition_is_exact() {
        assert!(SilkResampler::new(24000, 48000).is_err());
        assert!(SilkResampler::new(16000, 44100).is_err());
        for input in [8000, 12000, 16000] {
            for output in [8000, 12000, 16000, 24000, 48000] {
                let signal: Vec<_> = (0..input / 50)
                    .map(|i| if i % 2 == 0 { i16::MIN } else { i16::MAX })
                    .collect();
                let mut whole = SilkResampler::new(input, output).unwrap();
                let mut split = whole.clone();
                assert!(split.process(&[]).is_err());
                assert!(split.process(&[0]).is_err());
                assert!(split.process(&vec![0; input as usize]).is_err());
                let a = whole.process(&signal).unwrap();
                let b: Vec<_> = signal
                    .chunks(input as usize / 1000)
                    .flat_map(|part| split.process(part).unwrap())
                    .collect();
                assert_eq!(a, b, "{input}->{output} partition");
            }
        }
    }

    #[test]
    fn all_rate_pairs_match_c_persistent_reference() {
        let bytes = include_bytes!("reference/resampler.pcm");
        let mut cursor = 0;
        for input in [8000, 12000, 16000] {
            for output in [8000, 12000, 16000, 24000, 48000] {
                let mut state = SilkResampler::new(input, output).unwrap();
                for packet in 0..3_u32 {
                    let n = input / 50;
                    let signal: Vec<_> = (0..n)
                        .map(|k| {
                            let v = (k + packet * n)
                                .wrapping_mul(1_664_525)
                                .wrapping_add(1_013_904_223);
                            ((v >> 16).cast_signed() - 32768) as i16
                        })
                        .collect();
                    let actual = state.process(&signal).unwrap();
                    let length = output as usize / 50;
                    let expected: Vec<_> = bytes[cursor..cursor + length * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| i16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    cursor += length * 2;
                    assert_eq!(
                        actual.len(),
                        expected.len(),
                        "{input}->{output} packet {packet}"
                    );
                    let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                    assert!(
                        first.is_none(),
                        "{input}->{output} packet {packet}: first={first:?}, actual={:?}, expected={:?}",
                        first.map(|i| actual[i]),
                        first.map(|i| expected[i])
                    );
                }
            }
        }
        assert_eq!(cursor, bytes.len());
    }
}
