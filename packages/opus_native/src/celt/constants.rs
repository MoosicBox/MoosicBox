//! CELT decoder constants from RFC 6716 Section 4.3
//!
//! This module contains all probability distributions, tables, and
//! constants required for CELT decoding.

/// Number of CELT bands (RFC Table 55)
pub const CELT_NUM_BANDS: usize = 21;

/// Start frequency for each band in Hz (RFC Table 55)
#[allow(dead_code)]
pub const CELT_BAND_START_HZ: [u16; CELT_NUM_BANDS] = [
    0, 200, 400, 600, 800, 1000, 1200, 1400, 1600, 2000, 2400, 2800, 3200, 4000, 4800, 5600, 6800,
    8000, 9600, 12000, 15600,
];

/// Stop frequency for each band in Hz (RFC Table 55)
#[allow(dead_code)]
pub const CELT_BAND_STOP_HZ: [u16; CELT_NUM_BANDS] = [
    200, 400, 600, 800, 1000, 1200, 1400, 1600, 2000, 2400, 2800, 3200, 4000, 4800, 5600, 6800,
    8000, 9600, 12000, 15600, 20000,
];

/// MDCT bins per band per channel for 2.5ms frames (RFC Table 55)
pub const CELT_BINS_2_5MS: [u8; CELT_NUM_BANDS] = [
    1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 6, 6, 8, 12, 18, 22,
];

/// MDCT bins per band per channel for 5ms frames (RFC Table 55)
pub const CELT_BINS_5MS: [u8; CELT_NUM_BANDS] = [
    2, 2, 2, 2, 2, 2, 2, 2, 4, 4, 4, 4, 8, 8, 8, 12, 12, 16, 24, 36, 44,
];

/// MDCT bins per band per channel for 10ms frames (RFC Table 55)
pub const CELT_BINS_10MS: [u8; CELT_NUM_BANDS] = [
    4, 4, 4, 4, 4, 4, 4, 4, 8, 8, 8, 8, 16, 16, 16, 24, 24, 32, 48, 72, 88,
];

/// MDCT bins per band per channel for 20ms frames (RFC Table 55)
pub const CELT_BINS_20MS: [u8; CELT_NUM_BANDS] = [
    8, 8, 8, 8, 8, 8, 8, 8, 16, 16, 16, 16, 32, 32, 32, 48, 48, 64, 96, 144, 176,
];

/// Silence flag PDF: {32767, 1}/32768 (RFC Table 56)
pub const CELT_SILENCE_PDF: &[u16] = &[32768, 1, 0];

/// Post-filter flag PDF: {1, 1}/2 (RFC Table 56)
#[allow(dead_code)]
pub const CELT_POST_FILTER_PDF: &[u8] = &[2, 1, 0];

/// Transient flag PDF: {7, 1}/8 (RFC Table 56)
pub const CELT_TRANSIENT_PDF: &[u8] = &[8, 1, 0];

/// Intra flag PDF: {7, 1}/8 (RFC Table 56)
pub const CELT_INTRA_PDF: &[u8] = &[8, 1, 0];

/// Dual stereo flag PDF: {1, 1}/2 (RFC Table 56)
#[allow(dead_code)]
pub const CELT_DUAL_STEREO_PDF: &[u8] = &[2, 1, 0];
