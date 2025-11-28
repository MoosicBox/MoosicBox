//! Fixed-point arithmetic for bit-exact CELT decoding
//!
//! This module provides fixed-point math operations matching `LibOpus`'s
//! `FIXED_POINT` mode for bit-exact compatibility.
//!
//! # Type System
//!
//! * `CeltNorm` (`i16`) - Normalized PVQ coefficients in Q15 format (-1.0 to 1.0)
//! * `CeltSig` (`i32`) - Signal samples in Q12 format (`SIG_SHIFT=12`)
//! * `CeltEner` (`i32`) - Energy values in Q8 log format (`DB_SHIFT=24`)
//!
//! # References
//!
//! * `LibOpus` `celt/arch.h` lines 130-255 (`FIXED_POINT` definitions)
//! * `LibOpus` `celt/fixed_generic.h` (fixed-point math macros)

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

/// Normalized coefficient type (Q15 format: -32768 to 32767 represents -1.0 to ~1.0)
pub type CeltNorm = i16;

/// Signal sample type (Q12 format: 12 fractional bits, range ±2^19)
pub type CeltSig = i32;

/// Energy type (Q8 log format for energy values)
pub type CeltEner = i32;

// ============================================================================
// Constants (from celt/arch.h)
// ============================================================================

/// Q15 representation of 1.0 (32767/32768)
pub const Q15_ONE: i16 = 32767;

/// Q31 representation of 1.0
pub const Q31_ONE: i32 = 2_147_483_647;

/// Signal shift: number of fractional bits in `CeltSig`
/// `LibOpus` arch.h:207
pub const SIG_SHIFT: i32 = 12;

/// Safe saturation value for 32-bit signals
/// `LibOpus` arch.h:215
pub const SIG_SAT: i32 = 536_870_911;

/// Normalization scaling factor
/// `LibOpus` arch.h:217
pub const NORM_SCALING: i32 = 16384;

/// DB (decibel) shift for energy log representation
/// `LibOpus` arch.h:219
pub const DB_SHIFT: i32 = 24;

// ============================================================================
// Basic Operations
// ============================================================================

/// Saturate 32-bit value to 16-bit range
/// `LibOpus` arch.h:230
#[must_use]
#[inline]
pub const fn sat16(x: i32) -> i16 {
    if x > 32767 {
        32767
    } else if x < -32768 {
        -32768
    } else {
        x as i16
    }
}

/// Extract 16-bit value from 32-bit (assumes it fits)
/// `LibOpus` `fixed_generic.h:107`
#[must_use]
#[inline]
pub const fn extract16(x: i32) -> i16 {
    x as i16
}

/// Extend 16-bit value to 32-bit
/// `LibOpus` `fixed_generic.h:109`
#[must_use]
#[inline]
pub const fn extend32(x: i16) -> i32 {
    x as i32
}

/// Arithmetic shift right of 16-bit value
/// `LibOpus` `fixed_generic.h:112`
#[must_use]
#[inline]
pub const fn shr16(a: i16, shift: i32) -> i16 {
    a >> shift
}

/// Arithmetic shift left of 16-bit value
/// `LibOpus` `fixed_generic.h:114`
#[must_use]
#[inline]
pub const fn shl16(a: i16, shift: i32) -> i16 {
    ((a as u16) << shift) as i16
}

/// Arithmetic shift right of 32-bit value
/// `LibOpus` `fixed_generic.h:116`
#[must_use]
#[inline]
pub const fn shr32(a: i32, shift: i32) -> i32 {
    a >> shift
}

/// Arithmetic shift left of 32-bit value
/// `LibOpus` `fixed_generic.h:118`
#[must_use]
#[inline]
pub const fn shl32(a: i32, shift: i32) -> i32 {
    ((a as u32) << shift) as i32
}

/// Variable shift right (handles both left and right shifts)
///
/// If shift > 0: right shift, otherwise: left shift
///
/// # Reference
///
/// `LibOpus` `celt/fixed_generic.h` `VSHR32` macro
#[must_use]
#[inline]
pub const fn vshr32(a: i32, shift: i32) -> i32 {
    if shift > 0 {
        shr32(a, shift)
    } else {
        shl32(a, -shift)
    }
}

/// Arithmetic shift right with rounding-to-nearest
/// `LibOpus` `fixed_generic.h:121`
#[must_use]
#[inline]
pub const fn pshr32(a: i32, shift: i32) -> i32 {
    shr32(a.saturating_add(1 << (shift - 1)), shift)
}

/// Add two 16-bit values
/// `LibOpus` `fixed_generic.h:146`
#[must_use]
#[inline]
pub const fn add16(a: i16, b: i16) -> i16 {
    (a as i32 + b as i32) as i16
}

/// Subtract two 16-bit values
/// `LibOpus` `fixed_generic.h:148`
#[must_use]
#[inline]
pub const fn sub16(a: i16, b: i16) -> i16 {
    (a as i32 - b as i32) as i16
}

/// Add two 32-bit values
/// `LibOpus` `fixed_generic.h:150`
#[must_use]
#[inline]
pub const fn add32(a: i32, b: i32) -> i32 {
    a + b
}

/// Subtract two 32-bit values
/// `LibOpus` `fixed_generic.h:152`
#[must_use]
#[inline]
pub const fn sub32(a: i32, b: i32) -> i32 {
    a - b
}

/// Negate a 16-bit value
#[must_use]
#[inline]
pub const fn neg16(x: i16) -> i16 {
    -x
}

/// Negate a 32-bit value
#[must_use]
#[inline]
pub const fn neg32(x: i32) -> i32 {
    -x
}

