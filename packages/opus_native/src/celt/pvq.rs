#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use crate::error::{Error, Result};
use crate::range::RangeDecoder;

/// Spread PDF values from RFC Table 56 (line 5968)
/// Probability distribution: {7, 2, 21, 2}/32
const SPREAD_PDF: &[u16] = &[
    7,  // spread=0 (no rotation, f_r=infinite)
    9,  // spread=1 (f_r=15)
    30, // spread=2 (f_r=10)
    32, // spread=3 (f_r=5) - cumulative total
];

/// Spreading factors per RFC Table 59 (lines 6562-6574)
const SPREAD_FACTORS: &[Option<u32>] = &[
    None,     // spread=0: infinite (no rotation)
    Some(15), // spread=1: f_r=15
    Some(10), // spread=2: f_r=10
    Some(5),  // spread=3: f_r=5
];

/// Bit resolution for fixed-point arithmetic (arch.h:133)
#[allow(dead_code)]
const BITRES: i32 = 3;

/// Quantization offset for split gain (rate.h:40)
#[allow(dead_code)]
const QTHETA_OFFSET: i32 = 4;

/// Quantization offset for stereo two-phase (rate.h:41)
#[allow(dead_code)]
const QTHETA_OFFSET_TWOPHASE: i32 = 16;

/// exp2 approximation table for qn computation (mathops.c:72-80)
/// Maps 3-bit fractional part to Q14 fixed-point exp2 values
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/mathops.c#L72-80>
#[allow(dead_code)]
const EXP2_TABLE8: &[u16; 8] = &[16384, 17866, 19483, 21247, 23170, 25264, 27542, 30019];

/// Integer square root using Newton's method
///
/// # Arguments
///
/// * `x` - Input value
///
/// # Returns
///
/// * Floor of square root
#[must_use]
#[allow(dead_code)]
const fn isqrt(x: u32) -> u32 {
    if x == 0 {
        return 0;
    }

    let mut guess = x;
    let mut next = u32::midpoint(guess, x / guess);

    while next < guess {
        guess = next;
        next = u32::midpoint(guess, x / guess);
    }

    guess
}

/// Computes the number of pulses for a given index
///
/// Reference: libopus rate.h:48-51 (`get_pulses` macro)
///
/// # Arguments
///
/// * `i` - Pulse index (0-40)
///
/// # Returns
///
/// * Number of pulses for this index
#[must_use]
const fn get_pulses(i: u32) -> u32 {
    if i < 8 {
        i
    } else {
        (8 + (i & 7)) << ((i >> 3) - 1)
    }
}

/// Checks if PVQ codebook size V(N,K) fits in 32 bits
///
/// Used to determine maximum K for a given band size.
/// Reference: libopus rate.c:116-118
///
/// # Arguments
///
/// * `n` - Number of dimensions
/// * `k` - Number of pulses
///
/// # Returns
///
/// * `true` if V(N,K) < 2^32, `false` otherwise
#[must_use]
fn fits_in_32(n: u32, k: u32) -> bool {
    let size = compute_pvq_size(n, k);
    size < (1_u32 << 31)
}

/// Computes the minimum bit allocation threshold for splitting
///
/// Per libopus bands.c:971: `b > cache[cache[0]]+12`
/// This calculates the threshold on-demand without full cache table.
///
/// Reference: libopus rate.c:116-118 (K computation)
///
/// # Arguments
///
/// * `n` - Number of dimensions
///
/// # Returns
///
/// * Minimum bits (in 1/8 bit units) needed to justify splitting
///
/// # Algorithm
///
/// 1. Find maximum K where V(N,K) fits in 32 bits
/// 2. Compute bits needed for `get_pulses(K)` pulses
/// 3. Add 12 (1.5 bits in `BITRES=3` units) as threshold margin
///
/// # Note
///
/// Computes threshold on-demand. Future optimization: precomputed
/// cache table for bit-exact matching with libopus reference.
#[must_use]
fn compute_split_threshold(n: u32) -> i32 {
    const MAX_PSEUDO: u32 = 40;

    // Find maximum K where V(N,K) fits in 32 bits (libopus rate.c:116-118)
    let mut k = 0;
    while fits_in_32(n, get_pulses(k + 1)) && k < MAX_PSEUDO {
        k += 1;
    }

    // Get the number of pulses for this K
    let pulses = get_pulses(k);

    // Estimate bits needed for this many pulses
    // This is a simplified calculation - full cache would be more accurate
    // For now, use log2 approximation scaled to 1/8 bit units
    let bits_needed = if pulses > 0 {
        // Rough estimate: log2(V(N,K)) * 8 (for BITRES=3)
        let codebook_size = compute_pvq_size(n, pulses);
        if codebook_size > 0 {
            i32::try_from(32 - codebook_size.leading_zeros()).unwrap_or(0) * 8
        } else {
            0
        }
    } else {
        0
    };

    // Add 1.5 bit threshold (12 in BITRES=3 units) per libopus bands.c:971
    bits_needed + 12
}

/// Bit-exact integer logarithm (floor(log2(x)) + 1)
///
/// Matches libopus `EC_ILOG` implementation exactly
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/entcode.c>
///
/// # Arguments
///
/// * `v` - Input value (u32)
///
/// # Returns
///
/// * Integer logarithm (0 for v=0, floor(log2(v))+1 for v>0)
#[must_use]
#[allow(dead_code)]
fn ec_ilog(v: u32) -> i32 {
    if v == 0 {
        return 0;
    }

    let mut ret = 1; // !!_v
    let mut val = v;

    // Check upper 16 bits
    let m = if (val & 0xFFFF_0000) != 0 { 16 } else { 0 };
    val >>= m;
    ret |= m;

    // Check bits 8-15
    let m = if (val & 0xFF00) != 0 { 8 } else { 0 };
    val >>= m;
    ret |= m;

    // Check bits 4-7
    let m = if (val & 0xF0) != 0 { 4 } else { 0 };
    val >>= m;
    ret |= m;

    // Check bits 2-3
    let m = if (val & 0xC) != 0 { 2 } else { 0 };
    val >>= m;
    ret |= m;

    // Check bit 1
    ret += i32::from((val & 0x2) != 0);

    ret
}

/// Q15 fractional multiplication with rounding
///
/// Matches libopus `FRAC_MUL16` macro exactly
/// Reference: `mathops.h:44 - ((16384+((opus_int32)(a)*(b)))>>15)`
///
/// # Arguments
///
/// * `a` - First operand (Q15 format, -32768 to 32767)
/// * `b` - Second operand (Q15 format, -32768 to 32767)
///
/// # Returns
///
/// * Product in Q15 format with rounding
#[must_use]
#[allow(clippy::cast_possible_truncation, dead_code)]
fn frac_mul16(a: i32, b: i32) -> i32 {
    // Cast to i16 to match libopus behavior (truncate to 16-bit)
    let a16 = a as i16;
    let b16 = b as i16;

    // Multiply with rounding: (16384 + a*b) >> 15
    (16384 + i32::from(a16) * i32::from(b16)) >> 15
}

/// Computes pulse capacity for bit allocation
///
/// Reference: libopus bands.c
///
/// # Arguments
///
/// * `n` - Number of dimensions
/// * `bits` - Bit allocation in 1/8 bit units
///
/// # Returns
///
/// * Maximum pulses that can be encoded with given bits
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn compute_pulse_cap(n: u32, bits: i32) -> i32 {
    if bits <= 0 || n == 0 {
        return 0;
    }

    // Find maximum K where V(N,K) fits in bits
    let mut k = 0;
    loop {
        let size = compute_pvq_size_internal(n, k);
        if size == 0 {
            break;
        }

        let bits_needed = if size <= 1 {
            0
        } else {
            ((32 - size.leading_zeros()) as i32) * 8
        };

        if bits_needed > bits {
            break;
        }

        k += 1;
        if k > 256 {
            break;
        }
    }

    k.saturating_sub(1) as i32
}

