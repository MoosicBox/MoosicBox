//! Fixed-point inverse spreading and TF sample reorganization from `vq.c`/`bands.c`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
// Fixed-width wrap/truncation matches the reference macros; lengths are mode-bounded.
use crate::celt::fixed_point::celt_cos_norm;

// Pinned fixed-point mode pulse-cache tables (static_modes_fixed.h).
pub(super) const CACHE_INDEX50: [i16; 105] = [
    -1, -1, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 41, 41, 41, 82, 82, 123, 164, 200, 222, 0, 0, 0, 0,
    0, 0, 0, 0, 41, 41, 41, 41, 123, 123, 123, 164, 164, 240, 266, 283, 295, 41, 41, 41, 41, 41,
    41, 41, 41, 123, 123, 123, 123, 240, 240, 240, 266, 266, 305, 318, 328, 336, 123, 123, 123,
    123, 123, 123, 123, 123, 240, 240, 240, 240, 305, 305, 305, 318, 318, 343, 351, 358, 364, 240,
    240, 240, 240, 240, 240, 240, 240, 305, 305, 305, 305, 343, 343, 343, 351, 351, 370, 376, 382,
    387,
];
pub(super) const CACHE_BITS50: [u8; 392] = [
    40, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 40, 15, 23, 28, 31, 34, 36, 38, 39, 41, 42, 43, 44, 45, 46, 47,
    47, 49, 50, 51, 52, 53, 54, 55, 55, 57, 58, 59, 60, 61, 62, 63, 63, 65, 66, 67, 68, 69, 70, 71,
    71, 40, 20, 33, 41, 48, 53, 57, 61, 64, 66, 69, 71, 73, 75, 76, 78, 80, 82, 85, 87, 89, 91, 92,
    94, 96, 98, 101, 103, 105, 107, 108, 110, 112, 114, 117, 119, 121, 123, 124, 126, 128, 40, 23,
    39, 51, 60, 67, 73, 79, 83, 87, 91, 94, 97, 100, 102, 105, 107, 111, 115, 118, 121, 124, 126,
    129, 131, 135, 139, 142, 145, 148, 150, 153, 155, 159, 163, 166, 169, 172, 174, 177, 179, 35,
    28, 49, 65, 78, 89, 99, 107, 114, 120, 126, 132, 136, 141, 145, 149, 153, 159, 165, 171, 176,
    180, 185, 189, 192, 199, 205, 211, 216, 220, 225, 229, 232, 239, 245, 251, 21, 33, 58, 79, 97,
    112, 125, 137, 148, 157, 166, 174, 182, 189, 195, 201, 207, 217, 227, 235, 243, 251, 17, 35,
    63, 86, 106, 123, 139, 152, 165, 177, 187, 197, 206, 214, 222, 230, 237, 250, 25, 31, 55, 75,
    91, 105, 117, 128, 138, 146, 154, 161, 168, 174, 180, 185, 190, 200, 208, 215, 222, 229, 235,
    240, 245, 255, 16, 36, 65, 89, 110, 128, 144, 159, 173, 185, 196, 207, 217, 226, 234, 242, 250,
    11, 41, 74, 103, 128, 151, 172, 191, 209, 225, 241, 255, 9, 43, 79, 110, 138, 163, 186, 207,
    227, 246, 12, 39, 71, 99, 123, 144, 164, 182, 198, 214, 228, 241, 253, 9, 44, 81, 113, 142,
    168, 192, 214, 235, 255, 7, 49, 90, 127, 160, 191, 220, 247, 6, 51, 95, 134, 170, 203, 234, 7,
    47, 87, 123, 155, 184, 212, 237, 6, 52, 97, 137, 174, 208, 240, 5, 57, 106, 151, 192, 231, 5,
    59, 111, 158, 202, 243, 5, 55, 103, 147, 187, 224, 5, 60, 113, 161, 206, 248, 4, 65, 122, 175,
    224, 4, 67, 127, 182, 234,
];