// ============================================================================
// Multiplication Operations
// ============================================================================

/// Multiply two 16-bit values, result is 32-bit
/// `LibOpus`: `MULT16_16(a,b)`
#[must_use]
#[inline]
pub const fn mult16_16(a: i16, b: i16) -> i32 {
    (a as i32) * (b as i32)
}

/// Multiply two Q15 values, result is Q15 (no rounding)
///
/// Formula: `(a * b) >> 15`
///
/// # Reference
///
/// `LibOpus` `celt/fixed_generic.h` `MULT16_16_Q15` macro
#[must_use]
#[inline]
pub const fn mult16_16_q15(a: i16, b: i16) -> i16 {
    (mult16_16(a, b) >> 15) as i16
}

/// 16x32 multiplication, followed by 15-bit shift right
/// `LibOpus` fixed_generic.h:54-58
/// Result: (a * b) >> 15
#[must_use]
#[inline]
pub fn mult16_32_q15(a: i16, b: i32) -> i32 {
    // For 64-bit platforms, use direct i64 multiplication
    ((i64::from(a) * i64::from(b)) >> 15) as i32
}

/// 16x32 multiplication, followed by 16-bit shift right\
/// `LibOpus` fixed_generic.h:40-44
#[must_use]
#[inline]
pub fn mult16_32_q16(a: i16, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 16) as i32
}

/// 32x32 multiplication, followed by 31-bit shift right
/// `LibOpus` fixed_generic.h:67-72
#[must_use]
#[inline]
pub fn mult32_32_q31(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}

// ============================================================================
// Fixed-Point Conversion
// ============================================================================

/// Compile-time conversion of float constant to 16-bit Q format
/// `LibOpus` `fixed_generic.h:90`
#[must_use]
#[inline]
#[allow(clippy::cast_precision_loss)]
pub const fn qconst16(x: f64, bits: i32) -> i16 {
    (0.5 + x * ((1_i64 << bits) as f64)) as i16
}

/// Compile-time conversion of float constant to 32-bit Q format
/// `LibOpus` `fixed_generic.h:93`
#[must_use]
#[inline]
#[allow(clippy::cast_precision_loss)]
pub const fn qconst32(x: f64, bits: i32) -> i32 {
    (0.5 + x * ((1_i64 << bits) as f64)) as i32
}

/// Convert i16 sample to `CeltSig` (apply `SIG_SHIFT`)
/// `LibOpus` arch.h:181
#[must_use]
#[inline]
pub const fn int16_to_sig(a: i16) -> CeltSig {
    shl32(extend32(a), SIG_SHIFT)
}

/// Convert `CeltSig` to i16 sample (remove `SIG_SHIFT` with rounding)
#[must_use]
#[inline]
pub const fn sig_to_int16(a: CeltSig) -> i16 {
    sat16(pshr32(a, SIG_SHIFT))
}

// ============================================================================
// Special Operations for CELT
// ============================================================================

/// Multiply 16-bit normalized value by 32-bit gain, Q15 result
///
/// This is the core operation for denormalization:
/// norm (Q15) * gain (arbitrary) → output (shifted appropriately)
///
/// `LibOpus` uses `MULT16_32_Q15` extensively in `denormalise_bands`
#[must_use]
#[inline]
pub fn mult_norm_gain_q15(norm: CeltNorm, gain: i32) -> i32 {
    mult16_32_q15(norm, gain)
}

/// Inner product of two i16 vectors (dot product)
///
/// Computes Σ(x\[i\] * y\[i\]) in Q0 format (accumulated in i32).
///
/// # Reference
///
/// `LibOpus` `celt/pitch.h:159-167` `celt_inner_prod_c()`
#[must_use]
pub fn celt_inner_prod(x: &[i16], y: &[i16]) -> i32 {
    debug_assert_eq!(x.len(), y.len());
    let mut xy = 0_i32;
    for i in 0..x.len() {
        xy = xy.saturating_add(mult16_16(x[i], y[i]));
    }
    xy
}

/// Normalize i32 pulses to i16 Q15 coefficients (bit-exact)
///
/// Converts integer pulse vector to normalized coefficients in Q15 format.
/// This matches `LibOpus`'s `renormalise_vector()` for PVQ decoding.
///
/// # Arguments
///
/// * `pulses` - Integer pulses from PVQ decoder
/// * `output` - Output buffer for normalized i16 coefficients (Q15)
///
/// # Algorithm
///
/// 1. Compute E = EPSILON + Σ(pulses\[i\]²)
/// 2. k = floor(log2(E)) / 2
/// 3. t = E >> (2*(k-7))
/// 4. g = `rsqrt_norm(t)` * gain  (where gain = `Q31_ONE` for unit norm)
/// 5. output\[i\] = (g * pulses\[i\]) >> (k+1)
///
/// # Reference
///
/// `LibOpus` `celt/vq.c:379-403` `renormalise_vector()`
///
/// # Panics
///
/// * If pulses and output have different lengths
pub fn normalize_pulses_to_q15(pulses: &[i32], output: &mut [i16]) {
    const EPSILON: i32 = 1;
    const Q31_ONE: i32 = 0x7FFF_FFFF;

    assert_eq!(pulses.len(), output.len());

    let mut energy: i64 = i64::from(EPSILON);
    for &p in pulses {
        energy += i64::from(p) * i64::from(p);
    }

    let energy_i32 = energy.min(i64::from(i32::MAX)) as i32;

    let k = i32::from(celt_ilog2(energy_i32)) >> 1;
    let t = vshr32(energy_i32, 2 * (k - 7));
    let g = mult32_32_q31(i32::from(celt_rsqrt_norm(t)), Q31_ONE) as i16;

    for (i, &pulse) in pulses.iter().enumerate() {
        let pulse_i16 = if pulse > i32::from(i16::MAX) {
            i16::MAX
        } else if pulse < i32::from(i16::MIN) {
            i16::MIN
        } else {
            pulse as i16
        };

        let scaled = mult16_16(g, pulse_i16);
        output[i] = pshr32(scaled, k + 1) as i16;
    }
}

