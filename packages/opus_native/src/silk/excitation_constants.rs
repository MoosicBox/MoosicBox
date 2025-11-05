//! SILK excitation signal constants and probability distributions
//!
//! Contains constants for SILK excitation generation per RFC 6716 Section 4.2.7.7-4.2.7.9:
//! * LCG (Linear Congruential Generator) seed PDFs
//! * Rate level PDFs for different frame types
//! * Pulse count PDFs for shell coding
//! * Shell block configurations per bandwidth
//!
//! All PDF constants are stored in ICDF (Inverse Cumulative Distribution Function) format
//! for use with the range decoder's `ec_dec_icdf()` function.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use crate::Bandwidth;

/// Linear Congruential Generator seed PDF (RFC 6716 Section 4.2.7.7)
///
/// ICDF format: Converted from PDF {64, 64, 64, 64}/256
pub const LCG_SEED_PDF: &[u8] = &[192, 128, 64, 0];

/// Returns the number of shell blocks for given bandwidth and frame size
///
/// Shell blocks are used in SILK excitation coding per RFC 6716 Section 4.2.7.8.
///
/// # Returns
///
/// * `Some(count)` - Number of shell blocks for valid bandwidth/frame combinations
/// * `None` - Invalid or unsupported combination
#[must_use]
pub const fn get_shell_block_count(bandwidth: Bandwidth, frame_size_ms: u8) -> Option<usize> {
    match (bandwidth, frame_size_ms) {
        (Bandwidth::Narrowband, 10) => Some(5),
        (Bandwidth::Narrowband, 20) | (Bandwidth::Wideband, 10) => Some(10),
        (Bandwidth::Mediumband, 10) => Some(8),
        (Bandwidth::Mediumband, 20) => Some(15),
        (Bandwidth::Wideband, 20) => Some(20),
        _ => None,
    }
}

/// Rate level PDF for inactive/unvoiced frames (RFC 6716 Table 45)
///
/// ICDF format: Converted from PDF {15, 51, 12, 46, 45, 13, 33, 27, 14}/256
pub const RATE_LEVEL_PDF_INACTIVE: &[u8] = &[241, 190, 178, 132, 87, 74, 41, 14, 0];

/// Rate level PDF for voiced frames (RFC 6716 Table 45)
///
/// ICDF format: Converted from PDF {33, 30, 36, 17, 34, 49, 18, 21, 18}/256
pub const RATE_LEVEL_PDF_VOICED: &[u8] = &[223, 193, 157, 140, 106, 57, 39, 18, 0];

