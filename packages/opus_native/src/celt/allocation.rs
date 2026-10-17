//! Entropy-aware allocation from fixed-point `rate.c`. Values are eighth-bits.
#![allow(clippy::cast_sign_loss)] // Bounded nonnegative codec allocations and mode indices.
use super::{Allocation, CELT_NUM_BANDS, CeltDecoder, Error, RangeDecoder, Result};
use crate::celt::constants::{ALLOCATION_TABLE, CELT_BINS_2_5MS, LOG2_FRAC_TABLE};

impl CeltDecoder {
    #[allow(clippy::too_many_lines)] // Ordered allocation/entropy state machine mirrors rate.c.
    pub(super) fn allocation_decode(
        &self,
        rd: &mut RangeDecoder,
        mut total: i32,
        offsets: &[i32; 21],
        trim: u8,
        transient: bool,
    ) -> Result<(Allocation, usize, bool)> {
        let start = self.start_band;
        let end = self.end_band;
        if start >= end || end > CELT_NUM_BANDS {
            return Err(Error::CeltDecoder("invalid allocation bands".into()));
        }
        let c = if self.channels == crate::Channels::Stereo {
            2
        } else {
            1
        };
        let stereo = c - 1;
        let lm = self.frame_duration_index() as i32;
        let anti = if transient && lm >= 2 && total >= (lm + 2) * 8 {
            8
        } else {
            0
        };
        total = (total - anti).max(0);
        let skip = if total >= 8 { 8 } else { 0 };
        total -= skip;
        let mut intensity_rsv = 0;
        let mut dual_rsv = 0;
        if c == 2 {
            intensity_rsv = i32::from(LOG2_FRAC_TABLE[end - start]);
            if intensity_rsv > total {
                intensity_rsv = 0;
            } else {
                total -= intensity_rsv;
                dual_rsv = if total >= 8 { 8 } else { 0 };
                total -= dual_rsv;
            }
        }
        let widths = CELT_BINS_2_5MS.map(i32::from);
        let caps = self.compute_caps(lm as u8, c as usize);
        let mut edges = [0_i32; 22];
        for i in 0..21 {
            edges[i + 1] = edges[i] + widths[i];
        }
        let mut thresh = [0; 21];
        let mut tilt = [0; 21];
        for j in start..end {
            thresh[j] = (c * 8).max(((3 * widths[j]) << lm << 3) >> 4);
            tilt[j] = (c
                * widths[j]
                * (i32::from(trim) - 5 - lm)
                * (end - j - 1) as i32
                * (1 << (lm + 3)))
                >> 6;
            if widths[j] << lm == 1 {
                tilt[j] -= c * 8;
            }
        }
        let curve = |j: usize, quality: usize| {
            let bits = (c * widths[j] * i32::from(ALLOCATION_TABLE[j][quality])) << lm >> 2;
            if bits > 0 {
                (bits + tilt[j]).max(0)
            } else {
                bits
            }
        };
        let mut low = 1_i32;
        let mut high = 10_i32;
        while low <= high {
            let mid = low.midpoint(high);
            let mut sum = 0;
            let mut done = false;
            for j in (start..end).rev() {
                let bits = curve(j, mid as usize) + offsets[j];
                if bits >= thresh[j] || done {
                    done = true;
                    sum += bits.min(caps[j]);
                } else if bits >= c * 8 {
                    sum += c * 8;
                }
            }
            if sum > total {
                high = mid - 1;
            } else {
                low = mid + 1;
            }
        }
        let high_quality = low as usize;
        let low_quality = (low - 1) as usize;
        let mut a = [0; 21];
        let mut delta = [0; 21];
        let mut skip_start = start;
        for j in start..end {
            a[j] = curve(j, low_quality);
            let mut upper = if high_quality >= 11 {
                (caps[j] + tilt[j]).max(0)
            } else {
                curve(j, high_quality)
            };
            if low_quality > 0 {
                a[j] += offsets[j];
            }
            upper += offsets[j];
            if offsets[j] > 0 {
                skip_start = j;
            }
            delta[j] = (upper - a[j]).max(0);
        }
        let mut low = 0_i32;
        let mut high = 64;
        for _ in 0..6 {
            let mid = low.midpoint(high);
            let mut sum = 0;
            let mut done = false;
            for j in (start..end).rev() {
                let bits = a[j] + ((mid * delta[j]) >> 6);
                if bits >= thresh[j] || done {
                    done = true;
                    sum += bits.min(caps[j]);
                } else if bits >= c * 8 {
                    sum += c * 8;
                }
            }
            if sum > total {
                high = mid;
            } else {
                low = mid;
            }
        }
        let mut bits = [0; 21];
        let mut sum = 0;
        let mut done = false;
        for j in (start..end).rev() {
            let mut value = a[j] + ((low * delta[j]) >> 6);
            if value < thresh[j] && !done {
                value = if value >= c * 8 { c * 8 } else { 0 };
            } else {
                done = true;
            }
            bits[j] = value.min(caps[j]);
            sum += bits[j];
        }
        let mut coded = end;
        loop {
            let j = coded - 1;
            if j <= skip_start {
                total += skip;
                break;
            }
            let left = total - sum;
            let per = left / (edges[coded] - edges[start]);
            let remainder = left - (edges[coded] - edges[start]) * per;
            let rem = (remainder - (edges[j] - edges[start])).max(0);
            let mut band_bits = bits[j] + per * (edges[coded] - edges[j]) + rem;
            if band_bits >= thresh[j].max(c * 8 + 8) {
                if rd.ec_dec_bit_logp(1)? {
                    break;
                }
                sum += 8;
                band_bits -= 8;
            }
            sum -= bits[j] + intensity_rsv;
            if intensity_rsv > 0 {
                intensity_rsv = i32::from(LOG2_FRAC_TABLE[j - start]);
            }
            sum += intensity_rsv;
            if band_bits >= c * 8 {
                sum += c * 8;
                bits[j] = c * 8;
            } else {
                bits[j] = 0;
            }
            coded -= 1;
        }
        let intensity = if intensity_rsv > 0 {
            start + rd.ec_dec_uint((coded + 1 - start) as u32)? as usize
        } else {
            0
        };
        if intensity <= start {
            total += dual_rsv;
            dual_rsv = 0;
        }
        let dual = dual_rsv > 0 && rd.ec_dec_bit_logp(1)?;
        let mut left = total - sum;
        let per = left / (edges[coded] - edges[start]);
        left -= (edges[coded] - edges[start]) * per;
        for j in start..coded {
            bits[j] += per * widths[j];
        }
        for j in start..coded {
            let extra = left.min(widths[j]);
            bits[j] += extra;
            left -= extra;
        }
        let logn = [
            0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 16, 16, 16, 21, 21, 24, 29, 34, 36,
        ];
        let mut fine = [0_u8; 21];
        let mut priority = [0_u8; 21];
        let mut balance = 0;
        for j in start..coded {
            let n = widths[j] << lm;
            let bit = bits[j] + balance;
            let mut excess;
            if n > 1 {
                excess = (bit - caps[j]).max(0);
                bits[j] = bit - excess;
                let den = c * n + i32::from(c == 2 && n > 2 && !dual && j < intensity);
                let nlogn = den * (logn[j] + lm * 8);
                let mut offset = (nlogn >> 1) - den * 21;
                if n == 2 {
                    offset += den * 2;
                }
                if bits[j] + offset < den * 16 {
                    offset += nlogn >> 2;
                } else if bits[j] + offset < den * 24 {
                    offset += nlogn >> 3;
                }
                let mut ebits = ((bits[j] + offset + den * 4).max(0) / den) >> 3;
                if c * ebits > bits[j] >> 3 {
                    ebits = bits[j] >> stereo >> 3;
                }
                ebits = ebits.min(8);
                priority[j] = u8::from(ebits * (den * 8) >= bits[j] + offset);
                fine[j] = ebits as u8;
                bits[j] -= c * ebits * 8;
            } else {
                excess = (bit - c * 8).max(0);
                bits[j] = bit - excess;
                priority[j] = 1;
            }
            if excess > 0 {
                let extra = (excess >> (stereo + 3)).min(8 - i32::from(fine[j]));
                fine[j] += extra as u8;
                let extra_bits = extra * c * 8;
                priority[j] = u8::from(extra_bits >= excess - balance);
                excess -= extra_bits;
            }
            balance = excess;
        }
        for j in coded..end {
            fine[j] = (bits[j] >> stereo >> 3) as u8;
            bits[j] = 0;
            priority[j] = u8::from(fine[j] < 1);
        }
        Ok((
            Allocation {
                shape_bits: bits,
                fine_energy_bits: fine,
                fine_priority: priority,
                coded_bands: coded,
                balance,
                anti_collapse_rsv: anti,
                skip_rsv: skip,
                intensity_rsv,
                dual_stereo_rsv: dual_rsv,
            },
            intensity,
            dual,
        ))
    }
}