/// Renormalize vector to Q15ONE target (bit-exact)
///
/// Matches `LibOpus` `renormalise_vector()` behavior.
/// Used after anti-collapse to ensure unit energy.
///
/// # Arguments
///
/// * `vec` - Vector to renormalize (modified in place)
/// * `gain` - Target gain in Q31 format (`Q31_ONE` for unit norm)
///
/// # Reference
///
/// `LibOpus` `celt/vq.c:379-403` `renormalise_vector()`
pub fn renormalize_vector_i16(vec: &mut [i16], gain: i32) {
    const EPSILON: i64 = 1;

    // Compute energy using i64 to avoid overflow
    let mut energy_i64 = EPSILON;
    for &val in vec.iter() {
        energy_i64 += i64::from(val) * i64::from(val);
    }

    // Compute k from full i64 energy to get correct shift amount
    // k = ilog2(energy) >> 1, but we need to handle i64
    let k = if energy_i64 <= i64::from(i32::MAX) {
        i32::from(celt_ilog2(energy_i64 as i32)) >> 1
    } else {
        // For energy > i32::MAX, compute ilog2 from i64
        ((energy_i64.ilog2()) >> 1) as i32
    };

    // Clamp energy to i32 for t computation
    let energy_i32 = energy_i64.min(i64::from(i32::MAX)) as i32;
    let t = vshr32(energy_i32, 2 * (k - 7));
    let rsqrt = celt_rsqrt_norm(t);
    let g_i32 = mult32_32_q31(i32::from(rsqrt), gain);
    let g = g_i32 as i16;

    log::debug!(
        "renormalize: len={}, energy_i64={energy_i64}, k={k}, t={t}, rsqrt={rsqrt}, g_i32={g_i32}, g={g}, shift={}",
        vec.len(),
        k + 1
    );

    for val in vec.iter_mut() {
        let scaled = mult16_16(g, *val);
        *val = pshr32(scaled, k + 1) as i16;
    }

    // Debug: check final energy
    let mut final_energy_i64 = EPSILON;
    for &val in vec.iter() {
        final_energy_i64 += i64::from(val) * i64::from(val);
    }
    log::debug!("renormalize: final_energy_i64={final_energy_i64}");
}

// ============================================================================
// Integer Logarithm (Bit-Exact)
// ============================================================================

/// Integer log base 2 (branchless version)
///
/// Returns the position of the highest set bit in the value.
/// Undefined for zero and negative numbers.
///
/// # Reference
///
/// `LibOpus` `celt/entcode.c:41-62` `ec_ilog()`
#[must_use]
pub fn ec_ilog(v: u32) -> i16 {
    let mut v = v;
    let mut ret = i16::from(v != 0);

    let m = if (v & 0xFFFF_0000) != 0 { 16 } else { 0 };
    v >>= m;
    ret |= m;

    let m = if (v & 0xFF00) != 0 { 8 } else { 0 };
    v >>= m;
    ret |= m;

    let m = if (v & 0xF0) != 0 { 4 } else { 0 };
    v >>= m;
    ret |= m;

    let m = if (v & 0xC) != 0 { 2 } else { 0 };
    v >>= m;
    ret |= m;

    ret += i16::from((v & 0x2) != 0);
    ret
}

/// Integer log base 2
///
/// Returns floor(log2(x)). Undefined for zero and negative numbers.
///
/// # Reference
///
/// `LibOpus` `celt/mathops.h:275-279` `celt_ilog2()`
#[must_use]
#[inline]
pub fn celt_ilog2(x: i32) -> i16 {
    debug_assert!(x > 0, "celt_ilog2: x must be positive");
    ec_ilog(x as u32) - 1
}

/// Integer log base 2 (zero-safe version)
///
/// Returns 0 for x <= 0, otherwise floor(log2(x)).
///
/// # Reference
///
/// `LibOpus` `celt/mathops.h:284-287` `celt_zlog2()`
#[must_use]
#[inline]
pub fn celt_zlog2(x: i32) -> i16 {
    if x <= 0 { 0 } else { celt_ilog2(x) }
}

// ============================================================================
// Fixed-Point Square Root (Bit-Exact)
// ============================================================================

