//! SILK Long-Term Prediction (LTP) constants and probability distributions
//!
//! Contains constants for SILK pitch analysis and LTP coding per RFC 6716 Section 4.2.7.6:
//! * Pitch lag PDFs (high and low parts)
//! * Pitch lag delta PDFs for inter-subframe prediction
//! * Pitch contour PDFs and codebooks
//! * LTP filter coefficient PDFs and filter banks
//!
//! All PDF constants are stored in ICDF (Inverse Cumulative Distribution Function) format
//! for use with the range decoder's `ec_dec_icdf()` function.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

// RFC 6716 Table 29: PDF for High Part of Primary Pitch Lag (lines 4169-4175)
// RFC shows PDF: {3, 3, 6, 11, 21, 30, 32, 19, 11, 10, 12, 13, 13, 12, 11, 9, 8, 7, 6, 4, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp lag high pdf (RFC 6716 Section 4.2.7)
pub const LTP_LAG_HIGH_PDF: &[u8] = &[
    253, 250, 244, 233, 212, 182, 150, 131, 120, 110, 98, 85, 72, 60, 49, 40, 32, 25, 19, 15, 13,
    11, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 30: PDFs for Low Part of Primary Pitch Lag (lines 4177-4190)
// RFC shows PDF NB: {64, 64, 64, 64}/256
// RFC shows PDF MB: {43, 42, 43, 43, 42, 43}/256
// RFC shows PDF WB: {32, 32, 32, 32, 32, 32, 32, 32}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp lag low pdf nb (RFC 6716 Section 4.2.7)
pub const LTP_LAG_LOW_PDF_NB: &[u8] = &[192, 128, 64, 0];
/// Ltp lag low pdf mb (RFC 6716 Section 4.2.7)
pub const LTP_LAG_LOW_PDF_MB: &[u8] = &[213, 171, 128, 85, 43, 0];
/// Ltp lag low pdf wb (RFC 6716 Section 4.2.7)
pub const LTP_LAG_LOW_PDF_WB: &[u8] = &[224, 192, 160, 128, 96, 64, 32, 0];

// RFC 6716 Table 31: PDF for Primary Pitch Lag Change (lines 4217-4224)
// RFC shows PDF: {46, 2, 2, 3, 4, 6, 10, 15, 26, 38, 30, 22, 15, 10, 7, 6, 4, 4, 2, 2, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp lag delta pdf (RFC 6716 Section 4.2.7)
pub const LTP_LAG_DELTA_PDF: &[u8] = &[
    210, 208, 206, 203, 199, 193, 183, 168, 142, 104, 74, 52, 37, 27, 20, 14, 10, 6, 4, 2, 0,
];

// RFC 6716 Table 32: PDFs for Subframe Pitch Contour (lines 4233-4253)
// RFC shows PDF NB 10ms: {143, 50, 63}/256
// RFC shows PDF NB 20ms: {68, 12, 21, 17, 19, 22, 30, 24, 17, 16, 10}/256
// RFC shows PDF MB/WB 10ms: {91, 46, 39, 19, 14, 12, 8, 7, 6, 5, 5, 4}/256
// RFC shows PDF MB/WB 20ms: {33, 22, 18, 16, 15, 14, 14, 13, 13, 10, 9, 9, 8, 6, 6, 6, 5, 4, 4, 4, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Pitch contour pdf nb 10ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_PDF_NB_10MS: &[u8] = &[113, 63, 0];
/// Pitch contour pdf nb 20ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_PDF_NB_20MS: &[u8] = &[188, 176, 155, 138, 119, 97, 67, 43, 26, 10, 0];
/// Pitch contour pdf mbwb 10ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_PDF_MBWB_10MS: &[u8] = &[165, 119, 80, 61, 47, 35, 27, 20, 14, 9, 4, 0];
/// Pitch contour pdf mbwb 20ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_PDF_MBWB_20MS: &[u8] = &[
    223, 201, 183, 167, 152, 138, 124, 111, 98, 88, 79, 70, 62, 56, 50, 44, 39, 35, 31, 27, 24, 21,
    18, 16, 14, 12, 10, 8, 6, 4, 3, 2, 1, 0,
];