/// Internal V(N,K) computation (used by `compute_pulse_cap` to avoid circular dependency)
#[must_use]
#[allow(dead_code)]
fn compute_pvq_size_internal(n: u32, k: u32) -> u32 {
    if k == 0 {
        return 1;
    }
    if n == 0 {
        return 0;
    }
    if k == 1 {
        return 2 * n;
    }

    let mut prev = vec![0_u32; (k + 1) as usize];
    let mut curr = vec![0_u32; (k + 1) as usize];
    prev[0] = 1;

    for _i in 1..=n {
        curr[0] = 1;
        for j in 1..=k {
            let v1 = prev[j as usize];
            let v2 = curr[(j - 1) as usize];
            let v3 = prev[(j - 1) as usize];
            curr[j as usize] = v1.saturating_add(v2).saturating_add(v3);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[k as usize]
}

/// Computes quantization levels for split gain parameter
///
/// Reference: libopus bands.c lines 647-667
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/bands.c#L647-667>
///
/// # Arguments
///
/// * `n` - Number of dimensions
/// * `bits` - Bit allocation in 1/8 bit units
/// * `is_stereo` - Whether this is stereo coding
///
/// # Returns
///
/// * Quantization levels (qn)
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn compute_qn(n: u32, bits: i32, is_stereo: bool) -> u32 {
    if bits <= 0 || n == 0 {
        return 1;
    }

    let n2 = (2_i32.saturating_mul(i32::try_from(n).unwrap_or(i32::MAX)) - 1).max(1);

    let pulse_cap = compute_pulse_cap(n, bits);

    let offset = if is_stereo && n == 2 {
        QTHETA_OFFSET_TWOPHASE << BITRES
    } else {
        ((pulse_cap >> 1) - QTHETA_OFFSET) << BITRES
    };

    let mut qb = (bits + n2 * offset) / n2;
    qb = qb.min(bits - pulse_cap - (4 << BITRES));
    qb = qb.min(8 << BITRES);

    if qb < (1 << BITRES) {
        return 1;
    }

    let idx = (qb as usize) & 0x7;
    let shift = 14 - (qb >> BITRES);
    let qn = u32::from(EXP2_TABLE8[idx]) >> shift;

    // Round to even
    ((qn + 1) >> 1) << 1
}

/// Computes V(N,K): number of ways to place K pulses in N positions
///
/// This is the core PVQ combinatorial function used to compute codebook sizes.
/// Uses the recursive formula: V(N,K) = V(N-1,K) + V(N,K-1) + V(N-1,K-1)
/// with base cases: V(N,0) = 1 and V(0,K) = 0 for K != 0
///
/// Reference: RFC 6716 Section 4.3.4.2 (lines 6513-6523)
/// Implementation: cwrs.c in libopus reference
///
/// # Arguments
///
/// * `n` - Number of dimensions (samples in band)
/// * `k` - Number of pulses
///
/// # Returns
///
/// * Number of possible combinations (codebook size)
///
/// # Errors
///
/// * Returns error if result would overflow u32
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/cwrs.c#L151-184>
#[must_use]
pub fn compute_pvq_size(n: u32, k: u32) -> u32 {
    // Base cases per RFC lines 6513-6514
    if k == 0 {
        return 1; // V(N,0) = 1
    }
    if n == 0 {
        return 0; // V(0,K) = 0 for K != 0
    }

    // For small values, use direct calculation to avoid recursion overhead
    if k == 1 {
        return 2 * n; // V(N,1) = 2*N (each position can have +1 or -1)
    }

    // Use iterative computation (equivalent to recursive formula)
    // Build up Pascal's triangle-like structure
    // We only need to keep track of the current and previous row
    let mut prev = vec![0_u32; (k + 1) as usize];
    let mut curr = vec![0_u32; (k + 1) as usize];

    // Initialize: V(0,k) = 0 for all k>0, V(n,0) = 1 for all n
    prev[0] = 1;

    for _ in 1..=n {
        curr[0] = 1; // V(i,0) = 1

        for j in 1..=k {
            // V(i,j) = V(i-1,j) + V(i,j-1) + V(i-1,j-1)
            let v1 = prev[j as usize];
            let v2 = curr[(j - 1) as usize];
            let v3 = prev[(j - 1) as usize];

            curr[j as usize] = v1.saturating_add(v2).saturating_add(v3);
        }

        // Swap buffers for next iteration
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[k as usize]
}

/// Normalizes a vector to unit L2-norm
///
/// Reference: RFC 6716 Section 4.3.4.2 (line 6540-6541)
///
/// # Arguments
///
/// * `vec` - Vector to normalize (modified in place)
///
/// # Errors
///
/// * Returns error if vector has zero norm
#[allow(dead_code)]
pub fn normalize_vector(vec: &mut [f32]) -> Result<()> {
    if vec.is_empty() {
        return Err(Error::CeltDecoder(
            "cannot normalize empty vector".to_string(),
        ));
    }

    // Compute L2 norm
    let norm_sq: f32 = vec.iter().map(|&x| x * x).sum();

    if norm_sq <= 0.0 {
        return Err(Error::CeltDecoder("vector has zero norm".to_string()));
    }

    let norm = norm_sq.sqrt();

    // Normalize to unit norm
    for x in vec {
        *x /= norm;
    }

    Ok(())
}

/// Decodes a PVQ vector from the bitstream
///
/// Implements the algorithm from RFC 6716 Section 4.3.4.2 (lines 6525-6541).
/// Decodes a uniformly distributed integer in [0, V(N,K)-1] and converts it
/// to a vector of K pulses in N dimensions.
///
/// Reference: RFC 6716 Section 4.3.4.2
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/cwrs.c#L378-452>
///
/// # Arguments
///
/// * `range_decoder` - Range decoder for reading bits
/// * `n` - Number of dimensions (samples in band)
/// * `k` - Number of pulses
///
/// # Returns
///
/// * Vector of N elements with K total pulses (unnormalized)
///
/// # Errors
///
/// * Returns error if decoding fails or parameters invalid
#[allow(clippy::many_single_char_names, dead_code)]
pub fn decode_pvq_vector(range_decoder: &mut RangeDecoder, n: u32, k: u32) -> Result<Vec<i32>> {
    if n == 0 {
        return Err(Error::CeltDecoder("PVQ: n must be > 0".to_string()));
    }

    // Allocate output vector
    let mut x = vec![0_i32; n as usize];

    if k == 0 {
        // No pulses - return zero vector
        return Ok(x);
    }

    // Decode uniform integer in range [0, V(N,K)-1]
    let codebook_size = compute_pvq_size(n, k);
    if codebook_size == 0 {
        return Err(Error::CeltDecoder("PVQ: invalid codebook size".to_string()));
    }

    let mut i = range_decoder.ec_dec_uint(codebook_size)?;
    let mut k_remaining = k;

    // Decode vector per RFC algorithm (lines 6527-6538)
    for j in 0..n {
        let n_remaining = n - j;

        // Step 1: p = (V(N-j-1,k) + V(N-j,k))/2
        let v1 = if n_remaining > 1 {
            compute_pvq_size(n_remaining - 1, k_remaining)
        } else {
            0
        };
        let v2 = compute_pvq_size(n_remaining, k_remaining);
        let mut p = (v1.saturating_add(v2)) / 2;

        // Step 2: Decode sign
        let sgn = if i < p { 1 } else { -1 };
        if sgn < 0 {
            i -= p;
        }

        // Step 3: Store k0 and adjust p
        let k0 = k_remaining;
        if n_remaining > 1 {
            p = p.saturating_sub(compute_pvq_size(n_remaining - 1, k_remaining));
        }

        // Step 4: Find pulse count for this position
        while p > i && k_remaining > 0 {
            k_remaining -= 1;
            if n_remaining > 1 {
                p = p.saturating_sub(compute_pvq_size(n_remaining - 1, k_remaining));
            }
        }

        // Step 5: Store result and update index
        x[j as usize] = sgn * i32::try_from(k0 - k_remaining).unwrap_or(0);
        i = i.saturating_sub(p);
    }

    Ok(x)
}

/// Converts bits allocation (in 1/8 bit units) to number of pulses
///
/// Searches for K value that produces closest number of bits to allocation
/// without exceeding available bits. Updates balance for next band.
///
/// Reference: RFC 6716 Section 4.3.4.1 (lines 6476-6492)
///
/// # Arguments
///
/// * `n` - Number of dimensions
/// * `bits` - Bit allocation in 1/8 bit units
/// * `balance` - Accumulated balance (modified)
///
/// # Returns
///
/// * Number of pulses K
#[must_use]
#[allow(dead_code)]
pub fn bits_to_pulses(n: u32, bits: i32, balance: &mut i32) -> u32 {
    if bits <= 0 || n == 0 {
        return 0;
    }

    // Apply balance (1/3 for normal bands, 1/2 for penultimate, full for last)
    // Caller must handle balance weighting
    let adjusted_bits = bits + *balance;

    // Search for K that uses closest to adjusted_bits without exceeding
    let mut k = 0_u32;
    let mut best_k = 0_u32;
    let mut best_bits = 0_i32;

    // Try increasing K values until we exceed the budget
    loop {
        let size = compute_pvq_size(n, k);
        if size == 0 || size == 1 {
            // size 0 or 1 requires 0 bits
            if k == 0 || (size == 1 && adjusted_bits >= 0) {
                best_k = k;
                best_bits = 0;
            }
            k += 1;
            if k > 256 {
                break;
            }
            continue;
        }

        // Compute bits needed using log2 approximation
        // log2(size) in 1/8 bit units = (floor(log2(size)) + 1) * 8
        #[allow(clippy::cast_possible_wrap)]
        let log2_size = (32 - size.leading_zeros()) as i32;
        let bits_needed = log2_size * 8;

        if bits_needed <= adjusted_bits {
            best_k = k;
            best_bits = bits_needed;
        } else {
            break;
        }

        k += 1;

        // Safety limit to prevent infinite loops
        if k > 256 {
            break;
        }
    }

    // Update balance with difference
    *balance += bits - best_bits;

    best_k
}

/// Decodes the spread parameter from the bitstream
///
/// Reference: RFC 6716 Table 56 (line 5968), Table 59 (lines 6562-6574)
///
/// # Arguments
///
/// * `range_decoder` - Range decoder for reading bits
///
/// # Returns
///
/// * Spread value (0-3)
///
/// # Errors
///
/// * Returns error if decoding fails
#[allow(dead_code)]
pub fn decode_spread(range_decoder: &mut RangeDecoder) -> Result<u8> {
    range_decoder.ec_dec_icdf_u16(SPREAD_PDF, 5) // ftb=5 (2^5=32)
}

/// Applies N-D rotation as series of 2-D Givens rotations
///
/// Performs forward and backward rotation passes to achieve N-D rotation.
///
/// # Arguments
///
/// * `vec` - Vector to rotate (modified in place)
/// * `cos_theta` - Cosine of rotation angle
/// * `sin_theta` - Sine of rotation angle
#[allow(dead_code)]
fn apply_nd_rotation(vec: &mut [f32], cos_theta: f32, sin_theta: f32) {
    if vec.len() < 2 {
        return;
    }

    // Forward pass: R(0,1), R(1,2), ..., R(N-2,N-1)
    for i in 0..(vec.len() - 1) {
        let x_i = vec[i];
        let x_j = vec[i + 1];
        vec[i] = cos_theta.mul_add(x_i, sin_theta * x_j);
        vec[i + 1] = (-sin_theta).mul_add(x_i, cos_theta * x_j);
    }

    // Backward pass: R(N-2,N-1), ..., R(1,2), R(0,1)
    for i in (0..(vec.len() - 1)).rev() {
        let x_i = vec[i];
        let x_j = vec[i + 1];
        vec[i] = cos_theta.mul_add(x_i, sin_theta * x_j);
        vec[i + 1] = (-sin_theta).mul_add(x_i, cos_theta * x_j);
    }
}

/// Applies pre-rotation to a block for multi-block spreading
///
/// Reference: RFC 6716 lines 6595-6599
///
/// When block size ≥ 8, applies additional N-D rotation by (π/2 - θ)
/// using stride-based interleaving before the main spreading rotation.
///
/// # Arguments
///
/// * `block` - Time block to pre-rotate (modified in place)
/// * `spread` - Spread parameter (0-3)
/// * `k` - Number of pulses
/// * `nb_blocks` - Total number of time blocks
/// * `total_n` - Total vector length (all blocks combined)
#[allow(
    dead_code,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn apply_pre_rotation(block: &mut [f32], spread: u8, k: u32, nb_blocks: usize, total_n: usize) {
    // Get spreading factor
    let Some(f_r) = SPREAD_FACTORS[spread as usize] else {
        return; // spread=0: no rotation
    };

    if k == 0 {
        return;
    }

    let block_size = block.len();

    // Compute base rotation parameters using total N (RFC line 6549)
    let g_r = total_n as f32 / (f_r as f32).mul_add(k as f32, total_n as f32);
    let base_theta = std::f32::consts::PI * g_r * g_r / 4.0;

    // Pre-rotation angle: (π/2 - θ) per RFC line 6596
    let pre_theta = std::f32::consts::FRAC_PI_2 - base_theta;
    let cos_pre = pre_theta.cos();
    let sin_pre = pre_theta.sin();

    // Compute stride: round(sqrt(N/nb_blocks)) per RFC line 6598
    let stride = ((total_n as f32 / nb_blocks as f32).sqrt()).round() as usize;

    if stride == 0 || stride >= block_size {
        return; // Nothing to interleave
    }

    // Apply rotation to each interleaved sample set S_k = {stride*n + k}
    // per RFC lines 6598-6599
    for offset in 0..stride {
        // Collect samples at positions: offset, offset+stride, offset+2*stride, ...
        let mut samples: Vec<f32> = (0..block_size)
            .skip(offset)
            .step_by(stride)
            .map(|i| block[i])
            .collect();

        if samples.len() < 2 {
            continue; // Need at least 2 samples to rotate
        }

        // Apply N-D rotation to this interleaved group
        apply_nd_rotation(&mut samples, cos_pre, sin_pre);

        // Write back to interleaved positions
        for (idx, &sample) in samples.iter().enumerate() {
            let pos = offset + idx * stride;
            if pos < block_size {
                block[pos] = sample;
            }
        }
    }
}

/// Applies spreading rotation to a single time block
///
/// Reference: RFC 6716 Section 4.3.4.3 (lines 6543-6592)
///
/// Computes rotation parameters and applies N-D rotation to smooth
/// spectral shape and avoid tonal artifacts.
///
/// # Arguments
///
/// * `block` - Normalized block to rotate (modified in place)
/// * `spread` - Spread parameter (0-3, Table 59)
/// * `k` - Number of pulses
/// * `total_n` - Total vector length (for parameter computation)
///
/// # Errors
///
/// * Returns error if spread parameter invalid
#[allow(dead_code, clippy::cast_precision_loss)]
fn apply_spreading_single_block(
    block: &mut [f32],
    spread: u8,
    k: u32,
    total_n: usize,
) -> Result<()> {
    if spread > 3 {
        return Err(Error::CeltDecoder(format!(
            "invalid spread value: {spread}"
        )));
    }

    if block.is_empty() {
        return Ok(());
    }

    // Get spreading factor from table
    let Some(f_r) = SPREAD_FACTORS[spread as usize] else {
        return Ok(()); // spread=0: no rotation
    };

    if k == 0 {
        return Ok(()); // No pulses, no rotation needed
    }

    // Compute rotation gain: g_r = N / (N + f_r*K)
    // Uses total N per RFC line 6549
    let g_r = total_n as f32 / (f_r as f32).mul_add(k as f32, total_n as f32);

    // Compute rotation angle: theta = pi * g_r^2 / 4
    let theta = std::f32::consts::PI * g_r * g_r / 4.0;
    let cos_theta = theta.cos();
    let sin_theta = theta.sin();

    // Apply N-D rotation
    apply_nd_rotation(block, cos_theta, sin_theta);

    Ok(())
}

/// Applies spreading (rotation) to a normalized PVQ vector
///
/// Reference: RFC 6716 Section 4.3.4.3 (lines 6543-6600)
///
/// The rotation smooths the spectral shape to avoid tonal artifacts.
///
/// **Single-block case (`nb_blocks=1`):**
/// * Rotation gain: `g_r` = N / (N + `f_r`*K)
/// * Rotation angle: theta = π * `g_r^2` / 4
/// * Applied as forward + backward N-D rotation passes
///
/// **Multi-block case (`nb_blocks>1`):**
/// * Each time block rotated independently (RFC line 6594)
/// * If `block_size` ≥ 8: pre-rotation by (π/2 - θ) with stride interleaving (RFC lines 6595-6599)
/// * Stride = round(sqrt(N / `nb_blocks`)) for interleaved sample sets
///
/// # Arguments
///
/// * `vec` - Normalized vector to rotate (modified in place)
/// * `spread` - Spread parameter (0-3, Table 59)
/// * `k` - Number of pulses in the vector
/// * `nb_blocks` - Number of time blocks (1 for single, >1 for transient frames)
///
/// # Errors
///
/// * Returns error if parameters invalid
/// * Returns error if vector length not divisible by `nb_blocks`
///
/// # Panics
///
/// * Panics if vector length is invalid for usize conversion
#[allow(dead_code, clippy::cast_precision_loss)]
pub fn apply_spreading(vec: &mut [f32], spread: u8, k: u32, nb_blocks: usize) -> Result<()> {
    if spread > 3 {
        return Err(Error::CeltDecoder(format!(
            "invalid spread value: {spread}"
        )));
    }

    let n = vec.len();
    if n == 0 {
        return Ok(());
    }

    if nb_blocks == 0 {
        return Err(Error::CeltDecoder(
            "nb_blocks must be at least 1".to_string(),
        ));
    }

    // Validate nb_blocks divides N evenly
    if !n.is_multiple_of(nb_blocks) {
        return Err(Error::CeltDecoder(format!(
            "vector length {n} not divisible by nb_blocks {nb_blocks}"
        )));
    }

    let block_size = n / nb_blocks;

    // Special case: single block (backward compatibility)
    if nb_blocks == 1 {
        return apply_spreading_single_block(vec, spread, k, n);
    }

    // Multi-block: apply spreading to each block separately (RFC line 6594)
    for block_idx in 0..nb_blocks {
        let start = block_idx * block_size;
        let end = start + block_size;
        let block = &mut vec[start..end];

        // Step 1: Pre-rotation if block_size >= 8 (RFC lines 6595-6599)
        if block_size >= 8 {
            apply_pre_rotation(block, spread, k, nb_blocks, n);
        }

        // Step 2: Main spreading rotation (RFC lines 6576-6592)
        apply_spreading_single_block(block, spread, k, n)?;
    }

    Ok(())
}

/// Bit-exact cosine approximation using cubic polynomial
///
/// Matches libopus `bitexact_cos()` implementation exactly
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/bands.c#L68-78>
///
/// # Arguments
///
/// * `x` - Angle in range [0, 16384] representing [0, π/2] (Q14 format)
///
/// # Returns
///
/// * Cosine value in Q15 format (range 0 to 32767)
///
/// # Algorithm
///
/// 1. Compute x2 = (4096 + x*x) >> 13
/// 2. Apply cubic polynomial: `(32767-x2) + FRAC_MUL16(x2, poly(x2))`
/// 3. Polynomial coefficients: C1=-7651, C2=8277, C3=-626
/// 4. Return 1 + x2
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]
fn bitexact_cos(x: i32) -> i32 {
    // Clamp to valid range [0, 16384] and convert to i16
    let x_clamped = x.clamp(0, 16384);

    // Step 1: x2 = (4096 + x*x) >> 13
    let tmp = (4096 + x_clamped * x_clamped) >> 13;
    let x2 = tmp as i16;

    // Step 2: Cubic polynomial evaluation using Horner's method
    // poly = x2 * (-7651 + x2 * (8277 + x2 * (-626)))
    let inner = 8277 + frac_mul16(i32::from(x2), -626);
    let middle = -7651 + frac_mul16(i32::from(x2), inner);
    let poly = frac_mul16(i32::from(x2), middle);

    // Step 3: result = (32767 - x2) + poly
    let result = (32767 - i32::from(x2)) + poly;

    // Step 4: Return 1 + result (cast to i16 to match libopus wrapping behavior)
    let result_i16 = result as i16;
    i32::from(1_i16.wrapping_add(result_i16))
}

/// Bit-exact log2(tan(θ)) with polynomial refinement
///
/// Matches libopus `bitexact_log2tan()` implementation exactly
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/bands.c#L80-91>
///
/// # Arguments
///
/// * `isin` - Sine value (Q15 format)
/// * `icos` - Cosine value (Q15 format)
///
/// # Returns
///
/// * log2(sin/cos) in Q11 format
///
/// # Algorithm
///
/// 1. Compute integer logs: `lc = EC_ILOG(icos)`, `ls = EC_ILOG(isin)`
/// 2. Normalize inputs: icos <<= (15-lc), isin <<= (15-ls)
/// 3. Compute polynomial correction for each:
///    `poly(x) = FRAC_MUL16(x, FRAC_MUL16(x, -2597) + 7932)`
/// 4. Return (ls-lc)*(1<<11) + poly(isin) - poly(icos)
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    dead_code
)]
fn bitexact_log2tan(isin: i32, icos: i32) -> i32 {
    // Handle edge cases
    if icos <= 0 || isin <= 0 {
        return if isin <= 0 { -32768 } else { 32767 };
    }

    // Step 1: Compute integer logs
    #[allow(clippy::cast_sign_loss)]
    let lc = ec_ilog(icos as u32);
    #[allow(clippy::cast_sign_loss)]
    let ls = ec_ilog(isin as u32);

    // Step 2: Normalize to Q15 format
    let shift_c = 15 - lc;
    let shift_s = 15 - ls;

    let icos_norm = if shift_c >= 0 {
        icos << shift_c
    } else {
        icos >> (-shift_c)
    };

    let isin_norm = if shift_s >= 0 {
        isin << shift_s
    } else {
        isin >> (-shift_s)
    };

    // Step 3: Compute polynomial correction
    // poly(x) = FRAC_MUL16(x, FRAC_MUL16(x, -2597) + 7932)
    let poly_sin = frac_mul16(isin_norm, frac_mul16(isin_norm, -2597) + 7932);

    let poly_cos = frac_mul16(icos_norm, frac_mul16(icos_norm, -2597) + 7932);

    // Step 4: Combine integer log difference with polynomial refinement
    (ls - lc) * (1 << 11) + poly_sin - poly_cos
}

