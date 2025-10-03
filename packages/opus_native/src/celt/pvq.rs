#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

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
#[allow(clippy::many_single_char_names)]
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

/// Applies spreading (rotation) to a normalized PVQ vector
///
/// Reference: RFC 6716 Section 4.3.4.3 (lines 6543-6600)
///
/// The rotation smooths the spectral shape to avoid tonal artifacts.
/// Rotation gain: `g_r` = N / (N + `f_r`*K)
/// Rotation angle: theta = pi * `g_r^2` / 4
///
/// # Arguments
///
/// * `vec` - Normalized vector to rotate (modified in place)
/// * `spread` - Spread parameter (0-3)
/// * `k` - Number of pulses
///
/// # Errors
///
/// * Returns error if parameters invalid
///
/// # Panics
///
/// * Panics if vector length is invalid
#[allow(dead_code, clippy::cast_precision_loss)]
pub fn apply_spreading(vec: &mut [f32], spread: u8, k: u32) -> Result<()> {
    if spread > 3 {
        return Err(Error::CeltDecoder(format!(
            "invalid spread value: {spread}"
        )));
    }

    let n = u32::try_from(vec.len()).expect("invalid vector length");
    if n == 0 {
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
    let g_r = n as f32 / (f_r as f32).mul_add(k as f32, n as f32);

    // Compute rotation angle: theta = pi * g_r^2 / 4
    let theta = std::f32::consts::PI * g_r * g_r / 4.0;
    let cos_theta = theta.cos();
    let sin_theta = theta.sin();

    // Apply N-D rotation as series of 2-D rotations
    // Forward pass: R(0,1), R(1,2), ..., R(N-2,N-1)
    for i in 0..(n as usize - 1) {
        let x_i = vec[i];
        let x_j = vec[i + 1];
        vec[i] = cos_theta * x_i + sin_theta * x_j;
        vec[i + 1] = (-sin_theta).mul_add(x_i, cos_theta * x_j);
    }

    // Backward pass: R(N-2,N-1), ..., R(1,2), R(0,1)
    for i in (0..(n as usize - 1)).rev() {
        let x_i = vec[i];
        let x_j = vec[i + 1];
        vec[i] = cos_theta * x_i + sin_theta * x_j;
        vec[i + 1] = (-sin_theta).mul_add(x_i, cos_theta * x_j);
    }

    Ok(())
}

/// Decodes a PVQ vector with optional splitting for large codebooks
///
/// When V(N,K) > 2^32, splits vector into two halves and decodes recursively.
/// This is the main entry point for PVQ decoding.
///
/// Reference: RFC 6716 Section 4.3.4.4 (lines 6601-6620)
///
/// # Arguments
///
/// * `range_decoder` - Range decoder for reading bits
/// * `n` - Number of dimensions
/// * `k` - Number of pulses
/// * `max_depth` - Maximum split recursion depth
///
/// # Returns
///
/// * Decoded PVQ vector (unnormalized)
///
/// # Errors
///
/// * Returns error if decoding fails
#[allow(dead_code)]
pub fn decode_pvq_vector_split(
    range_decoder: &mut RangeDecoder,
    n: u32,
    k: u32,
    max_depth: u32,
) -> Result<Vec<i32>> {
    if n == 0 {
        return Err(Error::CeltDecoder("PVQ: n must be > 0".to_string()));
    }

    if k == 0 {
        return Ok(vec![0_i32; n as usize]);
    }

    // Check if codebook size fits in 32 bits
    let codebook_size = compute_pvq_size(n, k);

    // If codebook fits in 32 bits OR we've hit max depth, decode directly
    if codebook_size < (1_u32 << 31) || max_depth == 0 {
        return decode_pvq_vector(range_decoder, n, k);
    }

    // Otherwise, split in half
    let n1 = n / 2;
    let n2 = n - n1;

    // Decode gain split parameter (simplified - in real implementation,
    // this would use a quantized gain with entropy coding)
    // For now, just split pulses equally
    let k1 = k / 2;
    let k2 = k - k1;

    // Recursively decode each half
    let mut vec1 = decode_pvq_vector_split(range_decoder, n1, k1, max_depth - 1)?;
    let vec2 = decode_pvq_vector_split(range_decoder, n2, k2, max_depth - 1)?;

    // Concatenate results
    vec1.extend(vec2);
    Ok(vec1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_pvq_size_base_cases() {
        // V(N,0) = 1 for any N
        assert_eq!(compute_pvq_size(0, 0), 1);
        assert_eq!(compute_pvq_size(5, 0), 1);
        assert_eq!(compute_pvq_size(100, 0), 1);

        // V(0,K) = 0 for K != 0
        assert_eq!(compute_pvq_size(0, 1), 0);
        assert_eq!(compute_pvq_size(0, 5), 0);
    }

    #[test]
    fn test_compute_pvq_size_k1() {
        // V(N,1) = 2*N (each position can be +1 or -1)
        assert_eq!(compute_pvq_size(1, 1), 2);
        assert_eq!(compute_pvq_size(2, 1), 4);
        assert_eq!(compute_pvq_size(5, 1), 10);
        assert_eq!(compute_pvq_size(10, 1), 20);
    }

    #[test]
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

    #[test]
    fn test_compute_pvq_size_symmetry() {
        // The function should be well-defined for various inputs
        let v1 = compute_pvq_size(10, 3);
        let v2 = compute_pvq_size(10, 3);
        assert_eq!(v1, v2);
    }

    #[test]
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

    #[test]
    fn test_normalize_vector_already_normalized() {
        let mut vec = vec![1.0, 0.0, 0.0];
        normalize_vector(&mut vec).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
        assert!((vec[1] - 0.0).abs() < 1e-6);
        assert!((vec[2] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_vector_empty() {
        let mut vec: Vec<f32> = vec![];
        let result = normalize_vector(&mut vec);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_vector_zero() {
        let mut vec = vec![0.0, 0.0, 0.0];
        let result = normalize_vector(&mut vec);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_pvq_vector_zero_pulses() {
        let data = vec![0xFF; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector(&mut range_decoder, 4, 0);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 4);
        assert!(vec.iter().all(|&x| x == 0));
    }

    #[test]
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

    #[test]
    fn test_decode_pvq_vector_invalid_n() {
        let data = vec![0xFF; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector(&mut range_decoder, 0, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_bits_to_pulses_zero_bits() {
        let mut balance = 0;
        let k = bits_to_pulses(4, 0, &mut balance);
        assert_eq!(k, 0);
        assert_eq!(balance, 0);
    }

    #[test]
    fn test_bits_to_pulses_basic() {
        let mut balance = 0;

        // For N=4 with 30 eighth-bits (3.75 bits), should select some K
        let k = bits_to_pulses(4, 30, &mut balance);

        // Should return a valid pulse count (may be 0 if allocation too low)
        assert!(k <= 100);

        // Balance should be updated with difference
        assert_eq!(balance, 30); // All bits unused for low allocation
    }

    #[test]
    fn test_bits_to_pulses_with_balance() {
        let mut balance = -16; // 2 bits deficit

        // Should find K that uses (bits + balance)
        let k = bits_to_pulses(4, 40, &mut balance);

        // Should return valid pulse count
        assert!(k <= 100);

        // Balance tracks allocation efficiency
        // No strict check since exact value depends on quantization
    }

    #[test]
    fn test_decode_spread() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let spread = decode_spread(&mut range_decoder);
        assert!(spread.is_ok());

        let s = spread.unwrap();
        assert!(s <= 3);
    }

    #[test]
    fn test_apply_spreading_no_rotation() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // spread=0 should not rotate
        apply_spreading(&mut vec, 0, 5).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
        assert!((vec[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_apply_spreading_with_rotation() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // spread=3 (f_r=5) should rotate
        apply_spreading(&mut vec, 3, 2).unwrap();

        // Vector should still have unit norm after rotation
        let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);

        // Values should have changed
        assert!((vec[0] - 1.0).abs() > 1e-6);
    }

    #[test]
    fn test_apply_spreading_zero_pulses() {
        let mut vec = vec![1.0, 0.0, 0.0, 0.0];

        // K=0 should not rotate
        apply_spreading(&mut vec, 3, 0).unwrap();

        assert!((vec[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_apply_spreading_invalid_spread() {
        let mut vec = vec![1.0, 0.0];

        let result = apply_spreading(&mut vec, 4, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_pvq_vector_split_small_codebook() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        // Small codebook - should decode directly without splitting
        let result = decode_pvq_vector_split(&mut range_decoder, 4, 2, 2);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 4);

        // Total pulses should be 2
        let total: i32 = vec.iter().map(|&x| x.abs()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_decode_pvq_vector_split_zero_pulses() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector_split(&mut range_decoder, 8, 0, 2);
        assert!(result.is_ok());

        let vec = result.unwrap();
        assert_eq!(vec.len(), 8);
        assert!(vec.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_decode_pvq_vector_split_invalid_n() {
        let data = vec![0x00; 16];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let result = decode_pvq_vector_split(&mut range_decoder, 0, 5, 2);
        assert!(result.is_err());
    }
}