/// Scalar PVQ normalization from `vq.c` `normalise_residual` (no epsilon added).
#[cfg(test)]
pub(super) fn normalize_pulses_q14(pulses: &[i32]) -> Vec<i16> {
    use crate::celt::fixed_point::vshr32;
    let energy: i32 = pulses.iter().map(|&p| p * p).sum();
    if energy == 0 {
        return vec![0; pulses.len()];
    }
    let k = (31 - energy.leading_zeros()) / 2;
    let t = vshr32(energy, 2 * (k as i32 - 7));
    let gain = ((i64::from(residual_rsqrt_norm(t)) * 2_147_483_647) >> 31) as i32;
    pulses
        .iter()
        .map(|&p| ((gain * p + (1 << k)) >> (k + 1)) as i16)
        .collect()
}

pub(super) fn residual_rsqrt_norm(x: i32) -> i16 {
    let n = (x - 32768) as i16;

    let term2 = mul_q15(n, 6713);
    let term1 = mul_q15(n, (-13490_i16).wrapping_add(term2));
    let r = 23557_i16.wrapping_add(term1);

    let r2 = mul_q15(r, r);
    let y_part = mul_q15(r2, n).wrapping_add(r2).wrapping_sub(16384);
    let y = y_part << 1;

    let inner = mul_q15(y, 12288).wrapping_sub(16384);
    let adjustment = mul_q15(r, mul_q15(y, inner));

    r.wrapping_add(adjustment)
}

fn mul_q15(a: i16, b: i16) -> i16 {
    ((i32::from(a) * i32::from(b)) >> 15) as i16
}

fn reciprocal(x: i32) -> i32 {
    let integer = 31 - x.leading_zeros();
    let normalized = if integer >= 15 {
        x >> (integer - 15)
    } else {
        x << (15 - integer)
    };
    let n = (normalized - 32768) as i16;
    let mut r = 30840_i16.wrapping_add(mul_q15(-15420, n));
    r = r.wrapping_sub(mul_q15(
        r,
        mul_q15(r, n).wrapping_add(r.wrapping_add(-32768)),
    ));
    r = r.wrapping_sub(1_i16.wrapping_add(mul_q15(
        r,
        mul_q15(r, n).wrapping_add(r.wrapping_add(-32768)),
    )));
    if integer > 16 {
        i32::from(r) >> (integer - 16)
    } else {
        i32::from(r) << (16 - integer)
    }
}

#[allow(clippy::many_single_char_names)] // Rotation notation matches reference.
fn rotate_once(x: &mut [i16], stride: usize, c: i16, s: i16) {
    let len = x.len();
    let rotate = |x: &mut [i16], i: usize| {
        let a = i32::from(x[i]);
        let b = i32::from(x[i + stride]);
        x[i + stride] = ((i32::from(c) * b + i32::from(s) * a + 16384) >> 15) as i16;
        x[i] = ((i32::from(c) * a - i32::from(s) * b + 16384) >> 15) as i16;
    };
    for i in 0..len - stride {
        rotate(x, i);
    }
    if len > 2 * stride {
        for i in (0..len - 2 * stride).rev() {
            rotate(x, i);
        }
    }
}

/// Inverse fixed-point spreading, operating on normalized Q14 coefficients.
/// Preconditions: 1 <= blocks <= len, len divisible by blocks, spread <= 3.
pub(super) fn inverse_spread(x: &mut [i16], pulses: usize, blocks: usize, spread: u8) {
    let len = x.len();
    if len == 0 || 2 * pulses >= len || spread == 0 {
        return;
    }
    let factor = [15, 10, 5][usize::from(spread - 1)];
    let numerator = (32767 * len) as i64;
    let denominator = (len + factor * pulses) as i32;
    let gain = ((numerator * i64::from(reciprocal(denominator))) >> 31) as i16;
    let theta = mul_q15(gain, gain) >> 1;
    let c = celt_cos_norm(i32::from(theta));
    let s = celt_cos_norm(i32::from(32767 - theta));
    let mut stride2 = 0;
    if len >= 8 * blocks {
        stride2 = 1;
        while (stride2 * stride2 + stride2) * blocks + (blocks >> 2) < len {
            stride2 += 1;
        }
    }
    for block in x.chunks_exact_mut(len / blocks) {
        if stride2 != 0 {
            rotate_once(block, stride2, s, c);
        }
        rotate_once(block, 1, c, s);
    }
}