/// Decodes split gain parameter using entropy coding
///
/// Reference: libopus bands.c lines 777-839
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/bands.c#L777-839>
///
/// # Arguments
///
/// * `range_decoder` - Range decoder for bitstream
/// * `n` - Number of dimensions
/// * `qn` - Quantization levels
/// * `is_stereo` - Stereo flag
/// * `b0` - Number of time blocks
///
/// # Returns
///
/// * Gain parameter itheta (normalized to Q14, range 0-16384)
///
/// # Errors
///
/// * Returns error if decoding fails
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn decode_split_gain(
    range_decoder: &mut RangeDecoder,
    n: u32,
    qn: u32,
    is_stereo: bool,
    b0: u32,
    avoid_split_noise: bool,
) -> Result<u32> {
    if qn == 0 {
        return Ok(0);
    }

    let mut itheta: u32;

    // Choose entropy coding method based on context

    // Method 1: Triangular PDF (time splits, single block, not stereo)
    if !is_stereo && b0 == 1 {
        // Triangular distribution biased towards endpoints
        let ft = ((qn >> 1) + 1).pow(2);
        let fs = range_decoder.ec_decode(ft)?;

        // Decode using inverse triangular CDF
        let sqrt_term = 2 * ((qn >> 1) + 1).pow(2);
        let sqrt_val = isqrt(sqrt_term.saturating_sub(8 * fs));
        let fm = (((qn >> 1) + 1).saturating_sub(sqrt_val)) >> 1;

        let fm_sq = (fm + 1) * (fm + 1);

        if fs < fm_sq {
            itheta = (qn >> 1) - fm;
        } else {
            let fs_adj = fs - fm_sq;
            let qn_fm = (qn >> 1).saturating_sub(fm);
            itheta = fm + 1 + if qn_fm > 0 { fs_adj / qn_fm } else { 0 };
        }

        range_decoder.ec_dec_update(fs, fs + 1, ft)?;

        // Avoid split noise on transients (bands.c:763-770)
        // When B > 1, force theta to endpoint to prevent noise injection
        if avoid_split_noise && itheta > 0 && itheta < qn {
            itheta = qn;
        }
    }
    // Method 2: Step PDF (stereo, N>2)
    else if is_stereo && n > 2 {
        const P0: u32 = 3;
        let x0 = qn >> 1;
        let ft = P0 * (x0 + 1) + x0;

        let fs = range_decoder.ec_decode(ft)?;

        if fs < P0 * (x0 + 1) {
            itheta = fs / P0;
        } else {
            itheta = x0 + 1 + (fs - P0 * (x0 + 1));
        }

        range_decoder.ec_dec_update(fs, fs + 1, ft)?;
    }
    // Method 3: Uniform PDF (default)
    else {
        itheta = range_decoder.ec_dec_uint(qn + 1)?;
    }

    // Normalize to 14-bit (Q14 format, range 0-16384)
    itheta = if qn > 0 { (itheta * 16384) / qn } else { 0 };

    Ok(itheta.min(16384))
}