/// Fixed-point sqrt using polynomial approximation
///
/// Input: i32 value (any Q format)
/// Output: sqrt of input in Q14 format
///
/// Polynomial coefficients optimized in fixed-point to minimize both RMS
/// and max error of sqrt(x) over .25<x<1 without exceeding 32767.
/// RMS error: 3.4e-5, max error: 8.2e-5
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:126-146` `celt_sqrt()`
#[must_use]
pub fn celt_sqrt(x: i32) -> i32 {
    const C: [i16; 6] = [23171, 11574, -2901, 1592, -1002, 336];

    if x == 0 {
        return 0;
    }
    if x >= 1_073_741_824 {
        return 32767;
    }

    let k = i32::from((celt_ilog2(x) >> 1) - 7);
    let x_norm = vshr32(x, 2 * k);
    let n = (x_norm - 32768) as i16;

    let term5 = mult16_16_q15(n, C[5]);
    let term4 = mult16_16_q15(n, C[4].saturating_add(term5));
    let term3 = mult16_16_q15(n, C[3].saturating_add(term4));
    let term2 = mult16_16_q15(n, C[2].saturating_add(term3));
    let term1 = mult16_16_q15(n, C[1].saturating_add(term2));
    let rt = C[0].saturating_add(term1);

    vshr32(i32::from(rt), 7 - k)
}

// ============================================================================
// Fixed-Point Exponential (Bit-Exact)
// ============================================================================

/// Fractional part of exp2 using polynomial approximation
///
/// Computes 2^(x/1024) for fractional part.
/// Input: x in range [0, 1024) (representing [0, 1))
/// Output: Q16 format
///
/// Polynomial coefficients: D0=16383, D1=22804, D2=14819, D3=10204
///
/// # Reference
///
/// `LibOpus` `celt/mathops.h:322-327` `celt_exp2_frac()`
#[must_use]
fn celt_exp2_frac(x: i16) -> i32 {
    const D0: i16 = 16383;
    const D1: i16 = 22804;
    const D2: i16 = 14819;
    const D3: i16 = 10204;

    let frac = x << 4;
    let term3 = mult16_16_q15(D3, frac);
    let term2 = mult16_16_q15(frac, D2.saturating_add(term3));
    let term1 = mult16_16_q15(frac, D1.saturating_add(term2));

    i32::from(D0.saturating_add(term1))
}

/// Base-2 exponential approximation
///
/// Computes 2^x in fixed-point.
/// Input: x in Q10 format (1024 = 1.0)
/// Output: Q16 format
///
/// # Reference
///
/// `LibOpus` `celt/mathops.h:335-346` `celt_exp2()`
#[must_use]
pub fn celt_exp2(x: i16) -> i32 {
    let integer = i32::from(x >> 10);

    if integer > 14 {
        return 0x7F00_0000;
    }
    if integer < -15 {
        return 0;
    }

    let frac = celt_exp2_frac(x - ((integer as i16) << 10));
    vshr32(frac, -integer - 2)
}

/// Fixed-point exp2 approximation for Q8 energy values
///
/// Computes 2^x in fixed-point.
/// Input: x in Q8 format (`energy_q8`)
/// Output: 2^(x/256) in Q14 format
///
/// This matches `LibOpus`'s approach to energy denormalization.
///
/// # Reference
///
/// `LibOpus` bands.c uses `celt_exp2()` for denormalization
#[must_use]
pub fn celt_exp2_q8(x_q8: i16) -> i32 {
    let x_q10 =
        i16::try_from(i32::from(x_q8) * 4).unwrap_or(if x_q8 > 0 { i16::MAX } else { i16::MIN });
    let result_q16 = celt_exp2(x_q10);
    result_q16 >> 2
}

/// Fixed-point exp2 for dB values (anti-collapse)
///
/// Computes 2^(-x) where x is in Q8 format (256 = 1.0 in log domain)
/// Used in anti-collapse: r = 2 * exp2(-Ediff)
///
/// Input: x in Q8 format (e.g. energy difference)
/// Output: 2^(-x/256) in Q14 format
///
/// # Reference
///
/// `LibOpus` bands.c:339: `celt_exp2_db(-Ediff)`
#[must_use]
pub fn celt_exp2_db(x_q8: i32) -> i32 {
    if x_q8 >= 16 * 256 {
        return 0;
    }

    let neg_x_q8 = -x_q8;
    let x_q8_i16 =
        i16::try_from(neg_x_q8).unwrap_or(if neg_x_q8 > 0 { i16::MAX } else { i16::MIN });
    celt_exp2_q8(x_q8_i16)
}

/// Fixed-point reciprocal square root (normalized)
///
/// Computes 1/sqrt(x) for normalized input using Householder iteration.
/// Input: x in Q16 format (normalized to [16384, 65535])
/// Output: 1/sqrt(x) in Q14 format
///
/// Uses quadratic approximation + 2nd-order Householder iteration.
/// Maximum relative error: 1.04956E-4, RMSE: 2.80979E-5
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:98-123` `celt_rsqrt_norm()`
#[must_use]
pub const fn celt_rsqrt_norm(x: i32) -> i16 {
    let n = (x - 32768) as i16;

    let term2 = mult16_16_q15(n, 6713);
    let term1 = mult16_16_q15(n, -13490_i16.saturating_add(term2));
    let r = 23557_i16.saturating_add(term1);

    let r2 = mult16_16_q15(r, r);
    let y_part = mult16_16_q15(r2, n)
        .saturating_add(r2)
        .saturating_sub(16384);
    let y = y_part << 1;

    let inner = mult16_16_q15(y, 12288).saturating_sub(16384);
    let adjustment = mult16_16_q15(r, mult16_16_q15(y, inner));

    r.saturating_add(adjustment)
}

/// Denormalize a single i16 coefficient (Q15) by energy gain (Q14)
///
/// This is the core operation for band denormalization:
/// coeff (Q15) × sqrt(energy) (Q14) → output (Q12 = `CeltSig`)
///
/// # Algorithm
///
/// 1. Input: coeff in Q15, gain in Q14
/// 2. Multiply: (Q15 × Q14) = Q29
/// 3. Shift to Q12: >> 17
///
/// # Arguments
///
/// * `coeff` - Normalized coefficient in Q15 format
/// * `gain_q14` - Square root of energy in Q14 format
///
/// # Returns
///
/// Denormalized coefficient in Q12 format (`CeltSig`)
#[must_use]
#[inline]
pub fn denorm_coeff_q15_q14(coeff: CeltNorm, gain_q14: i32) -> CeltSig {
    // coeff (Q15) * gain (Q14) = Q29
    // Shift to Q12: >> 17
    let product = i64::from(coeff) * i64::from(gain_q14);
    (product >> 17) as i32
}