/// Pulse count PDF for rate level 0 (RFC 6716 Table 46)
///
/// ICDF format: Converted from PDF {131, 74, 25, 8, 3, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
pub const PULSE_COUNT_PDF_LEVEL_0: &[u8] = &[
    125, 51, 26, 18, 15, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

/// Pulse count PDF for rate level 1 (RFC 6716 Table 46)
///
/// ICDF format: Converted from PDF {58, 93, 60, 23, 7, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
pub const PULSE_COUNT_PDF_LEVEL_1: &[u8] = &[
    198, 105, 45, 22, 15, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

/// Pulse count PDF for rate level 2 (RFC 6716 Table 46)
///
/// ICDF format: Converted from PDF {43, 51, 46, 33, 24, 16, 11, 8, 6, 3, 3, 3, 2, 1, 1, 2, 1, 2}/256
pub const PULSE_COUNT_PDF_LEVEL_2: &[u8] = &[
    213, 162, 116, 83, 59, 43, 32, 24, 18, 15, 12, 9, 7, 6, 5, 3, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 3: {17, 52, 71, 57, 31, 12, 5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 3 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_3: &[u8] = &[
    239, 187, 116, 59, 28, 16, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 4: {6, 21, 41, 53, 49, 35, 21, 11, 6, 3, 2, 2, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 4 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_4: &[u8] = &[
    250, 229, 188, 135, 86, 51, 30, 19, 13, 10, 8, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 5: {7, 14, 22, 28, 29, 28, 25, 20, 17, 13, 11, 9, 7, 5, 4, 4, 3, 10}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 5 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_5: &[u8] = &[
    249, 235, 213, 185, 156, 128, 103, 83, 66, 53, 42, 33, 26, 21, 17, 13, 10, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 6: {2, 5, 14, 29, 42, 46, 41, 31, 19, 11, 6, 3, 2, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 6 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_6: &[u8] = &[
    254, 249, 235, 206, 164, 118, 77, 46, 27, 16, 10, 7, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 7: {1, 2, 4, 10, 19, 29, 35, 37, 34, 28, 20, 14, 8, 5, 4, 2, 2, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 7 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_7: &[u8] = &[
    255, 253, 249, 239, 220, 191, 156, 119, 85, 57, 37, 23, 15, 10, 6, 4, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 8: {1, 2, 2, 5, 9, 14, 20, 24, 27, 28, 26, 23, 20, 15, 11, 8, 6, 15}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 8 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_8: &[u8] = &[
    255, 253, 251, 246, 237, 223, 203, 179, 152, 124, 98, 75, 55, 40, 29, 21, 15, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 9: {1, 1, 1, 6, 27, 58, 56, 39, 25, 14, 10, 6, 3, 3, 2, 1, 1, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 9 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_9: &[u8] = &[
    255, 254, 253, 247, 220, 162, 106, 67, 42, 28, 18, 12, 9, 6, 4, 3, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 10: {2, 1, 6, 27, 58, 56, 39, 25, 14, 10, 6, 3, 3, 2, 1, 1, 2, 0}/256
// NOTE: Last PDF entry is 0, not a terminator - ICDF has TWO trailing zeros
// Converted to ICDF for ec_dec_icdf()
/// Pulse count pdf level 10 constant (RFC 6716 Section 4.2.7)
pub const PULSE_COUNT_PDF_LEVEL_10: &[u8] = &[
    254, 253, 247, 220, 162, 106, 67, 42, 28, 18, 12, 9, 6, 4, 3, 2, 0, 0,
];

// ====================================================================
// RFC 6716 Tables 47-50: PDFs for Pulse Count Split (lines 5047-5256)
// 64 total PDFs: 4 partition sizes × 16 pulse counts each
// All converted to ICDF for ec_dec_icdf()
// ====================================================================

// ====================================================================
// Table 47: 16-Sample Partition (pulse count 1-16)
// ====================================================================

// RFC 6716 Table 47: Pulse count 1
// RFC shows PDF {126, 130}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 1 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_1: &[u8] = &[130, 0];

// RFC 6716 Table 47: Pulse count 2
// RFC shows PDF {56, 142, 58}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 2 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_2: &[u8] = &[200, 58, 0];

// RFC 6716 Table 47: Pulse count 3
// RFC shows PDF {25, 101, 104, 26}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 3 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_3: &[u8] = &[231, 130, 26, 0];

// RFC 6716 Table 47: Pulse count 4
// RFC shows PDF {12, 60, 108, 64, 12}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 4 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_4: &[u8] = &[244, 184, 76, 12, 0];

// RFC 6716 Table 47: Pulse count 5
// RFC shows PDF {7, 35, 84, 87, 37, 6}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 5 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_5: &[u8] = &[249, 214, 130, 43, 6, 0];

// RFC 6716 Table 47: Pulse count 6
// RFC shows PDF {4, 20, 59, 86, 63, 21, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 6 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_6: &[u8] = &[252, 232, 173, 87, 24, 3, 0];

// RFC 6716 Table 47: Pulse count 7
// RFC shows PDF {3, 12, 38, 72, 75, 42, 12, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 7 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_7: &[u8] = &[253, 241, 203, 131, 56, 14, 2, 0];

// RFC 6716 Table 47: Pulse count 8
// RFC shows PDF {2, 8, 25, 54, 73, 59, 27, 7, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 8 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_8: &[u8] = &[254, 246, 221, 167, 94, 35, 8, 1, 0];

// RFC 6716 Table 47: Pulse count 9
// RFC shows PDF {2, 5, 17, 39, 63, 65, 42, 18, 4, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 9 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_9: &[u8] = &[254, 249, 232, 193, 130, 65, 23, 5, 1, 0];

// RFC 6716 Table 47: Pulse count 10
// RFC shows PDF {1, 4, 12, 28, 49, 63, 54, 30, 11, 3, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 10 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_10: &[u8] = &[255, 251, 239, 211, 162, 99, 45, 15, 4, 1, 0];

// RFC 6716 Table 47: Pulse count 11
// RFC shows PDF {1, 4, 8, 20, 37, 55, 57, 41, 22, 8, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 11 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_11: &[u8] = &[255, 251, 243, 223, 186, 131, 74, 33, 11, 3, 1, 0];

// RFC 6716 Table 47: Pulse count 12
// RFC shows PDF {1, 3, 7, 15, 28, 44, 53, 48, 33, 16, 6, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 12 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_12: &[u8] = &[255, 252, 245, 230, 202, 158, 105, 57, 24, 8, 2, 1, 0];

// RFC 6716 Table 47: Pulse count 13
// RFC shows PDF {1, 2, 6, 12, 21, 35, 47, 48, 40, 25, 12, 5, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 13 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_13: &[u8] =
    &[255, 253, 247, 235, 214, 179, 132, 84, 44, 19, 7, 2, 1, 0];

// RFC 6716 Table 47: Pulse count 14
// RFC shows PDF {1, 1, 4, 10, 17, 27, 37, 47, 43, 33, 21, 9, 4, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 14 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_14: &[u8] = &[
    255, 254, 250, 240, 223, 196, 159, 112, 69, 36, 15, 6, 2, 1, 0,
];

// RFC 6716 Table 47: Pulse count 15
// RFC shows PDF {1, 1, 1, 8, 14, 22, 33, 40, 43, 38, 28, 16, 8, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 15 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_15: &[u8] = &[
    255, 254, 253, 245, 231, 209, 176, 136, 93, 55, 27, 11, 3, 2, 1, 0,
];

// RFC 6716 Table 47: Pulse count 16
// RFC shows PDF {1, 1, 1, 1, 13, 18, 27, 36, 41, 41, 34, 24, 14, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 16 pdf 16 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_16_PDF_16: &[u8] = &[
    255, 254, 253, 252, 239, 221, 194, 158, 117, 76, 42, 18, 4, 3, 2, 1, 0,
];

// ====================================================================
// Table 48: 8-Sample Partition (pulse count 1-16)
// ====================================================================

// RFC 6716 Table 48: Pulse count 1
// RFC shows PDF {127, 129}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 1 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_1: &[u8] = &[129, 0];

// RFC 6716 Table 48: Pulse count 2
// RFC shows PDF {53, 149, 54}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 2 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_2: &[u8] = &[203, 54, 0];

// RFC 6716 Table 48: Pulse count 3
// RFC shows PDF {22, 105, 106, 23}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 3 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_3: &[u8] = &[234, 129, 23, 0];

// RFC 6716 Table 48: Pulse count 4
// RFC shows PDF {11, 61, 111, 63, 10}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 4 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_4: &[u8] = &[245, 184, 73, 10, 0];

// RFC 6716 Table 48: Pulse count 5
// RFC shows PDF {6, 35, 86, 88, 36, 5}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 5 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_5: &[u8] = &[250, 215, 129, 41, 5, 0];

// RFC 6716 Table 48: Pulse count 6
// RFC shows PDF {4, 20, 59, 87, 62, 21, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 6 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_6: &[u8] = &[252, 232, 173, 86, 24, 3, 0];

// RFC 6716 Table 48: Pulse count 7
// RFC shows PDF {3, 13, 40, 71, 73, 41, 13, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 7 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_7: &[u8] = &[253, 240, 200, 129, 56, 15, 2, 0];

// RFC 6716 Table 48: Pulse count 8
// RFC shows PDF {3, 9, 27, 53, 70, 56, 28, 9, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 8 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_8: &[u8] = &[253, 244, 217, 164, 94, 38, 10, 1, 0];

// RFC 6716 Table 48: Pulse count 9
// RFC shows PDF {3, 8, 19, 37, 57, 61, 44, 20, 6, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 9 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_9: &[u8] = &[253, 245, 226, 189, 132, 71, 27, 7, 1, 0];

// RFC 6716 Table 48: Pulse count 10
// RFC shows PDF {3, 7, 15, 28, 44, 54, 49, 33, 17, 5, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 10 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_10: &[u8] = &[253, 246, 231, 203, 159, 105, 56, 23, 6, 1, 0];

// RFC 6716 Table 48: Pulse count 11
// RFC shows PDF {1, 7, 13, 22, 34, 46, 48, 38, 28, 14, 4, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 11 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_11: &[u8] = &[255, 248, 235, 213, 179, 133, 85, 47, 19, 5, 1, 0];

// RFC 6716 Table 48: Pulse count 12
// RFC shows PDF {1, 1, 11, 22, 27, 35, 42, 47, 33, 25, 10, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 12 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_12: &[u8] = &[255, 254, 243, 221, 194, 159, 117, 70, 37, 12, 2, 1, 0];

// RFC 6716 Table 48: Pulse count 13
// RFC shows PDF {1, 1, 6, 14, 26, 37, 43, 43, 37, 26, 14, 6, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 13 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_13: &[u8] =
    &[255, 254, 248, 234, 208, 171, 128, 85, 48, 22, 8, 2, 1, 0];

// RFC 6716 Table 48: Pulse count 14
// RFC shows PDF {1, 1, 4, 10, 20, 31, 40, 42, 40, 31, 20, 10, 4, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 14 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_14: &[u8] = &[
    255, 254, 250, 240, 220, 189, 149, 107, 67, 36, 16, 6, 2, 1, 0,
];

// RFC 6716 Table 48: Pulse count 15
// RFC shows PDF {1, 1, 3, 8, 16, 26, 35, 38, 38, 35, 26, 16, 8, 3, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 15 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_15: &[u8] = &[
    255, 254, 251, 243, 227, 201, 166, 128, 90, 55, 29, 13, 5, 2, 1, 0,
];

// RFC 6716 Table 48: Pulse count 16
// RFC shows PDF {1, 1, 2, 6, 12, 21, 30, 36, 38, 36, 30, 21, 12, 6, 2, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 8 pdf 16 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_8_PDF_16: &[u8] = &[
    255, 254, 252, 246, 234, 213, 183, 147, 109, 73, 43, 22, 10, 4, 2, 1, 0,
];

// ====================================================================
// Table 49: 4-Sample Partition (pulse count 1-16)
// ====================================================================

// RFC 6716 Table 49: Pulse count 1
// RFC shows PDF {127, 129}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 1 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_1: &[u8] = &[129, 0];

// RFC 6716 Table 49: Pulse count 2
// RFC shows PDF {49, 157, 50}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 2 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_2: &[u8] = &[207, 50, 0];

// RFC 6716 Table 49: Pulse count 3
// RFC shows PDF {20, 107, 109, 20}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 3 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_3: &[u8] = &[236, 129, 20, 0];

// RFC 6716 Table 49: Pulse count 4
// RFC shows PDF {11, 60, 113, 62, 10}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 4 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_4: &[u8] = &[245, 185, 72, 10, 0];

// RFC 6716 Table 49: Pulse count 5
// RFC shows PDF {7, 36, 84, 87, 36, 6}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 5 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_5: &[u8] = &[249, 213, 129, 42, 6, 0];

// RFC 6716 Table 49: Pulse count 6
// RFC shows PDF {6, 24, 57, 82, 60, 23, 4}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 6 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_6: &[u8] = &[250, 226, 169, 87, 27, 4, 0];

// RFC 6716 Table 49: Pulse count 7
// RFC shows PDF {5, 18, 39, 64, 68, 42, 16, 4}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 7 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_7: &[u8] = &[251, 233, 194, 130, 62, 20, 4, 0];

// RFC 6716 Table 49: Pulse count 8
// RFC shows PDF {6, 14, 29, 47, 61, 52, 30, 14, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 8 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_8: &[u8] = &[250, 236, 207, 160, 99, 47, 17, 3, 0];

// RFC 6716 Table 49: Pulse count 9
// RFC shows PDF {1, 15, 23, 35, 51, 50, 40, 30, 10, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 9 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_9: &[u8] = &[255, 240, 217, 182, 131, 81, 41, 11, 1, 0];

// RFC 6716 Table 49: Pulse count 10
// RFC shows PDF {1, 1, 21, 32, 42, 52, 46, 41, 18, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 10 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_10: &[u8] = &[255, 254, 233, 201, 159, 107, 61, 20, 2, 1, 0];

// RFC 6716 Table 49: Pulse count 11
// RFC shows PDF {1, 6, 16, 27, 36, 42, 42, 36, 27, 16, 6, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 11 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_11: &[u8] = &[255, 249, 233, 206, 170, 128, 86, 50, 23, 7, 1, 0];

// RFC 6716 Table 49: Pulse count 12
// RFC shows PDF {1, 5, 12, 21, 31, 38, 40, 38, 31, 21, 12, 5, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 12 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_12: &[u8] = &[255, 250, 238, 217, 186, 148, 108, 70, 39, 18, 6, 1, 0];

// RFC 6716 Table 49: Pulse count 13
// RFC shows PDF {1, 3, 9, 17, 26, 34, 38, 38, 34, 26, 17, 9, 3, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 13 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_13: &[u8] =
    &[255, 252, 243, 226, 200, 166, 128, 90, 56, 30, 13, 4, 1, 0];

// RFC 6716 Table 49: Pulse count 14
// RFC shows PDF {1, 3, 7, 14, 22, 29, 34, 36, 34, 29, 22, 14, 7, 3, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 14 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_14: &[u8] = &[
    255, 252, 245, 231, 209, 180, 146, 110, 76, 47, 25, 11, 4, 1, 0,
];

// RFC 6716 Table 49: Pulse count 15
// RFC shows PDF {1, 2, 5, 11, 18, 25, 31, 35, 35, 31, 25, 18, 11, 5, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 15 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_15: &[u8] = &[
    255, 253, 248, 237, 219, 194, 163, 128, 93, 62, 37, 19, 8, 3, 1, 0,
];

// RFC 6716 Table 49: Pulse count 16
// RFC shows PDF {1, 1, 4, 9, 15, 21, 28, 32, 34, 32, 28, 21, 15, 9, 4, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 4 pdf 16 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_4_PDF_16: &[u8] = &[
    255, 254, 250, 241, 226, 205, 177, 145, 111, 79, 51, 30, 15, 6, 2, 1, 0,
];

// ====================================================================
// Table 50: 2-Sample Partition (pulse count 1-16)
// ====================================================================

// RFC 6716 Table 50: Pulse count 1
// RFC shows PDF {128, 128}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 1 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_1: &[u8] = &[128, 0];

// RFC 6716 Table 50: Pulse count 2
// RFC shows PDF {42, 172, 42}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 2 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_2: &[u8] = &[214, 42, 0];

// RFC 6716 Table 50: Pulse count 3
// RFC shows PDF {21, 107, 107, 21}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 3 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_3: &[u8] = &[235, 128, 21, 0];

// RFC 6716 Table 50: Pulse count 4
// RFC shows PDF {12, 60, 112, 61, 11}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 4 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_4: &[u8] = &[244, 184, 72, 11, 0];

// RFC 6716 Table 50: Pulse count 5
// RFC shows PDF {8, 34, 86, 86, 35, 7}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 5 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_5: &[u8] = &[248, 214, 128, 42, 7, 0];

// RFC 6716 Table 50: Pulse count 6
// RFC shows PDF {8, 23, 55, 90, 55, 20, 5}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 6 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_6: &[u8] = &[248, 225, 170, 80, 25, 5, 0];

// RFC 6716 Table 50: Pulse count 7
// RFC shows PDF {5, 15, 38, 72, 72, 36, 15, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 7 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_7: &[u8] = &[251, 236, 198, 126, 54, 18, 3, 0];

// RFC 6716 Table 50: Pulse count 8
// RFC shows PDF {6, 12, 27, 52, 77, 47, 20, 10, 5}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 8 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_8: &[u8] = &[250, 238, 211, 159, 82, 35, 15, 5, 0];

// RFC 6716 Table 50: Pulse count 9
// RFC shows PDF {6, 19, 28, 35, 40, 40, 35, 28, 19, 6}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 9 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_9: &[u8] = &[250, 231, 203, 168, 128, 88, 53, 25, 6, 0];

// RFC 6716 Table 50: Pulse count 10
// RFC shows PDF {4, 14, 22, 31, 37, 40, 37, 31, 22, 14, 4}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 10 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_10: &[u8] = &[252, 238, 216, 185, 148, 108, 71, 40, 18, 4, 0];

// RFC 6716 Table 50: Pulse count 11
// RFC shows PDF {3, 10, 18, 26, 33, 38, 38, 33, 26, 18, 10, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 11 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_11: &[u8] = &[253, 243, 225, 199, 166, 128, 90, 57, 31, 13, 3, 0];

// RFC 6716 Table 50: Pulse count 12
// RFC shows PDF {2, 8, 13, 21, 29, 36, 38, 36, 29, 21, 13, 8, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 12 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_12: &[u8] = &[254, 246, 233, 212, 183, 147, 109, 73, 44, 23, 10, 2, 0];

// RFC 6716 Table 50: Pulse count 13
// RFC shows PDF {1, 5, 10, 17, 25, 32, 38, 38, 32, 25, 17, 10, 5, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 13 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_13: &[u8] =
    &[255, 250, 240, 223, 198, 166, 128, 90, 58, 33, 16, 6, 1, 0];

// RFC 6716 Table 50: Pulse count 14
// RFC shows PDF {1, 4, 7, 13, 21, 29, 35, 36, 35, 29, 21, 13, 7, 4, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 14 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_14: &[u8] = &[
    255, 251, 244, 231, 210, 181, 146, 110, 75, 46, 25, 12, 5, 1, 0,
];

// RFC 6716 Table 50: Pulse count 15
// RFC shows PDF {1, 2, 5, 10, 17, 25, 32, 36, 36, 32, 25, 17, 10, 5, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 15 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_15: &[u8] = &[
    255, 253, 248, 238, 221, 196, 164, 128, 92, 60, 35, 18, 8, 3, 1, 0,
];

// RFC 6716 Table 50: Pulse count 16
// RFC shows PDF {1, 2, 4, 7, 13, 21, 28, 34, 36, 34, 28, 21, 13, 7, 4, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pulse split 2 pdf 16 constant (RFC 6716 Section 4.2.7)
pub const PULSE_SPLIT_2_PDF_16: &[u8] = &[
    255, 253, 249, 242, 229, 208, 180, 146, 110, 76, 48, 27, 14, 7, 3, 1, 0,
];

/// Gets the pulse split PDF for hierarchical decoding (RFC 6716 Section 4.2.7.8.3, lines 4995-5007).
///
/// # Arguments
///
/// * `partition_size` - Size of current partition (16, 8, 4, or 2)
/// * `pulse_count` - Number of pulses in partition (1-16)
///
/// # Returns
///
/// * `Some(&[u8])` - ICDF array for the given partition size and pulse count
/// * `None` - If partition size or pulse count is invalid
#[must_use]
#[allow(clippy::match_same_arms)]
pub const fn get_pulse_split_pdf(partition_size: usize, pulse_count: u8) -> Option<&'static [u8]> {
    match (partition_size, pulse_count) {
        (16, 1) => Some(PULSE_SPLIT_16_PDF_1),
        (16, 2) => Some(PULSE_SPLIT_16_PDF_2),
        (16, 3) => Some(PULSE_SPLIT_16_PDF_3),
        (16, 4) => Some(PULSE_SPLIT_16_PDF_4),
        (16, 5) => Some(PULSE_SPLIT_16_PDF_5),
        (16, 6) => Some(PULSE_SPLIT_16_PDF_6),
        (16, 7) => Some(PULSE_SPLIT_16_PDF_7),
        (16, 8) => Some(PULSE_SPLIT_16_PDF_8),
        (16, 9) => Some(PULSE_SPLIT_16_PDF_9),
        (16, 10) => Some(PULSE_SPLIT_16_PDF_10),
        (16, 11) => Some(PULSE_SPLIT_16_PDF_11),
        (16, 12) => Some(PULSE_SPLIT_16_PDF_12),
        (16, 13) => Some(PULSE_SPLIT_16_PDF_13),
        (16, 14) => Some(PULSE_SPLIT_16_PDF_14),
        (16, 15) => Some(PULSE_SPLIT_16_PDF_15),
        (16, 16) => Some(PULSE_SPLIT_16_PDF_16),

        (8, 1) => Some(PULSE_SPLIT_8_PDF_1),
        (8, 2) => Some(PULSE_SPLIT_8_PDF_2),
        (8, 3) => Some(PULSE_SPLIT_8_PDF_3),
        (8, 4) => Some(PULSE_SPLIT_8_PDF_4),
        (8, 5) => Some(PULSE_SPLIT_8_PDF_5),
        (8, 6) => Some(PULSE_SPLIT_8_PDF_6),
        (8, 7) => Some(PULSE_SPLIT_8_PDF_7),
        (8, 8) => Some(PULSE_SPLIT_8_PDF_8),
        (8, 9) => Some(PULSE_SPLIT_8_PDF_9),
        (8, 10) => Some(PULSE_SPLIT_8_PDF_10),
        (8, 11) => Some(PULSE_SPLIT_8_PDF_11),
        (8, 12) => Some(PULSE_SPLIT_8_PDF_12),
        (8, 13) => Some(PULSE_SPLIT_8_PDF_13),
        (8, 14) => Some(PULSE_SPLIT_8_PDF_14),
        (8, 15) => Some(PULSE_SPLIT_8_PDF_15),
        (8, 16) => Some(PULSE_SPLIT_8_PDF_16),

        (4, 1) => Some(PULSE_SPLIT_4_PDF_1),
        (4, 2) => Some(PULSE_SPLIT_4_PDF_2),
        (4, 3) => Some(PULSE_SPLIT_4_PDF_3),
        (4, 4) => Some(PULSE_SPLIT_4_PDF_4),
        (4, 5) => Some(PULSE_SPLIT_4_PDF_5),
        (4, 6) => Some(PULSE_SPLIT_4_PDF_6),
        (4, 7) => Some(PULSE_SPLIT_4_PDF_7),
        (4, 8) => Some(PULSE_SPLIT_4_PDF_8),
        (4, 9) => Some(PULSE_SPLIT_4_PDF_9),
        (4, 10) => Some(PULSE_SPLIT_4_PDF_10),
        (4, 11) => Some(PULSE_SPLIT_4_PDF_11),
        (4, 12) => Some(PULSE_SPLIT_4_PDF_12),
        (4, 13) => Some(PULSE_SPLIT_4_PDF_13),
        (4, 14) => Some(PULSE_SPLIT_4_PDF_14),
        (4, 15) => Some(PULSE_SPLIT_4_PDF_15),
        (4, 16) => Some(PULSE_SPLIT_4_PDF_16),

        (2, 1) => Some(PULSE_SPLIT_2_PDF_1),
        (2, 2) => Some(PULSE_SPLIT_2_PDF_2),
        (2, 3) => Some(PULSE_SPLIT_2_PDF_3),
        (2, 4) => Some(PULSE_SPLIT_2_PDF_4),
        (2, 5) => Some(PULSE_SPLIT_2_PDF_5),
        (2, 6) => Some(PULSE_SPLIT_2_PDF_6),
        (2, 7) => Some(PULSE_SPLIT_2_PDF_7),
        (2, 8) => Some(PULSE_SPLIT_2_PDF_8),
        (2, 9) => Some(PULSE_SPLIT_2_PDF_9),
        (2, 10) => Some(PULSE_SPLIT_2_PDF_10),
        (2, 11) => Some(PULSE_SPLIT_2_PDF_11),
        (2, 12) => Some(PULSE_SPLIT_2_PDF_12),
        (2, 13) => Some(PULSE_SPLIT_2_PDF_13),
        (2, 14) => Some(PULSE_SPLIT_2_PDF_14),
        (2, 15) => Some(PULSE_SPLIT_2_PDF_15),
        (2, 16) => Some(PULSE_SPLIT_2_PDF_16),

        _ => None,
    }
}

// RFC 6716 Table 51: PDF for Excitation LSBs (lines 5276-5282)
// RFC shows PDF: {136, 120}/256
// Converted to ICDF for ec_dec_icdf()
/// Excitation lsb pdf constant (RFC 6716 Section 4.2.7)
pub const EXCITATION_LSB_PDF: &[u8] = &[120, 0];

// ====================================================================
// RFC 6716 Table 52: PDFs for Excitation Signs (lines 5310-5420)
// 42 total PDFs: 3 signal types × 2 quantization offset types × 7 pulse count categories
// All converted to ICDF for ec_dec_icdf()
// ====================================================================

// Signal Type: Inactive, Quantization Offset: Low
// RFC 6716 Table 52 line 5314: Pulse count 0, PDF {2, 254}/256
pub const SIGN_PDF_INACTIVE_LOW_0: &[u8] = &[254, 0];

// RFC 6716 Table 52 line 5316: Pulse count 1, PDF {207, 49}/256
pub const SIGN_PDF_INACTIVE_LOW_1: &[u8] = &[49, 0];

// RFC 6716 Table 52 line 5318: Pulse count 2, PDF {189, 67}/256
pub const SIGN_PDF_INACTIVE_LOW_2: &[u8] = &[67, 0];

// RFC 6716 Table 52 line 5327: Pulse count 3, PDF {179, 77}/256
pub const SIGN_PDF_INACTIVE_LOW_3: &[u8] = &[77, 0];

// RFC 6716 Table 52 line 5329: Pulse count 4, PDF {174, 82}/256
pub const SIGN_PDF_INACTIVE_LOW_4: &[u8] = &[82, 0];

// RFC 6716 Table 52 line 5331: Pulse count 5, PDF {163, 93}/256
pub const SIGN_PDF_INACTIVE_LOW_5: &[u8] = &[93, 0];

// RFC 6716 Table 52 line 5333: Pulse count 6+, PDF {157, 99}/256
pub const SIGN_PDF_INACTIVE_LOW_6PLUS: &[u8] = &[99, 0];

// Signal Type: Inactive, Quantization Offset: High
// RFC 6716 Table 52 line 5335: Pulse count 0, PDF {58, 198}/256
pub const SIGN_PDF_INACTIVE_HIGH_0: &[u8] = &[198, 0];

// RFC 6716 Table 52 line 5337: Pulse count 1, PDF {245, 11}/256
pub const SIGN_PDF_INACTIVE_HIGH_1: &[u8] = &[11, 0];

// RFC 6716 Table 52 line 5339: Pulse count 2, PDF {238, 18}/256
pub const SIGN_PDF_INACTIVE_HIGH_2: &[u8] = &[18, 0];

// RFC 6716 Table 52 line 5341: Pulse count 3, PDF {232, 24}/256
pub const SIGN_PDF_INACTIVE_HIGH_3: &[u8] = &[24, 0];

// RFC 6716 Table 52 line 5343: Pulse count 4, PDF {225, 31}/256
pub const SIGN_PDF_INACTIVE_HIGH_4: &[u8] = &[31, 0];

// RFC 6716 Table 52 line 5345: Pulse count 5, PDF {220, 36}/256
pub const SIGN_PDF_INACTIVE_HIGH_5: &[u8] = &[36, 0];

// RFC 6716 Table 52 line 5347: Pulse count 6+, PDF {211, 45}/256
pub const SIGN_PDF_INACTIVE_HIGH_6PLUS: &[u8] = &[45, 0];

// Signal Type: Unvoiced, Quantization Offset: Low
// RFC 6716 Table 52 line 5349: Pulse count 0, PDF {1, 255}/256
pub const SIGN_PDF_UNVOICED_LOW_0: &[u8] = &[255, 0];

// RFC 6716 Table 52 line 5351: Pulse count 1, PDF {210, 46}/256
pub const SIGN_PDF_UNVOICED_LOW_1: &[u8] = &[46, 0];

// RFC 6716 Table 52 line 5353: Pulse count 2, PDF {190, 66}/256
pub const SIGN_PDF_UNVOICED_LOW_2: &[u8] = &[66, 0];

// RFC 6716 Table 52 line 5355: Pulse count 3, PDF {178, 78}/256
pub const SIGN_PDF_UNVOICED_LOW_3: &[u8] = &[78, 0];

// RFC 6716 Table 52 line 5357: Pulse count 4, PDF {169, 87}/256
pub const SIGN_PDF_UNVOICED_LOW_4: &[u8] = &[87, 0];

// RFC 6716 Table 52 line 5359: Pulse count 5, PDF {162, 94}/256
pub const SIGN_PDF_UNVOICED_LOW_5: &[u8] = &[94, 0];

// RFC 6716 Table 52 line 5361-5362: Pulse count 6+, PDF {152, 104}/256
pub const SIGN_PDF_UNVOICED_LOW_6PLUS: &[u8] = &[104, 0];

// Signal Type: Unvoiced, Quantization Offset: High
// RFC 6716 Table 52 line 5364: Pulse count 0, PDF {48, 208}/256
pub const SIGN_PDF_UNVOICED_HIGH_0: &[u8] = &[208, 0];

// RFC 6716 Table 52 line 5366: Pulse count 1, PDF {242, 14}/256
pub const SIGN_PDF_UNVOICED_HIGH_1: &[u8] = &[14, 0];

// RFC 6716 Table 52 line 5368: Pulse count 2, PDF {235, 21}/256
pub const SIGN_PDF_UNVOICED_HIGH_2: &[u8] = &[21, 0];

// RFC 6716 Table 52 line 5370: Pulse count 3, PDF {224, 32}/256
pub const SIGN_PDF_UNVOICED_HIGH_3: &[u8] = &[32, 0];

// RFC 6716 Table 52 line 5372: Pulse count 4, PDF {214, 42}/256
pub const SIGN_PDF_UNVOICED_HIGH_4: &[u8] = &[42, 0];

// RFC 6716 Table 52 line 5374: Pulse count 5, PDF {205, 51}/256
pub const SIGN_PDF_UNVOICED_HIGH_5: &[u8] = &[51, 0];

// RFC 6716 Table 52 line 5383: Pulse count 6+, PDF {190, 66}/256
pub const SIGN_PDF_UNVOICED_HIGH_6PLUS: &[u8] = &[66, 0];

// Signal Type: Voiced, Quantization Offset: Low
// RFC 6716 Table 52 line 5385: Pulse count 0, PDF {1, 255}/256
pub const SIGN_PDF_VOICED_LOW_0: &[u8] = &[255, 0];

// RFC 6716 Table 52 line 5387: Pulse count 1, PDF {162, 94}/256
pub const SIGN_PDF_VOICED_LOW_1: &[u8] = &[94, 0];

// RFC 6716 Table 52 line 5389-5390: Pulse count 2, PDF {152, 104}/256
pub const SIGN_PDF_VOICED_LOW_2: &[u8] = &[104, 0];

// RFC 6716 Table 52 line 5392-5393: Pulse count 3, PDF {147, 109}/256
pub const SIGN_PDF_VOICED_LOW_3: &[u8] = &[109, 0];

// RFC 6716 Table 52 line 5395-5396: Pulse count 4, PDF {144, 112}/256
pub const SIGN_PDF_VOICED_LOW_4: &[u8] = &[112, 0];

// RFC 6716 Table 52 line 5398-5399: Pulse count 5, PDF {141, 115}/256
pub const SIGN_PDF_VOICED_LOW_5: &[u8] = &[115, 0];

// RFC 6716 Table 52 line 5401-5402: Pulse count 6+, PDF {138, 118}/256
pub const SIGN_PDF_VOICED_LOW_6PLUS: &[u8] = &[118, 0];

// Signal Type: Voiced, Quantization Offset: High
// RFC 6716 Table 52 line 5404: Pulse count 0, PDF {8, 248}/256
pub const SIGN_PDF_VOICED_HIGH_0: &[u8] = &[248, 0];

// RFC 6716 Table 52 line 5406: Pulse count 1, PDF {203, 53}/256
pub const SIGN_PDF_VOICED_HIGH_1: &[u8] = &[53, 0];

// RFC 6716 Table 52 line 5408: Pulse count 2, PDF {187, 69}/256
pub const SIGN_PDF_VOICED_HIGH_2: &[u8] = &[69, 0];

// RFC 6716 Table 52 line 5410: Pulse count 3, PDF {176, 80}/256
pub const SIGN_PDF_VOICED_HIGH_3: &[u8] = &[80, 0];

// RFC 6716 Table 52 line 5412: Pulse count 4, PDF {168, 88}/256
pub const SIGN_PDF_VOICED_HIGH_4: &[u8] = &[88, 0];

// RFC 6716 Table 52 line 5414: Pulse count 5, PDF {161, 95}/256
pub const SIGN_PDF_VOICED_HIGH_5: &[u8] = &[95, 0];

// RFC 6716 Table 52 line 5416-5417: Pulse count 6+, PDF {154, 102}/256
pub const SIGN_PDF_VOICED_HIGH_6PLUS: &[u8] = &[102, 0];