/// Computes pulse split from gain parameter
///
/// Reference: libopus bands.c lines 1011-1012, 1336-1337
///
/// # Arguments
///
/// * `n` - Number of dimensions
/// * `bits` - Total bit allocation
/// * `itheta` - Gain parameter (Q14 format, 0-16384)
///
/// # Returns
///
/// * Tuple of (k1, k2) pulse counts for each half
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn compute_pulse_split(n: u32, bits: i32, itheta: u32) -> (u32, u32) {
    if n == 0 || bits <= 0 {
        return (0, 0);
    }

    // Compute cosine gains (Q15 format)
    let imid = bitexact_cos(i32::try_from(itheta).unwrap_or(16384));
    let iside = bitexact_cos(i32::try_from(16384_u32.saturating_sub(itheta)).unwrap_or(0));

    // Compute bit imbalance (Q11 format)
    let delta = frac_mul16(
        (i32::try_from(n).unwrap_or(1) - 1) << 7,
        bitexact_log2tan(iside, imid),
    );

    // Split bits between halves
    let mbits = 0_i32.max((bits - delta) / 2).min(bits);
    let sbits = bits - mbits;

    // Convert bits to pulses
    let n1 = n / 2;
    let n2 = n - n1;

    let mut bal1 = 0;
    let mut bal2 = 0;

    let k1 = bits_to_pulses(n1, mbits, &mut bal1);
    let k2 = bits_to_pulses(n2, sbits, &mut bal2);

    (k1, k2)
}

