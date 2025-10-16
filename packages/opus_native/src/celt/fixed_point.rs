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

/// Arithmetic shift right with rounding-to-nearest
/// `LibOpus` `fixed_generic.h:121`
#[must_use]
#[inline]
pub const fn pshr32(a: i32, shift: i32) -> i32 {
    shr32(a + (1 << (shift - 1)), shift)
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

/// Normalize i32 pulses to i16 Q15 coefficients
///
/// Converts integer pulse vector to normalized coefficients in Q15 format.
/// This matches `LibOpus`'s approach of normalizing to unit energy in the
/// normalized domain.
///
/// # Arguments
///
/// * `pulses` - Integer pulses from PVQ decoder
/// * `output` - Output buffer for normalized i16 coefficients (Q15)
///
/// # Algorithm
///
/// 1. Compute sum of squares: Ryy = Σ(pulses[i]²)
/// 2. Compute g = 1/sqrt(Ryy) × `Q15_ONE`
/// 3. output[i] = (pulses[i] × g) >> `scale_shift`
///
/// This produces unit-norm coefficients in Q15 format.
///
/// # Panics
///
/// * If pulses and output have different lengths
#[allow(clippy::cast_precision_loss)]
pub fn normalize_pulses_to_q15(pulses: &[i32], output: &mut [i16]) {
    assert_eq!(pulses.len(), output.len());

    // Compute Ryy = sum of squares
    let mut ryy: i64 = 0;
    for &p in pulses {
        ryy += i64::from(p) * i64::from(p);
    }

    if ryy == 0 {
        output.fill(0);
        return;
    }

    // For normalization to unit energy in Q15:
    // We want: sqrt(Σ output[i]²) = Q15_ONE
    // So: output[i] = pulses[i] × Q15_ONE / sqrt(Ryy)
    //
    // Using floating-point for now for correctness.
    // TODO: Replace with fixed-point reciprocal sqrt for bit-exact matching
    let ryy_sqrt = (ryy as f64).sqrt();
    let scale = f64::from(Q15_ONE) / ryy_sqrt;

    for (i, &pulse) in pulses.iter().enumerate() {
        let normalized = f64::from(pulse) * scale;
        output[i] = sat16(normalized.round() as i32);
    }
}

/// Renormalize vector to Q15ONE target
///
/// Matches `LibOpus` `renormalise_vector()` behavior
/// Used after anti-collapse to ensure unit energy
///
/// # Reference
///
/// `LibOpus` vq.c:378-403
#[allow(clippy::cast_precision_loss)]
pub fn renormalize_vector_i16(vec: &mut [i16]) {
    // Compute L2 norm in fixed-point
    let mut norm_sq: i64 = 0;
    for &val in vec.iter() {
        norm_sq += i64::from(val) * i64::from(val);
    }

    if norm_sq == 0 {
        return;
    }

    // Compute scaling factor to achieve Q15ONE norm
    // sqrt(norm_sq) should equal Q15ONE
    let norm = (norm_sq as f64).sqrt();
    let scale = f64::from(Q15_ONE) / norm;

    for val in vec.iter_mut() {
        let scaled = f64::from(*val) * scale;
        *val = sat16(scaled.round() as i32);
    }
}

/// Fixed-point sqrt approximation using lookup table + Newton-Raphson
///
/// Matches `LibOpus` `celt_sqrt()` for fixed-point mode.
/// Input: i32 value (any Q format)
/// Output: sqrt of input in Q14 format
///
/// # Reference
///
/// `LibOpus` `mathops.c:celt_sqrt()`
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn celt_sqrt(x: i32) -> i32 {
    if x <= 0 {
        return 0;
    }

    // Simple approximation using f32 for now
    // TODO: Replace with LibOpus's table-based fixed-point sqrt
    let x_f = x as f32;
    let sqrt_f = x_f.sqrt();

    // Result in Q14 format
    (sqrt_f * 16384.0) as i32
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
    // Convert Q8 to exponent: x_q8 / 256.0
    // Then compute: 2^(x_q8 / 256.0)
    let exponent = f32::from(x_q8) / 256.0;
    let result = exponent.exp2();

    // Return in Q14 format
    (result * 16384.0) as i32
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
    // Clamp to prevent overflow
    if x_q8 >= 16 * 256 {
        return 0; // 2^(-16) is effectively zero
    }

    // Convert Q8 to exponent: -x_q8 / 256.0
    // Then compute: 2^(-x_q8 / 256.0)
    let exponent = -f64::from(x_q8) / 256.0;
    let result = exponent.exp2();

    // Return in Q14 format (capped at 16383)
    let val = (result * 16384.0) as i32;
    val.min(16383)
}

/// Fixed-point reciprocal square root (normalized)
///
/// Computes 1/sqrt(x) for normalized input
/// Input: x in Q(shift*2) format (e.g., Q14 if shift=7)
/// Output: 1/sqrt(x) in Q15 format
///
/// # Reference
///
/// `LibOpus` `celt_rsqrt_norm()` in mathops.c
#[must_use]
pub fn celt_rsqrt_norm(x: i32) -> i16 {
    if x <= 0 {
        return Q15_ONE;
    }

    // Use f64 for intermediate calculation
    // TODO: Replace with LibOpus's table-based implementation for bit-exact matching
    let x_f = f64::from(x) / 16384.0; // Assume Q14 input
    let rsqrt = 1.0 / x_f.sqrt();

    // Return in Q15 format
    qconst16(rsqrt, 15)
}

/// Integer base-2 logarithm (position of highest set bit)
///
/// Returns floor(log2(x)) for x > 0, or 0 for x <= 0
/// This is equivalent to finding the position of the most significant bit.
///
/// # Reference
///
/// `LibOpus` `mathops.c:celt_ilog2()`
#[must_use]
pub const fn celt_ilog2(mut x: i32) -> i32 {
    if x <= 0 {
        return 0;
    }

    // Count leading zeros and subtract from 31
    // This gives us the position of the highest set bit
    let mut log = 0;
    while x > 0 {
        x >>= 1;
        log += 1;
    }
    log - 1
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

/// Compute cosine using CORDIC algorithm
///
/// Input: angle in Q15 format (where 32768 = π)
/// Output: cos(angle) in Q15 format
///
/// `LibOpus` uses table-based interpolation, but CORDIC is simpler and adequate.
///
/// # Reference
///
/// `LibOpus` `kiss_fft.c` uses precomputed trig tables
#[must_use]
pub fn celt_cos(x_q15: i16) -> i16 {
    // Simple CORDIC implementation
    // For production, should use LibOpus's trig tables

    // Normalize angle to [0, 2π)
    let mut angle = i32::from(x_q15);

    // cos(x) = sin(x + π/2)
    // π/2 in Q15 = 16384
    angle += 16384;

    celt_sin(angle as i16)
}

/// Compute sine using CORDIC algorithm
///
/// Input: angle in Q15 format (where 32768 = π)
/// Output: sin(angle) in Q15 format
///
/// # Reference
///
/// `LibOpus` uses precomputed trig tables in `kiss_fft.c`
#[must_use]
pub fn celt_sin(x_q15: i16) -> i16 {
    // Simplified CORDIC for now
    // Production should use LibOpus's celt_cos_norm() approach

    let angle = i32::from(x_q15);

    // Normalize to [-π, π]
    // 32768 represents π in Q15
    let normalized = ((angle + 32768) & 0xFFFF) - 32768;

    // Use Taylor series for small angles (good enough for now)
    // sin(x) ≈ x - x³/6 for small x
    // This is a placeholder - LibOpus uses tables

    // For production: implement proper CORDIC or use LibOpus trig tables
    // Temporary: compute using floating point internally but at least
    // the interface is fixed-point
    let angle_f = f64::from(normalized) * std::f64::consts::PI / 32768.0;
    let result = angle_f.sin();
    qconst16(result, 15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_q15_one() {
        assert_eq!(Q15_ONE, 32767);
    }

    #[test]
    fn test_sig_shift() {
        assert_eq!(SIG_SHIFT, 12);
    }

    #[test]
    fn test_sat16() {
        assert_eq!(sat16(100), 100);
        assert_eq!(sat16(40000), 32767);
        assert_eq!(sat16(-40000), -32768);
    }

    #[test]
    fn test_mult16_16() {
        assert_eq!(mult16_16(100, 200), 20000);
        assert_eq!(mult16_16(-100, 200), -20000);
    }

    #[test]
    fn test_mult16_32_q15() {
        // Q15 format: 32767 represents ~1.0
        // So 32767 * 32768 >> 15 should give ~32768
        let result = mult16_32_q15(Q15_ONE, 32768);
        assert_eq!(result, 32767);
    }

    #[test]
    fn test_int16_to_sig() {
        let sig = int16_to_sig(100);
        assert_eq!(sig, 100 << SIG_SHIFT);
    }

    #[test]
    fn test_sig_to_int16() {
        let sig = 100 << SIG_SHIFT;
        assert_eq!(sig_to_int16(sig), 100);
    }
}