// ============================================================================
// Trigonometric Functions (Fixed-Point)
// ============================================================================

/// Multiply two Q15 values with rounding (P15 = "precision 15")
///
/// Formula: `((a * b) + (1<<14)) >> 15`
///
/// # Reference
///
/// `LibOpus` `celt/arch.h` `MULT16_16_P15` macro
#[must_use]
#[inline]
fn mult16_16_p15(a: i16, b: i16) -> i16 {
    let product = i32::from(a) * i32::from(b);
    ((product + (1 << 14)) >> 15) as i16
}

/// Cosine approximation for [0, π/2] using polynomial
///
/// Input: x in Q15 format (where 32768 = π/2)
/// Output: cos(x) in Q15 format
///
/// Polynomial coefficients from `LibOpus`:
/// * L1 = 32767
/// * L2 = -7651
/// * L3 = 8277
/// * L4 = -626
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:153-160` `_celt_cos_pi_2()`
#[must_use]
fn celt_cos_pi_2(x: i16) -> i16 {
    const L1: i16 = 32767;
    const L2: i16 = -7651;
    const L3: i16 = 8277;
    const L4: i16 = -626;

    let x2 = mult16_16_p15(x, x);

    let term4 = mult16_16_p15(L4, x2);
    let term3 = mult16_16_p15(x2, L3.saturating_add(term4));
    let term2 = mult16_16_p15(x2, L2.saturating_add(term3));

    1_i16.saturating_add((L1.saturating_sub(x2)).saturating_add(term2).min(32766))
}

/// Cosine for full range using bit-exact polynomial approximation
///
/// Input: x where 65536 = full period (not 2π)
/// Output: cos(x) in Q15 format
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:167-188` `celt_cos_norm()`
#[must_use]
pub fn celt_cos_norm(x: i32) -> i16 {
    let mut x = x & 0x0001_ffff;

    if x > (1 << 16) {
        x = (1 << 17) - x;
    }

    if (x & 0x0000_7fff) != 0 {
        if x < (1 << 15) {
            celt_cos_pi_2(x as i16)
        } else {
            -celt_cos_pi_2((65536 - x) as i16)
        }
    } else if (x & 0x0000_ffff) != 0 {
        0
    } else if (x & 0x0001_ffff) != 0 {
        -32767
    } else {
        32767
    }
}