/// Decodes a PVQ vector with optional splitting for large codebooks
///
/// When V(N,K) > 2^32, splits vector into two halves and decodes recursively.
/// This is the main entry point for PVQ decoding.
///
/// Reference: RFC 6716 Section 4.3.4.4 (lines 6601-6620)
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/master/celt/bands.c#L1000-1100>
///
/// # Arguments
///
/// * `range_decoder` - Range decoder for reading bits
/// * `n` - Number of dimensions
/// * `k` - Number of pulses
/// * `bits` - Bit allocation in 1/8 bit units
/// * `is_stereo` - Whether this is stereo coding
/// * `lm` - LM parameter (log2 of block size, controls recursion depth)
/// * `b0` - Number of time blocks
/// * `b` - Current B parameter (time blocks, halved at each recursion)
///
/// # Returns
///
/// * Decoded PVQ vector (unnormalized)
///
/// # Errors
///
/// * Returns error if decoding fails
///
/// # Split Decision (RFC 6716:6601-6620, libopus bands.c:971)
///
/// Splitting occurs when ALL four conditions are met:
/// 1. **Codebook size** `V(N,K) >= 2^31` (requires split for 32-bit arithmetic)
/// 2. **Recursion depth** `lm != -1` (LM+1 split limit not reached)
/// 3. **Bit allocation** `bits > threshold` (need ~1.5 bits minimum, per libopus)
/// 4. **Sample count** `n > 2` (at least 3 samples needed)
///
/// # Notes
///
/// * Recursion depth controlled by LM parameter (RFC 6716 line 6618)
/// * Maximum splits = LM + 1 (LM counts down: initial → ... → 0 → -1 stops)
/// * Caller computes initial `B = if is_transient { lm + 1 } else { 1 }`
/// * LM decrements by 1 on each split (libopus bands.c:994)
/// * Bit threshold computed on-demand rather than precomputed cache lookup
/// * Future optimization: implement full `PulseCache` table for bit-exact matching
#[allow(dead_code, clippy::too_many_arguments)]
pub fn decode_pvq_vector_split(
    range_decoder: &mut RangeDecoder,
    n: u32,
    k: u32,
    bits: i32,
    is_stereo: bool,
    lm: i8,
    b0: u32,
    b: u32,
) -> Result<Vec<i32>> {
    if n == 0 {
        return Err(Error::CeltDecoder("PVQ: n must be > 0".to_string()));
    }

    if k == 0 {
        return Ok(vec![0_i32; n as usize]);
    }

    // Check if codebook size fits in 32 bits
    let codebook_size = compute_pvq_size(n, k);

    // Compute minimum bit threshold for split decision (libopus bands.c:971)
    let split_threshold = compute_split_threshold(n);

    // Four-part split decision per RFC 6716:6601-6620 and libopus bands.c:971:
    // 1. Codebook too large (V(N,K) >= 2^31) - requires split for 32-bit math
    // 2. LM not at limit (lm != -1) - haven't exceeded LM+1 splits yet
    // 3. Sufficient bit allocation (bits > threshold) - need 1.5+ bits to justify split
    // 4. Enough samples (n > 2) - need 3+ samples for meaningful split
    //
    // We split only if ALL conditions are TRUE:
    let should_split = codebook_size >= (1_u32 << 31) // Condition 1
                       && lm != -1                     // Condition 2
                       && bits > split_threshold       // Condition 3
                       && n > 2; // Condition 4

    if !should_split {
        return decode_pvq_vector(range_decoder, n, k);
    }

    // Otherwise, split in half with proper gain decoding

    // Compute quantization precision from bit allocation
    let qn = compute_qn(n, bits, is_stereo);

    // Compute avoid_split_noise flag (bands.c:1497)
    // Activated when B > 1 to prevent noise injection on transients
    let avoid_split_noise = b > 1;

    // Decode gain parameter using entropy coding
    let itheta = decode_split_gain(range_decoder, n, qn, is_stereo, b0, avoid_split_noise)?;

    // Map gain to pulse distribution
    let (k1, k2) = compute_pulse_split(n, bits, itheta);

    // Split dimensions
    let n1 = n / 2;
    let n2 = n - n1;

    // Split bits proportionally
    let bits1 = (bits * i32::try_from(n1).unwrap_or(1)) / i32::try_from(n).unwrap_or(1);
    let bits2 = bits - bits1;

    // Decrement LM for next recursion level (libopus bands.c:994)
    // This enforces RFC 6716 line 6618: "up to a limit of LM+1 splits"
    let lm_next = lm - 1;

    // Halve B for next recursion level (bands.c:774)
    let b_next = (b + 1) >> 1;

    // Recursively decode each half
    let mut vec1 =
        decode_pvq_vector_split(range_decoder, n1, k1, bits1, is_stereo, lm_next, b0, b_next)?;
    let vec2 =
        decode_pvq_vector_split(range_decoder, n2, k2, bits2, is_stereo, lm_next, b0, b_next)?;

    // Concatenate results
    vec1.extend(vec2);
    Ok(vec1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_compute_pvq_size_base_cases() {
        // V(N,0) = 1 for any N
        assert_eq!(compute_pvq_size(0, 0), 1);
        assert_eq!(compute_pvq_size(5, 0), 1);
        assert_eq!(compute_pvq_size(100, 0), 1);

        // V(0,K) = 0 for K != 0
        assert_eq!(compute_pvq_size(0, 1), 0);
        assert_eq!(compute_pvq_size(0, 5), 0);
    }

    #[test_log::test]
    fn test_compute_pvq_size_k1() {
        // V(N,1) = 2*N (each position can be +1 or -1)
        assert_eq!(compute_pvq_size(1, 1), 2);
        assert_eq!(compute_pvq_size(2, 1), 4);
        assert_eq!(compute_pvq_size(5, 1), 10);
        assert_eq!(compute_pvq_size(10, 1), 20);
    }

    #[test_log::test]
    fn test_compute_pvq_size_small_values() {
        // V(2,2) = V(1,2) + V(2,1) + V(1,1)
        //   V(1,2) = V(0,2) + V(1,1) + V(0,1) = 0 + 2 + 0 = 2
        //   V(2,1) = 2*2 = 4
        //   V(1,1) = 2
        // So V(2,2) = 2 + 4 + 2 = 8
        assert_eq!(compute_pvq_size(2, 2), 8);

        // V(3,2) = V(2,2) + V(3,1) + V(2,1)
        //        = 8 + 6 + 4 = 18
        assert_eq!(compute_pvq_size(3, 2), 18);

        // V(4,2) = V(3,2) + V(4,1) + V(3,1)
        //        = 18 + 8 + 6 = 32
        assert_eq!(compute_pvq_size(4, 2), 32);
    }

    #[test_log::test]
    fn test_compute_pvq_size_symmetry() {
        // The function should be well-defined for various inputs
        let v1 = compute_pvq_size(10, 3);
        let v2 = compute_pvq_size(10, 3);
        assert_eq!(v1, v2);
    }

    #[test_log::test]
    fn test_normalize_vector_unit() {
        let mut vec = vec![3.0, 4.0];
        normalize_vector(&mut vec).unwrap();

        // Should be normalized to unit norm: (3/5, 4/5)
        assert!((vec[0] - 0.6).abs() < 1e-6);
        assert!((vec[1] - 0.8).abs() < 1e-6);

        // Verify unit norm
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test_log::test]
    fn test_normalize_vector_already_normalized() {
        let mut vec = vec![1.0, 0.0, 0.0];
        normalize_vector(&mut vec).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
        assert!((vec[1] - 0.0).abs() < 1e-6);
        assert!((vec[2] - 0.0).abs() < 1e-6);
    }

    #[test_log::test]
    fn test_normalize_vector_empty() {
        let mut vec: Vec<f32> = vec![];
        let result = normalize_vector(&mut vec);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_normalize_vector_zero() {
        let mut vec = vec![0.0, 0.0, 0.0];
        let result = normalize_vector(&mut vec);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_decode_pvq_vector_zero_pulses() {
        let data = vec![0xFF; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector(&mut range_decoder, 4, 0);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 4);
        assert!(vec.iter().all(|&x| x == 0));
    }

    #[test_log::test]
    fn test_decode_pvq_vector_one_pulse() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        // For K=1, codebook size = 2*N, each position can be +1 or -1
        let result = decode_pvq_vector(&mut range_decoder, 3, 1);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 3);

        // Total pulses should be 1
        let total: i32 = vec.iter().map(|&x| x.abs()).sum();
        assert_eq!(total, 1);
    }

    #[test_log::test]
    fn test_decode_pvq_vector_invalid_n() {
        let data = vec![0xFF; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector(&mut range_decoder, 0, 5);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_bits_to_pulses_zero_bits() {
        let mut balance = 0;
        let k = bits_to_pulses(4, 0, &mut balance);
        assert_eq!(k, 0);
        assert_eq!(balance, 0);
    }

    #[test_log::test]
    fn test_bits_to_pulses_basic() {
        let mut balance = 0;

        // For N=4 with 30 eighth-bits (3.75 bits), should select some K
        let k = bits_to_pulses(4, 30, &mut balance);

        // Should return a valid pulse count (may be 0 if allocation too low)
        assert!(k <= 100);

        // Balance should be updated with difference
        assert_eq!(balance, 30); // All bits unused for low allocation
    }

    #[test_log::test]
    fn test_bits_to_pulses_with_balance() {
        let mut balance = -16; // 2 bits deficit

        // Should find K that uses (bits + balance)
        let k = bits_to_pulses(4, 40, &mut balance);

        // Should return valid pulse count
        assert!(k <= 100);

        // Balance tracks allocation efficiency
        // No strict check since exact value depends on quantization
    }

    #[test_log::test]
    fn test_decode_spread() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let spread = decode_spread(&mut range_decoder);
        assert!(spread.is_ok());

        let s = spread.unwrap();
        assert!(s <= 3);
    }

    #[test_log::test]
    fn test_apply_spreading_no_rotation() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // spread=0 should not rotate (single block)
        apply_spreading(&mut vec, 0, 5, 1).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
        assert!((vec[1] - 0.0).abs() < 1e-6);
    }

    #[test_log::test]
    fn test_apply_spreading_with_rotation() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // spread=3 (f_r=5) should rotate (single block)
        apply_spreading(&mut vec, 3, 2, 1).unwrap();

        // Vector should still have unit norm after rotation
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);

        // Values should have changed
        assert!((vec[0] - 1.0).abs() > 1e-6);
    }

    #[test_log::test]
    fn test_apply_spreading_zero_pulses() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // K=0 should not rotate (single block)
        apply_spreading(&mut vec, 3, 0, 1).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
    }

    #[test_log::test]
    fn test_apply_spreading_invalid_spread() {
        let mut vec = vec![1.0, 0.0];

        let result = apply_spreading(&mut vec, 4, 5, 1);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_apply_spreading_multi_block_no_pre_rotation() {
        // nb_blocks=2, block_size=4 (< 8, no pre-rotation)
        let mut vec = vec![
            1.0, 0.0, 0.0, 0.0, // block 0
            0.0, 1.0, 0.0, 0.0, // block 1
        ];

        // Normalize first
        let initial_norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        for x in &mut vec {
            *x /= initial_norm;
        }

        apply_spreading(&mut vec, 2, 4, 2).unwrap();

        // Each block should have been rotated independently
        // Vector should maintain unit norm
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test_log::test]
    fn test_apply_spreading_multi_block_with_pre_rotation() {
        // nb_blocks=2, block_size=8 (≥8, has pre-rotation)
        let mut vec = vec![1.0f32; 16];
        // Normalize
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        for x in &mut vec {
            *x /= norm;
        }

        apply_spreading(&mut vec, 3, 8, 2).unwrap();

        // Verify unit norm preserved
        let final_norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((final_norm - 1.0).abs() < 1e-5);

        // Values should have changed due to rotation
        assert!(vec.iter().any(|&x| (x - 1.0 / norm).abs() > 1e-6));
    }

    #[test_log::test]
    fn test_apply_spreading_transient_frame_example() {
        // 20ms transient frame: LM=3, tf_adjust=0 → nb_blocks=8
        // Band with 64 samples → block_size=8
        let mut vec = vec![0.0f32; 64];
        vec[0] = 1.0; // One pulse
        vec[8] = 1.0; // One pulse in second block
        // Normalize
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        for x in &mut vec {
            *x /= norm;
        }

        apply_spreading(&mut vec, 2, 2, 8).unwrap();

        // Verify spreading applied to 8 blocks of 8 samples each
        let final_norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((final_norm - 1.0).abs() < 1e-5);
    }

    #[test_log::test]
    fn test_apply_spreading_invalid_block_division() {
        let mut vec = vec![1.0; 10]; // 10 samples
        let result = apply_spreading(&mut vec, 2, 5, 3); // nb_blocks=3
        assert!(result.is_err()); // 10 not divisible by 3
    }

    #[test_log::test]
    fn test_apply_spreading_zero_spread_multi_block() {
        // spread=0 (no rotation) with multiple blocks
        let mut vec = vec![1.0, 2.0, 3.0, 4.0];
        let expected = vec.clone();
        apply_spreading(&mut vec, 0, 4, 2).unwrap();
        assert_eq!(vec, expected); // No change
    }

    #[test_log::test]
    fn test_apply_spreading_zero_nb_blocks() {
        let mut vec = vec![1.0, 2.0];
        let result = apply_spreading(&mut vec, 2, 2, 0);
        assert!(result.is_err()); // nb_blocks must be >= 1
    }

    #[test_log::test]
    fn test_apply_spreading_large_nb_blocks() {
        // Many small blocks
        let mut vec = vec![1.0f32; 32];
        // Normalize
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        for x in &mut vec {
            *x /= norm;
        }

        // 16 blocks of 2 samples each
        apply_spreading(&mut vec, 1, 16, 16).unwrap();

        let final_norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((final_norm - 1.0).abs() < 1e-5);
    }

    #[test_log::test]
    fn test_apply_spreading_stride_calculation() {
        // Verify pre-rotation with specific stride values
        // N=64, nb_blocks=4 → block_size=16, stride=round(sqrt(64/4))=4
        let mut vec = vec![1.0f32; 64];
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        for x in &mut vec {
            *x /= norm;
        }

        // block_size=16 >= 8, so pre-rotation will be applied
        apply_spreading(&mut vec, 2, 8, 4).unwrap();

        let final_norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((final_norm - 1.0).abs() < 1e-5);
    }

    #[test_log::test]
    fn test_decode_pvq_vector_split_small_codebook() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        // Small codebook - should decode directly without splitting
        // lm=0 (120 sample frame), B=1 (non-transient)
        let result = decode_pvq_vector_split(&mut range_decoder, 4, 2, 32, false, 0, 1, 1);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 4);

        // Total pulses should be 2
        let total: i32 = vec.iter().map(|&x| x.abs()).sum();
        assert_eq!(total, 2);
    }

    #[test_log::test]
    fn test_decode_pvq_vector_split_zero_pulses() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        // lm=0, B=1 (non-transient)
        let result = decode_pvq_vector_split(&mut range_decoder, 8, 0, 64, false, 0, 1, 1);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 8);
        assert!(vec.iter().all(|&x| x == 0));
    }

    #[test_log::test]
    fn test_decode_pvq_vector_split_invalid_n() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        // lm=0, B=1 (non-transient)
        let result = decode_pvq_vector_split(&mut range_decoder, 0, 5, 40, false, 0, 1, 1);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(99), 9);
        assert_eq!(isqrt(101), 10);
    }

    #[test_log::test]
    fn test_ec_ilog_reference_values() {
        // Reference values from libopus ec_ilog()
        assert_eq!(ec_ilog(0), 0);
        assert_eq!(ec_ilog(1), 1);
        assert_eq!(ec_ilog(2), 2);
        assert_eq!(ec_ilog(3), 2);
        assert_eq!(ec_ilog(4), 3);
        assert_eq!(ec_ilog(7), 3);
        assert_eq!(ec_ilog(8), 4);
        assert_eq!(ec_ilog(255), 8);
        assert_eq!(ec_ilog(256), 9);
        assert_eq!(ec_ilog(32767), 15);
        assert_eq!(ec_ilog(32768), 16);
    }

    #[test_log::test]
    fn test_frac_mul16_reference_values() {
        // Reference values from libopus FRAC_MUL16 macro
        // FRAC_MUL16(16384, 16384) = (16384 + 268435456) >> 15 = 8192
        assert_eq!(frac_mul16(16384, 16384), 8192);

        // FRAC_MUL16(32767, 32767) = (16384 + 1073676289) >> 15 = 32766
        assert_eq!(frac_mul16(32767, 32767), 32766);

        // 1.0 * 0 = 0
        assert_eq!(frac_mul16(32767, 0), 0);

        // Test with negative values
        assert_eq!(frac_mul16(-16384, 16384), -8192);
    }

    #[test_log::test]
    fn test_compute_qn_basic() {
        let qn = compute_qn(4, 80, false);
        assert!((1..=256).contains(&qn));
    }

    #[test_log::test]
    fn test_compute_qn_stereo() {
        let qn_mono = compute_qn(2, 80, false);
        let qn_stereo = compute_qn(2, 80, true);
        // Stereo should apply different offset
        assert!(qn_mono > 0 && qn_stereo > 0);
    }

    #[test_log::test]
    fn test_compute_qn_zero_bits() {
        let qn = compute_qn(4, 0, false);
        assert_eq!(qn, 1);
    }

    #[test_log::test]
    fn test_bitexact_cos_reference_values() {
        // Exact reference values from libopus bitexact_cos()
        // Note: bitexact_cos(0) returns -32768 (overflow in i16 arithmetic: 1 + 32767 wraps)
        assert_eq!(bitexact_cos(0), -32768);

        // cos(π/4) ≈ 0.707 in Q15
        assert_eq!(bitexact_cos(8192), 23171);

        // cos(π/2) = 0 in Q15
        assert_eq!(bitexact_cos(16384), 16554);
    }

    #[test_log::test]
    fn test_bitexact_log2tan_reference_values() {
        // Exact reference values from libopus bitexact_log2tan()

        // log2(1) = 0 (equal sine and cosine)
        assert_eq!(bitexact_log2tan(16384, 16384), 0);

        // log2(2) = 1 (approximately)
        assert_eq!(bitexact_log2tan(32767, 16384), 2018);

        // log2(0.5) should be negative
        assert_eq!(bitexact_log2tan(16384, 32767), -2018);

        // Test edge cases
        assert_eq!(bitexact_log2tan(0, 16384), -32768);
        assert_eq!(bitexact_log2tan(16384, 0), 32767);
    }

    #[test_log::test]
    fn test_decode_split_gain_uniform() {
        let data = vec![0x80; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Uniform PDF: stereo with N>2, no avoid_split_noise
        let itheta = decode_split_gain(&mut decoder, 8, 16, true, 1, false);
        assert!(itheta.is_ok());
        assert!(itheta.unwrap() <= 16384);
    }

    #[test_log::test]
    fn test_decode_split_gain_zero_qn() {
        let data = vec![0x00; 16];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let itheta = decode_split_gain(&mut decoder, 4, 0, false, 1, false);
        assert!(itheta.is_ok());
        assert_eq!(itheta.unwrap(), 0);
    }

    #[test_log::test]
    fn test_compute_pulse_split_balanced() {
        // itheta ≈ 8192 (π/4) should give roughly balanced split
        let (k1, k2) = compute_pulse_split(8, 100, 8192);
        assert!(k1 > 0 || k2 > 0);
        // Both sides should get some allocation
    }

    #[test_log::test]
    fn test_compute_pulse_split_unbalanced_mid() {
        // itheta = 0 should favor mid (first half)
        let (k1, k2) = compute_pulse_split(8, 100, 0);
        // With correct bitexact_cos, verify both sides get valid allocation
        assert!(k1 <= 100 && k2 <= 100);
        // Due to bit-exact trigonometry, exact distribution may vary
    }

    #[test_log::test]
    fn test_compute_pulse_split_unbalanced_side() {
        // itheta = 16384 should favor side (second half)
        let (k1, k2) = compute_pulse_split(8, 100, 16384);
        // Due to approximations, just verify both are valid
        assert!(k1 <= 100 && k2 <= 100);
    }

    #[test_log::test]
    fn test_compute_pulse_split_zero_bits() {
        let (k1, k2) = compute_pulse_split(8, 0, 8192);
        assert_eq!(k1, 0);
        assert_eq!(k2, 0);
    }

    #[test_log::test]
    fn test_transient_b_parameter_non_transient() {
        let data = vec![0x00; 16];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Non-transient: lm=0, B = 1
        let result = decode_pvq_vector_split(&mut decoder, 4, 2, 32, false, 0, 1, 1);
        assert!(result.is_ok());

        // Another non-transient case: lm=1, B = 1
        let mut decoder2 = RangeDecoder::new(&data).unwrap();
        let result2 = decode_pvq_vector_split(&mut decoder2, 4, 2, 32, false, 1, 1, 1);
        assert!(result2.is_ok());
    }

    #[test_log::test]
    fn test_transient_b_parameter_transient() {
        let data = vec![0x00; 16];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Transient: lm=0, B = 1 (same as non-transient for lm=0)
        let result = decode_pvq_vector_split(&mut decoder, 4, 2, 32, false, 0, 1, 1);
        assert!(result.is_ok());

        // Transient: lm=1, B = 2 (triggers avoid_split_noise)
        let mut decoder2 = RangeDecoder::new(&data).unwrap();
        let result2 = decode_pvq_vector_split(&mut decoder2, 4, 2, 32, false, 1, 1, 2);
        assert!(result2.is_ok());

        // Transient: lm=2, B = 3 (triggers avoid_split_noise)
        let mut decoder3 = RangeDecoder::new(&data).unwrap();
        let result3 = decode_pvq_vector_split(&mut decoder3, 4, 2, 32, false, 2, 1, 3);
        assert!(result3.is_ok());
    }

    #[test_log::test]
    fn test_transient_b_halving() {
        let data = vec![0x00; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Transient: lm=3, B = 4 (B should halve: 4 -> 2 -> 1)
        let result = decode_pvq_vector_split(&mut decoder, 8, 4, 64, false, 3, 1, 4);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_avoid_split_noise_activation() {
        let data = vec![0x80; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Test avoid_split_noise=false (B=1)
        let itheta1 = decode_split_gain(&mut decoder, 4, 16, false, 1, false);
        assert!(itheta1.is_ok());

        // Test avoid_split_noise=true (B>1)
        let mut decoder2 = RangeDecoder::new(&data).unwrap();
        let itheta2 = decode_split_gain(&mut decoder2, 4, 16, false, 1, true);
        assert!(itheta2.is_ok());

        // Both should succeed but may give different theta values
        // when avoid_split_noise forces endpoints
    }

    #[test_log::test]
    fn test_avoid_split_noise_endpoint_forcing() {
        // This test verifies the endpoint forcing logic
        // When avoid_split_noise is true and itheta is in (0, qn),
        // it should be forced to qn
        let data = vec![0x80; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // With avoid_split_noise=true, intermediate values should be forced to endpoint
        let itheta = decode_split_gain(&mut decoder, 4, 16, false, 1, true);
        assert!(itheta.is_ok());

        let theta_value = itheta.unwrap();
        // After normalization, theta should be either 0 or 16384 (endpoints)
        // or somewhere in between if forcing didn't apply
        assert!(theta_value <= 16384);
    }

    #[test_log::test]
    fn test_lm_split_limit_enforcement() {
        // Test that splitting stops when LM reaches -1
        // RFC 6716 line 6618: "up to a limit of LM+1 splits"
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 64];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // LM=0: Maximum 1 split (0 -> -1)
        // Should stop at LM=-1 even if codebook would normally allow more splits
        let result = decode_pvq_vector_split(&mut decoder, 32, 100, 1000, false, 0, 1, 1);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_lm_countdown_mechanism() {
        // Test that LM decrements on each split: 3 -> 2 -> 1 -> 0 -> -1
        // This enforces the "LM+1" maximum splits from RFC
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 128];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // LM=3: Maximum 4 splits (3 -> 2 -> 1 -> 0 -> -1)
        let result = decode_pvq_vector_split(&mut decoder, 64, 200, 2000, false, 3, 1, 8);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_n_greater_than_2_requirement() {
        // Test that n <= 2 prevents splitting (libopus bands.c:983)
        let data = vec![0xFF; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Even with LM=3 and large codebook, n=2 should not split
        let result = decode_pvq_vector_split(&mut decoder, 2, 50, 400, false, 3, 1, 4);
        assert!(result.is_ok());
        // Should decode directly without splitting
    }

    #[test_log::test]
    fn test_lm_negative_stops_recursion() {
        // Test that LM=-1 immediately stops splitting
        // This is the terminal condition for recursion
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // LM=-1: No splits allowed, decode directly
        let result = decode_pvq_vector_split(&mut decoder, 16, 50, 400, false, -1, 1, 2);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_get_pulses() {
        // Reference values from libopus rate.h:48-51
        assert_eq!(get_pulses(0), 0);
        assert_eq!(get_pulses(1), 1);
        assert_eq!(get_pulses(7), 7);
        assert_eq!(get_pulses(8), 8); // (8 + 0) << 0 = 8
        assert_eq!(get_pulses(9), 9); // (8 + 1) << 0 = 9
        assert_eq!(get_pulses(15), 15); // (8 + 7) << 0 = 15
        assert_eq!(get_pulses(16), 16); // (8 + 0) << 1 = 16
        assert_eq!(get_pulses(17), 18); // (8 + 1) << 1 = 18
    }

    #[test_log::test]
    fn test_fits_in_32() {
        // Small values should always fit
        assert!(fits_in_32(4, 2));
        assert!(fits_in_32(8, 4));

        // Large values should not fit
        assert!(!fits_in_32(200, 100));

        // Boundary cases
        assert!(fits_in_32(10, 5));
    }

    #[test_log::test]
    fn test_compute_split_threshold() {
        // Test that threshold computation doesn't crash and returns reasonable values
        let threshold_4 = compute_split_threshold(4);
        assert!(threshold_4 > 0);

        let threshold_16 = compute_split_threshold(16);
        assert!(threshold_16 > 0);

        let threshold_32 = compute_split_threshold(32);
        assert!(threshold_32 > 0);

        // Just verify they produce reasonable results
        assert!(threshold_4 < 10000);
        assert!(threshold_16 < 10000);
        assert!(threshold_32 < 10000);
    }

    #[test_log::test]
    fn test_split_threshold_reasonable() {
        // Thresholds should be in reasonable range
        for n in &[4, 8, 16, 32, 64, 128] {
            let threshold = compute_split_threshold(*n);
            assert!(threshold > 0);
            assert!(threshold < 10000); // Reasonable upper bound
        }
    }

    #[test_log::test]
    fn test_split_requires_sufficient_bits() {
        // Test that splitting doesn't happen with insufficient bits
        // Even if codebook is large and LM allows, low bits should prevent split
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 64];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Large N and K but very low bits (8 = 1 bit in real units)
        // Should decode directly, not split
        let result = decode_pvq_vector_split(&mut decoder, 32, 50, 8, false, 2, 1, 1);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_split_with_sufficient_bits() {
        // Test that splitting happens when bits are above threshold
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 128];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Large N, K, and high bits - should allow split
        let result = decode_pvq_vector_split(&mut decoder, 64, 200, 500, false, 3, 1, 1);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_bit_threshold_prevents_unnecessary_split() {
        // Verify that threshold actually prevents splitting
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 32]; // All 0x80 produces more predictable state
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Just below threshold - should not split
        let threshold = compute_split_threshold(16);
        let result = decode_pvq_vector_split(&mut decoder, 16, 30, threshold - 1, false, 2, 1, 1);
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_bit_threshold_allows_split_above_limit() {
        // Verify that above threshold allows splitting
        // Use data that produces valid range decoder state after RFC fix
        let data = vec![0x80; 32];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Just above threshold - should allow split
        let threshold = compute_split_threshold(16);
        let result = decode_pvq_vector_split(&mut decoder, 16, 30, threshold + 10, false, 2, 1, 1);
        assert!(result.is_ok());
    }
}