pub(super) fn haar(x: &mut [i16], n: usize, stride: usize) {
    for lane in 0..stride {
        for i in 0..n / 2 {
            let a = i32::from(x[lane + stride * 2 * i]) * 23170;
            let b = i32::from(x[lane + stride * (2 * i + 1)]) * 23170;
            x[lane + stride * 2 * i] = ((a + b + 16384) >> 15) as i16;
            x[lane + stride * (2 * i + 1)] = ((a - b + 16384) >> 15) as i16;
        }
    }
}

/// Block layout at the scalar PVQ leaf, after signed TF changes.
#[must_use]
pub(super) fn tf_blocks(n: usize, blocks: usize, resolution: i8) -> (usize, usize, usize) {
    let recombine = resolution.max(0) as usize;
    let mut b = blocks >> recombine;
    let mut width = n / b;
    let mut divide = 0;
    let mut remaining = resolution;
    while width.is_multiple_of(2) && remaining < 0 {
        b <<= 1;
        width >>= 1;
        divide += 1;
        remaining += 1;
    }
    (b, recombine, divide)
}

/// Inverts the sample permutation and Haar steps after residual reconstruction.
pub(super) fn undo_tf(x: &mut [i16], original_blocks: usize, resolution: i8) {
    let n = x.len();
    if n <= 1 {
        return;
    }
    let (blocks, recombine, divide) = tf_blocks(n, original_blocks, resolution);
    let permutation_blocks = blocks << recombine;
    let width = n / permutation_blocks;
    let ordery = [
        1, 0, 3, 0, 2, 1, 7, 0, 4, 3, 6, 1, 5, 2, 15, 0, 8, 7, 12, 3, 11, 4, 14, 1, 9, 6, 13, 2,
        10, 5,
    ];
    if blocks > 1 {
        let src = x.to_vec();
        for lane in 0..permutation_blocks {
            let source_lane = if original_blocks == 1 {
                ordery[permutation_blocks - 2 + lane]
            } else {
                lane
            };
            for i in 0..width {
                x[i * permutation_blocks + lane] = src[source_lane * width + i];
            }
        }
    }
    let mut b = original_blocks >> recombine;
    let mut nb = n / b;
    // N_B0/B0 in quant_band refer to the layout after recombining, before divide.
    b <<= divide;
    nb >>= divide;
    for _ in 0..divide {
        b >>= 1;
        nb <<= 1;
        haar(x, nb, b);
    }
    for k in 0..recombine {
        haar(x, n >> k, 1 << k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_tf_matches_reference_haar_schedule() {
        let cases: &[(usize, usize, i8, &[i16])] = &[
            (
                16,
                4,
                1,
                &[
                    -772, -10813, 7724, 772, -1158, 772, -4247, 772, 10041, 772, 6952, 772, -1930,
                    772, -5019, 772,
                ],
            ),
            (
                32,
                4,
                2,
                &[
                    -820, 274, -7646, -8192, 5189, 8465, 546, 0, 3005, 8465, 546, 0, 821, 8465,
                    547, 0, -1363, 8465, 546, 0, -3547, 8465, 546, 0, -5730, 8465, 546, 0, 276,
                    274, -7646, 8192,
                ],
            ),
            (
                16,
                1,
                -2,
                &[
                    4916, 6007, -8192, 7100, -3822, -2184, 0, -1092, 12016, -2184, 0, -1092, -4914,
                    -2184, 0, -1092,
                ],
            ),
            (
                32,
                4,
                -1,
                &[
                    -3089, 6952, 5408, 3864, -8496, 3089, 3089, 3089, -3475, -5019, -6563, -8108,
                    3089, 3089, 3089, 3089, 7724, 6180, 4636, 3091, 3089, 3089, 3089, 3089, -4247,
                    -5791, -7335, 2705, 3089, 3089, 3089, -8496,
                ],
            ),
            (
                8,
                1,
                0,
                &[-8192, -273, 7646, -819, 7100, -1365, 6554, -1911],
            ),
        ];
        for &(n, blocks, tf, expected) in cases {
            let mut x: Vec<i16> = (0..n).map(|i| (i * 7919 % 16384) as i16 - 8192).collect();
            undo_tf(&mut x, blocks, tf);
            assert_eq!(x, expected);
        }
    }

    #[test]
    fn scalar_residual_matches_actual_alg_unquant() {
        type ScalarCase = (u32, u32, u8, usize, &'static [i16], (u32, u32, u32));
        let cases: &[ScalarCase] = &[
            (
                8,
                1,
                2,
                1,
                &[-15262, -4656, -1477, 2173, 603, 871, -2364, -385],
                (134_217_728, 1_380_759, 5),
            ),
            (
                16,
                2,
                1,
                4,
                &[
                    0, 0, 0, 0, 0, 0, 0, 0, -9209, -13338, -2381, -114, 0, 0, 0, 0,
                ],
                (2_147_483_648, 353_474_446, 10),
            ),
            (
                4,
                3,
                3,
                1,
                &[0, -14653, -7327, 0],
                (24_403_223, 1_380_768, 8),
            ),
        ];
        let bytes: Vec<u8> = "9fd5dcd0e2977eabecef98ff3f560f5b9c1abf4d0e0f83e0b75614b38d422b6f4d8b6df89d13a8bde303412c8c8f42d40aa00d44dc6c7771bc5ba9".as_bytes().as_chunks::<2>().0.iter().map(|pair|u8::from_str_radix(std::str::from_utf8(pair).unwrap(),16).unwrap()).collect();
        for &(n, k, spread, blocks, expected, entropy) in cases {
            let mut rd = crate::range::RangeDecoder::new(&bytes).unwrap();
            let pulses = crate::celt::pvq::decode_pvq_vector(&mut rd, n, k).unwrap();
            let mut x = normalize_pulses_q14(&pulses);
            inverse_spread(&mut x, k as usize, blocks, spread);
            assert_eq!(x, expected, "N={n}, K={k}");
            assert_eq!((rd.get_range(), rd.get_value(), rd.ec_tell()), entropy);
        }
    }

    #[test]
    fn inverse_spreading_matches_fixed_libopus() {
        let cases: &[(usize, usize, usize, u8, &[i16])] = &[
            (
                16,
                1,
                2,
                1,
                &[
                    -6205, 2923, -3793, 4457, -6721, 738, -5626, 1798, -7356, 2301, -5127, 3407,
                    -5286, -3791, 8751, -808,
                ],
            ),
            (
                32,
                4,
                3,
                2,
                &[
                    8732, -1109, 2051, 1949, -9415, 6033, -3821, -338, 5799, -3807, 647, 6258,
                    -3330, 10, 7366, -762, 5269, -5817, 927, 7520, -47, -164, 4488, -4380, -1379,
                    5314, 6916, 11299, 684, -1410, 657, -4696,
                ],
            ),
            (
                8,
                1,
                1,
                3,
                &[9359, -885, 2285, 2853, -9910, 4454, -2325, -94],
            ),
            (
                16,
                2,
                0,
                2,
                &[
                    11049, -1213, -728, 6901, -6097, -3604, -854, 1637, 8696, 3539, -3417, 474,
                    5782, -2129, 2883, 2712,
                ],
            ),
        ];
        for &(n, blocks, pulses, spread, expected) in cases {
            let mut x: Vec<i16> = (0..n).map(|i| (i * 7919 % 16384) as i16 - 8192).collect();
            inverse_spread(&mut x, pulses, blocks, spread);
            assert_eq!(x, expected);
        }
    }
}