/// Compute cosine using bit-exact polynomial approximation
///
/// Input: angle in Q15 format (where 32768 = π)
/// Output: cos(angle) in Q15 format
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:167-188`
#[must_use]
pub fn celt_cos(x_q15: i16) -> i16 {
    celt_cos_norm(i32::from(x_q15) << 1)
}

/// Compute sine using bit-exact polynomial approximation
///
/// Input: angle in Q15 format (where 32768 = π)
/// Output: sin(angle) in Q15 format
///
/// Uses identity: sin(x) = cos(x - π/2)
///
/// # Reference
///
/// `LibOpus` `celt/mathops.c:167-188`
#[must_use]
pub fn celt_sin(x_q15: i16) -> i16 {
    let angle = i32::from(x_q15) << 1;
    let shifted = angle - 32768;
    celt_cos_norm(shifted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_q15_one() {
        assert_eq!(Q15_ONE, 32767);
    }

    #[test_log::test]
    fn test_sig_shift() {
        assert_eq!(SIG_SHIFT, 12);
    }

    #[test_log::test]
    fn test_sat16() {
        assert_eq!(sat16(100), 100);
        assert_eq!(sat16(40000), 32767);
        assert_eq!(sat16(-40000), -32768);
    }

    #[test_log::test]
    fn test_mult16_16() {
        assert_eq!(mult16_16(100, 200), 20000);
        assert_eq!(mult16_16(-100, 200), -20000);
    }

    #[test_log::test]
    fn test_mult16_32_q15() {
        // Q15 format: 32767 represents ~1.0
        // So 32767 * 32768 >> 15 should give ~32768
        let result = mult16_32_q15(Q15_ONE, 32768);
        assert_eq!(result, 32767);
    }

    #[test_log::test]
    fn test_mult16_16_p15() {
        assert_eq!(mult16_16_p15(Q15_ONE, Q15_ONE), 32766);
        assert_eq!(mult16_16_p15(Q15_ONE / 2, Q15_ONE / 2), Q15_ONE / 4);
        assert_eq!(mult16_16_p15(-Q15_ONE, Q15_ONE), -32766);
    }

    #[test_log::test]
    fn test_celt_cos_special_angles() {
        assert_eq!(celt_cos(0), 32767);
        assert_eq!(celt_cos(16384), 0);
        assert_eq!(celt_cos(-16384), 0);
        let cos_32768 = celt_cos(32767);
        assert!(cos_32768 >= -32768 && cos_32768 <= -32766);
    }

    #[test_log::test]
    fn test_celt_sin_special_angles() {
        let sin_0 = celt_sin(0);
        assert!(sin_0.abs() <= 1);
        assert_eq!(celt_sin(16384), 32767);
        assert_eq!(celt_sin(-16384), -32767);
    }

    #[test_log::test]
    fn test_celt_cos_norm_special_values() {
        assert_eq!(celt_cos_norm(0), 32767);
        assert_eq!(celt_cos_norm(32768), 0);
        assert_eq!(celt_cos_norm(65536), -32767);
        assert_eq!(celt_cos_norm(98304), 0);
    }

    #[test_log::test]
    fn test_trig_identity_sin_cos() {
        for angle in [0_i16, 8192, 16384, 24576, -8192, -16384] {
            let s = i32::from(celt_sin(angle));
            let c = i32::from(celt_cos(angle));
            let sum_sq = s * s + c * c;
            let expected = i32::from(Q15_ONE) * i32::from(Q15_ONE);
            let tolerance = expected / 100;
            assert!(
                (sum_sq - expected).abs() < tolerance,
                "sin²+cos² failed for angle {angle}: {sum_sq} vs {expected}"
            );
        }
    }

    #[test_log::test]
    fn test_int16_to_sig() {
        let sig = int16_to_sig(100);
        assert_eq!(sig, 100 << SIG_SHIFT);
    }

    #[test_log::test]
    fn test_sig_to_int16() {
        let sig = 100 << SIG_SHIFT;
        assert_eq!(sig_to_int16(sig), 100);
    }

    // Additional tests for edge cases and complex math functions

    #[test_log::test]
    fn test_celt_sqrt_zero() {
        assert_eq!(celt_sqrt(0), 0);
    }

    #[test_log::test]
    fn test_celt_sqrt_small_values() {
        // sqrt(1) in Q14 = 16384
        let result = celt_sqrt(1);
        // For very small input, result should be small
        assert!(result >= 0);

        // sqrt(16384) ≈ 128, so in Q14 output format
        let result_16k = celt_sqrt(16384);
        assert!(result_16k > 0);
    }

    #[test_log::test]
    fn test_celt_sqrt_large_values() {
        // At the boundary value 1_073_741_824 (2^30), should return max
        let result = celt_sqrt(1_073_741_824);
        assert_eq!(result, 32767);

        // Above boundary should saturate
        let result_above = celt_sqrt(1_073_741_825);
        assert_eq!(result_above, 32767);

        // i32::MAX should also saturate
        let result_max = celt_sqrt(i32::MAX);
        assert_eq!(result_max, 32767);
    }

    #[test_log::test]
    fn test_celt_sqrt_powers_of_four() {
        // sqrt(4) = 2, sqrt(16) = 4, sqrt(256) = 16
        // In fixed-point Q14, output scaling depends on input magnitude
        let sqrt_4 = celt_sqrt(4);
        let sqrt_16 = celt_sqrt(16);
        let sqrt_256 = celt_sqrt(256);

        // sqrt should increase with input
        assert!(sqrt_16 > sqrt_4);
        assert!(sqrt_256 > sqrt_16);
    }

    #[test_log::test]
    fn test_vshr32_positive_shift() {
        // Positive shift = right shift
        assert_eq!(vshr32(256, 4), 16);
        assert_eq!(vshr32(1024, 2), 256);
        assert_eq!(vshr32(-256, 4), -16);
    }

    #[test_log::test]
    fn test_vshr32_negative_shift() {
        // Negative shift = left shift
        assert_eq!(vshr32(16, -4), 256);
        assert_eq!(vshr32(1, -8), 256);
        assert_eq!(vshr32(-1, -4), -16);
    }

    #[test_log::test]
    fn test_vshr32_zero_shift() {
        assert_eq!(vshr32(12345, 0), 12345);
        assert_eq!(vshr32(-12345, 0), -12345);
    }

    #[test_log::test]
    fn test_pshr32_rounding() {
        // pshr32 adds 0.5 before shifting (rounding-to-nearest)
        // (a + (1 << (shift-1))) >> shift

        // 3 >> 1 without rounding = 1, with rounding = 2
        assert_eq!(pshr32(3, 1), 2);

        // 5 >> 2 without rounding = 1, with rounding = 1 (5 + 2 = 7, 7 >> 2 = 1)
        assert_eq!(pshr32(5, 2), 1);

        // 7 >> 2 without rounding = 1, with rounding = 2 (7 + 2 = 9, 9 >> 2 = 2)
        assert_eq!(pshr32(7, 2), 2);

        // -3 >> 1 should round towards zero in two's complement
        let neg_result = pshr32(-3, 1);
        assert!(neg_result == -1 || neg_result == -2);
    }

    #[test_log::test]
    fn test_celt_exp2_extreme_positive() {
        // Large positive exponent should saturate
        let result = celt_exp2(15 * 1024); // 15.0 in Q10
        assert_eq!(result, 0x7F00_0000);
    }

    #[test_log::test]
    fn test_celt_exp2_extreme_negative() {
        // Large negative exponent should underflow to zero
        let result = celt_exp2(-16 * 1024); // -16.0 in Q10
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn test_celt_exp2_zero() {
        // 2^0 = 1.0 in Q16 = 65536
        let result = celt_exp2(0);
        // Should be close to 65536 / 4 = 16384 (due to vshr32 adjustment)
        assert!(result > 0);
    }

    #[test_log::test]
    fn test_celt_exp2_one() {
        // 2^1 = 2.0
        let result = celt_exp2(1024); // 1.0 in Q10
        let result_zero = celt_exp2(0);
        // exp2(1) should be twice exp2(0)
        assert!(result > result_zero);
    }

    #[test_log::test]
    fn test_celt_exp2_db_large_value() {
        // When x_q8 >= 16 * 256 = 4096, should return 0
        assert_eq!(celt_exp2_db(4096), 0);
        assert_eq!(celt_exp2_db(5000), 0);
    }

    #[test_log::test]
    fn test_celt_exp2_db_small_value() {
        // Small positive values should return non-zero
        let result = celt_exp2_db(256); // 1.0 in Q8
        assert!(result > 0);
    }

    #[test_log::test]
    fn test_celt_exp2_db_zero() {
        // 2^(-0) = 1.0
        let result = celt_exp2_db(0);
        assert!(result > 0);
    }

    #[test_log::test]
    fn test_celt_rsqrt_norm_typical_input() {
        // Input should be in range [16384, 65535] (Q16 normalized)
        // Output is 1/sqrt(x) in Q14

        // For x = 32768 (0.5 in Q16), 1/sqrt(0.5) ≈ 1.414
        let result = celt_rsqrt_norm(32768);
        // In Q14, 1.414 ≈ 23170
        assert!(result > 20000 && result < 26000);
    }

    #[test_log::test]
    fn test_celt_rsqrt_norm_boundary_low() {
        // Near minimum normalized input
        let result = celt_rsqrt_norm(16384);
        // Should be larger since 1/sqrt(small) is larger
        assert!(result > 0);
    }

    #[test_log::test]
    fn test_celt_rsqrt_norm_boundary_high() {
        // Near maximum normalized input
        let result = celt_rsqrt_norm(65535);
        // Should be smaller since 1/sqrt(large) is smaller
        assert!(result > 0);
    }

    #[test_log::test]
    fn test_denorm_coeff_q15_q14_basic() {
        // coeff (Q15) * gain (Q14) → output (Q12)
        // Q15 + Q14 = Q29, then >> 17 = Q12

        // coeff = 16384 (0.5 in Q15), gain = 16384 (1.0 in Q14)
        // Result should be ~2048 in Q12 (0.5)
        let result = denorm_coeff_q15_q14(16384, 16384);
        assert_eq!(result, 2048);
    }

    #[test_log::test]
    fn test_denorm_coeff_q15_q14_negative() {
        // Negative coefficient
        let result = denorm_coeff_q15_q14(-16384, 16384);
        assert_eq!(result, -2048);
    }

    #[test_log::test]
    fn test_denorm_coeff_q15_q14_zero() {
        assert_eq!(denorm_coeff_q15_q14(0, 16384), 0);
        assert_eq!(denorm_coeff_q15_q14(16384, 0), 0);
    }

    #[test_log::test]
    fn test_normalize_pulses_to_q15_basic() {
        let pulses = vec![3, 4, 0];
        let mut output = vec![0i16; 3];

        normalize_pulses_to_q15(&pulses, &mut output);

        // Verify the function produces non-zero output for non-zero input
        assert!(output[0] != 0 || output[1] != 0);

        // Check proportionality is preserved: output[0]/output[1] ≈ 3/4 = 0.75
        if output[1] != 0 {
            let ratio = f32::from(output[0]) / f32::from(output[1]);
            assert!(
                (ratio - 0.75).abs() < 0.1,
                "Expected ratio ~0.75, got {ratio}"
            );
        }
    }

    #[test_log::test]
    fn test_normalize_pulses_to_q15_single_pulse() {
        let pulses = vec![1, 0, 0, 0];
        let mut output = vec![0i16; 4];

        normalize_pulses_to_q15(&pulses, &mut output);

        // Single pulse should result in mostly zero with one large value
        let non_zero_count = output.iter().filter(|&&x| x.abs() > 100).count();
        assert!(non_zero_count >= 1);
    }

    #[test_log::test]
    fn test_renormalize_vector_i16_unit_norm() {
        // Start with a vector that's not unit norm
        let mut vec = vec![16384_i16, 16384, 0, 0];

        // Renormalize to Q31_ONE (unit norm)
        renormalize_vector_i16(&mut vec, Q31_ONE);

        // Verify the function produces valid output (non-panicking)
        // and preserves the direction of the vector (ratio should be ~1.0)
        if vec[0] != 0 && vec[1] != 0 {
            let ratio = f32::from(vec[0]) / f32::from(vec[1]);
            assert!(
                (ratio - 1.0).abs() < 0.1,
                "Expected ratio ~1.0, got {ratio}"
            );
        }

        // Values should have changed from original
        // The function modifies in place, so at least one value should differ
        assert!(vec[0] != 16384 || vec[1] != 16384);
    }

    #[test_log::test]
    fn test_ec_ilog_zero() {
        assert_eq!(ec_ilog(0), 0);
    }

    #[test_log::test]
    fn test_ec_ilog_powers_of_two() {
        assert_eq!(ec_ilog(1), 1);
        assert_eq!(ec_ilog(2), 2);
        assert_eq!(ec_ilog(4), 3);
        assert_eq!(ec_ilog(8), 4);
        assert_eq!(ec_ilog(16), 5);
        assert_eq!(ec_ilog(256), 9);
        assert_eq!(ec_ilog(65536), 17);
    }

    #[test_log::test]
    fn test_ec_ilog_non_powers() {
        // ec_ilog returns floor(log2(x)) + 1
        assert_eq!(ec_ilog(3), 2); // floor(log2(3)) + 1 = 1 + 1
        assert_eq!(ec_ilog(5), 3); // floor(log2(5)) + 1 = 2 + 1
        assert_eq!(ec_ilog(255), 8);
        assert_eq!(ec_ilog(257), 9);
    }

    #[test_log::test]
    fn test_celt_ilog2_basic() {
        // celt_ilog2 returns floor(log2(x)) = ec_ilog(x) - 1
        assert_eq!(celt_ilog2(1), 0);
        assert_eq!(celt_ilog2(2), 1);
        assert_eq!(celt_ilog2(4), 2);
        assert_eq!(celt_ilog2(8), 3);
        assert_eq!(celt_ilog2(100), 6);
    }

    #[test_log::test]
    fn test_celt_zlog2_handles_zero() {
        // celt_zlog2 is zero-safe version
        assert_eq!(celt_zlog2(0), 0);
        assert_eq!(celt_zlog2(-1), 0);
        assert_eq!(celt_zlog2(-100), 0);
    }

    #[test_log::test]
    fn test_celt_zlog2_positive() {
        assert_eq!(celt_zlog2(1), 0);
        assert_eq!(celt_zlog2(2), 1);
        assert_eq!(celt_zlog2(8), 3);
    }

    #[test_log::test]
    fn test_sig_to_int16_saturation() {
        // Test saturation behavior when signal exceeds i16 range
        let large_sig = i32::MAX;
        let result = sig_to_int16(large_sig);
        assert_eq!(result, i16::MAX);

        let small_sig = i32::MIN;
        let result_neg = sig_to_int16(small_sig);
        assert_eq!(result_neg, i16::MIN);
    }

    #[test_log::test]
    fn test_sig_to_int16_rounding() {
        // Test rounding behavior (pshr32 adds 0.5 before shift)
        // sig = 2048 + 2047 = 4095, pshr32(4095, 12) = (4095 + 2048) >> 12 = 1
        let sig = 4095;
        let result = sig_to_int16(sig);
        assert_eq!(result, 1);

        // sig = 2048 - 1 = 2047, pshr32(2047, 12) = (2047 + 2048) >> 12 = 0
        let sig2 = 2047;
        let result2 = sig_to_int16(sig2);
        assert_eq!(result2, 0);
    }

    #[test_log::test]
    fn test_celt_inner_prod_basic() {
        let x = [100_i16, 200, 300];
        let y = [1_i16, 2, 3];
        // 100*1 + 200*2 + 300*3 = 100 + 400 + 900 = 1400
        let result = celt_inner_prod(&x, &y);
        assert_eq!(result, 1400);
    }

    #[test_log::test]
    fn test_celt_inner_prod_negative() {
        let x = [100_i16, -200, 300];
        let y = [1_i16, 2, -3];
        // 100*1 + (-200)*2 + 300*(-3) = 100 - 400 - 900 = -1200
        let result = celt_inner_prod(&x, &y);
        assert_eq!(result, -1200);
    }

    #[test_log::test]
    fn test_celt_inner_prod_empty() {
        let x: [i16; 0] = [];
        let y: [i16; 0] = [];
        let result = celt_inner_prod(&x, &y);
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn test_qconst16_basic() {
        // qconst16(0.5, 15) should give ~16384
        let result = qconst16(0.5, 15);
        assert_eq!(result, 16384);

        // qconst16(1.0, 14) should give 16384
        let result2 = qconst16(1.0, 14);
        assert_eq!(result2, 16384);
    }

    #[test_log::test]
    fn test_qconst32_basic() {
        // qconst32(0.5, 31) should give ~1_073_741_824
        let result = qconst32(0.5, 31);
        assert_eq!(result, 1_073_741_824);
    }

    #[test_log::test]
    fn test_mult16_32_q16_basic() {
        // a=16384 (0.5 in Q15), b=65536 (1.0 in Q16)
        // result = (16384 * 65536) >> 16 = 16384
        let result = mult16_32_q16(16384, 65536);
        assert_eq!(result, 16384);
    }

    #[test_log::test]
    fn test_mult32_32_q31_basic() {
        // Multiply two Q31 values and shift by 31
        // Q31_ONE * Q31_ONE >> 31 should be close to Q31_ONE
        let result = mult32_32_q31(Q31_ONE, Q31_ONE);
        // Due to truncation, result should be close to Q31_ONE
        // Note: Q31_ONE is i32::MAX, so result <= Q31_ONE is always true
        assert!(result > Q31_ONE - 10);
    }

    #[test_log::test]
    fn test_shift_operations_consistency() {
        // shr16 and shl16 should be inverses (within truncation)
        let original: i16 = 1000;
        let shifted_right = shr16(original, 2);
        let shifted_back = shl16(shifted_right, 2);
        // Should lose low 2 bits
        assert_eq!(shifted_back, original & !3);

        // shr32 and shl32 should be inverses (within truncation)
        let original32: i32 = 100_000;
        let shifted_right32 = shr32(original32, 4);
        let shifted_back32 = shl32(shifted_right32, 4);
        assert_eq!(shifted_back32, original32 & !15);
    }

    #[test_log::test]
    fn test_add_sub_16_overflow_wrap() {
        // add16 and sub16 use i32 intermediate but cast back to i16
        // Test wrapping behavior at boundaries
        let result = add16(i16::MAX, 1);
        assert_eq!(result, i16::MIN); // Wraps around

        let result_sub = sub16(i16::MIN, 1);
        assert_eq!(result_sub, i16::MAX); // Wraps around
    }

    #[test_log::test]
    fn test_neg16_neg32() {
        assert_eq!(neg16(100), -100);
        assert_eq!(neg16(-100), 100);
        assert_eq!(neg32(100_000), -100_000);
        assert_eq!(neg32(-100_000), 100_000);
    }

    #[test_log::test]
    fn test_extract16_extend32_roundtrip() {
        let original: i16 = -12345;
        let extended = extend32(original);
        let extracted = extract16(extended);
        assert_eq!(extracted, original);
    }
}