// RFC 6716 Tables 33-36: Codebooks for Subframe Pitch Contour
// Table 33: NB 10ms (lines 4263-4271) - 2 subframes
/// Pitch contour cb nb 10ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_CB_NB_10MS: &[[i8; 2]; 3] = &[[0, 0], [1, 0], [0, 1]];

// Table 34: NB 20ms (lines 4276-4303) - 4 subframes
/// Pitch contour cb nb 20ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_CB_NB_20MS: &[[i8; 4]; 11] = &[
    [0, 0, 0, 0],
    [2, 1, 0, -1],
    [-1, 0, 1, 2],
    [-1, 0, 0, 1],
    [-1, 0, 0, 0],
    [0, 0, 0, 1],
    [0, 0, 1, 1],
    [1, 1, 0, 0],
    [1, 0, 0, 0],
    [0, 0, 0, -1],
    [1, 0, 0, -1],
];

// Table 35: MB/WB 10ms (lines 4319-4345) - 2 subframes
/// Pitch contour cb mbwb 10ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_CB_MBWB_10MS: &[[i8; 2]; 12] = &[
    [0, 0],
    [0, 1],
    [1, 0],
    [-1, 1],
    [1, -1],
    [-1, 2],
    [2, -1],
    [-2, 2],
    [2, -2],
    [-2, 3],
    [3, -2],
    [-3, 3],
];

// Table 36: MB/WB 20ms (lines 4350-4439) - 4 subframes
/// Pitch contour cb mbwb 20ms (RFC 6716 Section 4.2.7)
pub const PITCH_CONTOUR_CB_MBWB_20MS: &[[i8; 4]; 34] = &[
    [0, 0, 0, 0],
    [0, 0, 1, 1],
    [1, 1, 0, 0],
    [-1, 0, 0, 0],
    [0, 0, 0, 1],
    [1, 0, 0, 0],
    [-1, 0, 0, 1],
    [0, 0, 0, -1],
    [-1, 0, 1, 2],
    [1, 0, 0, -1],
    [-2, -1, 1, 2],
    [2, 1, 0, -1],
    [-2, 0, 0, 2],
    [-2, 0, 1, 3],
    [2, 1, -1, -2],
    [-3, -1, 1, 3],
    [2, 0, 0, -2],
    [3, 1, 0, -2],
    [-3, -1, 2, 4],
    [-4, -1, 1, 4],
    [3, 1, -1, -3],
    [-4, -1, 2, 5],
    [4, 2, -1, -3],
    [4, 1, -1, -4],
    [-5, -1, 2, 6],
    [5, 2, -1, -4],
    [-6, -2, 2, 6],
    [-5, -2, 2, 5],
    [6, 2, -1, -5],
    [-7, -2, 3, 8],
    [6, 2, -2, -6],
    [5, 2, -2, -5],
    [8, 3, -2, -7],
    [-9, -3, 3, 9],
];

// RFC 6716 Table 37: Periodicity Index PDF (lines 4487-4493)
// RFC shows PDF: {77, 80, 99}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp periodicity pdf (RFC 6716 Section 4.2.7)
pub const LTP_PERIODICITY_PDF: &[u8] = &[179, 99, 0];

// RFC 6716 Table 38: LTP Filter PDFs (lines 4500-4514)
// RFC shows PDF for periodicity=0: {185, 15, 13, 13, 9, 9, 6, 6}/256
// RFC shows PDF for periodicity=1: {57, 34, 21, 20, 15, 13, 12, 13, 10, 10, 9, 10, 9, 8, 7, 8}/256
// RFC shows PDF for periodicity=2: {15, 16, 14, 12, 12, 12, 11, 11, 11, 10, 9, 9, 9, 9, 8, 8, 8, 8, 7, 7, 6, 6, 5, 4, 5, 4, 4, 4, 3, 4, 3, 2}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp filter pdf 0 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_PDF_0: &[u8] = &[71, 56, 43, 30, 21, 12, 6, 0];
/// Ltp filter pdf 1 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_PDF_1: &[u8] = &[
    199, 165, 144, 124, 109, 96, 84, 71, 61, 51, 42, 32, 23, 15, 8, 0,
];
/// Ltp filter pdf 2 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_PDF_2: &[u8] = &[
    241, 225, 211, 199, 187, 175, 164, 153, 142, 132, 123, 114, 105, 96, 88, 80, 72, 64, 57, 50,
    44, 38, 33, 29, 24, 20, 16, 12, 9, 5, 2, 0,
];

// RFC 6716 Tables 39-41: LTP Filter Codebooks (5-tap filters, signed Q7 format)
// Table 39: Periodicity Index 0 (lines 4543-4563) - 8 filters
/// Ltp filter cb 0 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_CB_0: &[[i8; 5]; 8] = &[
    [4, 6, 24, 7, 5],
    [0, 0, 2, 0, 0],
    [12, 28, 41, 13, -4],
    [-9, 15, 42, 25, 14],
    [1, -2, 62, 41, -9],
    [-10, 37, 65, -4, 3],
    [-6, 4, 66, 7, -8],
    [16, 14, 38, -3, 33],
];

// Table 40: Periodicity Index 1 (lines 4599-4635) - 16 filters
/// Ltp filter cb 1 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_CB_1: &[[i8; 5]; 16] = &[
    [13, 22, 39, 23, 12],
    [-1, 36, 64, 27, -6],
    [-7, 10, 55, 43, 17],
    [1, 1, 8, 1, 1],
    [6, -11, 74, 53, -9],
    [-12, 55, 76, -12, 8],
    [-3, 3, 93, 27, -4],
    [26, 39, 59, 3, -8],
    [2, 0, 77, 11, 9],
    [-8, 22, 44, -6, 7],
    [40, 9, 26, 3, 9],
    [-7, 20, 101, -7, 4],
    [3, -8, 42, 26, 0],
    [-15, 33, 68, 2, 23],
    [-2, 55, 46, -2, 15],
    [3, -1, 21, 16, 41],
];

// Table 41: Periodicity Index 2 (lines 4637-4720) - 32 filters
/// Ltp filter cb 2 (RFC 6716 Section 4.2.7)
pub const LTP_FILTER_CB_2: &[[i8; 5]; 32] = &[
    [-6, 27, 61, 39, 5],
    [-11, 42, 88, 4, 1],
    [-2, 60, 65, 6, -4],
    [-1, -5, 73, 56, 1],
    [-9, 19, 94, 29, -9],
    [0, 12, 99, 6, 4],
    [8, -19, 102, 46, -13],
    [3, 2, 13, 3, 2],
    [9, -21, 84, 72, -18],
    [-11, 46, 104, -22, 8],
    [18, 38, 48, 23, 0],
    [-16, 70, 83, -21, 11],
    [5, -11, 117, 22, -8],
    [-6, 23, 117, -12, 3],
    [3, -8, 95, 28, 4],
    [-10, 15, 77, 60, -15],
    [-1, 4, 124, 2, -4],
    [3, 38, 84, 24, -25],
    [2, 13, 42, 13, 31],
    [21, -4, 56, 46, -1],
    [-1, 35, 79, -13, 19],
    [-7, 65, 88, -9, -14],
    [20, 4, 81, 49, -29],
    [20, 0, 75, 3, -17],
    [5, -9, 44, 92, -8],
    [1, -3, 22, 69, 31],
    [-6, 95, 41, -12, 5],
    [39, 67, 16, -4, 1],
    [0, -6, 120, 55, -36],
    [-13, 44, 122, 4, -24],
    [81, 5, 11, 3, 7],
    [2, 0, 9, 10, 88],
];

// RFC 6716 Table 42: PDF for LTP Scaling Parameter (lines 4767-4773)
// RFC shows PDF: {128, 64, 64}/256
// Converted to ICDF for ec_dec_icdf()
/// Ltp scaling pdf (RFC 6716 Section 4.2.7)
pub const LTP_SCALING_PDF: &[u8] = &[128, 64, 0];

/// Returns LTP scaling factor in Q14 format for the given index.
///
/// These scaling factors are specified in RFC 6716 Section 4.2.7.6.3 (lines 4751-4753).
/// The values are in Q14 fixed-point format.
///
/// # Arguments
///
/// * `index` - LTP scaling index (0, 1, or 2)
///
/// # Returns
///
/// Scaling factor in Q14 format:
/// * Index 0: 15565 (0.951 in Q14)
/// * Index 1: 12288 (0.75 in Q14)
/// * Index 2: 8192 (0.5 in Q14)
#[must_use]
pub const fn ltp_scaling_factor_q14(index: usize) -> u16 {
    match index {
        1 => 12288,
        2 => 8192,
        _ => 15565,
    }
}
