//! SILK Line Spectral Frequency (LSF) constants and probability distributions
//!
//! Contains constants for SILK LSF quantization per RFC 6716 Section 4.2.7.3-4.2.7.5:
//! * Stage-1 LSF indices (coarse quantization)
//! * Stage-2 LSF residuals (fine quantization)
//! * Separate PDFs for NB/MB/WB and Inactive/Voiced frame types
//!
//! All PDF constants are stored in ICDF (Inverse Cumulative Distribution Function) format
//! for use with the range decoder's `ec_dec_icdf()` function.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

// ============================================================================
// PDF to ICDF Conversion Notice
// ============================================================================
//
// Per RFC 6716 Section 4.1.3.3 (lines 1548-1552):
//
// "Although icdf[k] is more convenient for the code, the frequency counts,
//  f[k], are a more natural representation of the probability distribution
//  function (PDF) for a given symbol. Therefore, this document lists the
//  latter, not the former, when describing the context..."
//
// The RFC tables show PDF (Probability Distribution Function) values for
// human readability, but the ec_dec_icdf() function requires ICDF (Inverse
// Cumulative Distribution Function) format.
//
// Conversion formula:
//   Given PDF: [p₀, p₁, p₂, ..., pₙ] where sum(pᵢ) = 256
//   Calculate cumsum: [p₀, p₀+p₁, p₀+p₁+p₂, ..., 256]
//   ICDF: [256-p₀, 256-(p₀+p₁), ..., 256-256] = [..., 0]
//
// All constants below are stored in ICDF format with RFC PDF values
// documented in comments for reference.
// ============================================================================

// RFC 6716 Table 14: PDFs for Normalized LSF Stage-1 Index Decoding (lines 2639-2660)
// RFC shows PDF NB/MB INACTIVE: {44, 34, 30, 19, 21, 12, 11, 3, 3, 2, 16, 2, 2, 1, 5, 2, 1, 3, 3, 1, 1, 2, 2, 2, 3, 1, 9, 9, 2, 7, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf stage1 pdf nb mb inactive (RFC 6716 Section 4.2.7)
pub const LSF_STAGE1_PDF_NB_MB_INACTIVE: &[u8] = &[
    212, 178, 148, 129, 108, 96, 85, 82, 79, 77, 61, 59, 57, 56, 51, 49, 48, 45, 42, 41, 40, 38,
    36, 34, 31, 30, 21, 12, 10, 3, 1, 0,
];

// RFC shows PDF NB/MB VOICED: {1, 10, 1, 8, 3, 8, 8, 14, 13, 14, 1, 14, 12, 13, 11, 11, 12, 11, 10, 10, 11, 8, 9, 8, 7, 8, 1, 1, 6, 1, 6, 5}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf stage1 pdf nb mb voiced (RFC 6716 Section 4.2.7)
pub const LSF_STAGE1_PDF_NB_MB_VOICED: &[u8] = &[
    255, 245, 244, 236, 233, 225, 217, 203, 190, 176, 175, 161, 149, 136, 125, 114, 102, 91, 81,
    71, 60, 52, 43, 35, 28, 20, 19, 18, 12, 11, 5, 0,
];

// RFC shows PDF WB INACTIVE: {31, 21, 3, 17, 1, 8, 17, 4, 1, 18, 16, 4, 2, 3, 1, 10, 1, 3, 16, 11, 16, 2, 2, 3, 2, 11, 1, 4, 9, 8, 7, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf stage1 pdf wb inactive (RFC 6716 Section 4.2.7)
pub const LSF_STAGE1_PDF_WB_INACTIVE: &[u8] = &[
    225, 204, 201, 184, 183, 175, 158, 154, 153, 135, 119, 115, 113, 110, 109, 99, 98, 95, 79, 68,
    52, 50, 48, 45, 43, 32, 31, 27, 18, 10, 3, 0,
];

// RFC shows PDF WB VOICED: {1, 4, 16, 5, 18, 11, 5, 14, 15, 1, 3, 12, 13, 14, 14, 6, 14, 12, 2, 6, 1, 12, 12, 11, 10, 3, 10, 5, 1, 1, 1, 3}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf stage1 pdf wb voiced (RFC 6716 Section 4.2.7)
pub const LSF_STAGE1_PDF_WB_VOICED: &[u8] = &[
    255, 251, 235, 230, 212, 201, 196, 182, 167, 166, 163, 151, 138, 124, 110, 104, 90, 78, 76, 70,
    69, 57, 45, 34, 24, 21, 11, 6, 5, 4, 3, 0,
];

// RFC 6716 Table 15: PDFs for NB/MB Normalized LSF Stage-2 Index Decoding (lines 2695-2715)
// RFC shows PDF A: {1, 1, 1, 15, 224, 11, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf stage2 pdf nb a (RFC 6716 Section 4.2.7)
pub const LSF_STAGE2_PDF_NB_A: &[u8] = &[255, 254, 253, 238, 14, 3, 2, 1, 0];
/// LSF Stage-2 PDF NB variant B (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_B: &[u8] = &[255, 254, 252, 218, 35, 3, 2, 1, 0];
/// LSF Stage-2 PDF NB variant C (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_C: &[u8] = &[255, 254, 250, 208, 59, 4, 2, 1, 0];
/// LSF Stage-2 PDF NB variant D (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_D: &[u8] = &[255, 254, 246, 194, 71, 10, 2, 1, 0];
/// LSF Stage-2 PDF NB variant E (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_E: &[u8] = &[255, 252, 236, 183, 82, 8, 2, 1, 0];
/// LSF Stage-2 PDF NB variant F (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_F: &[u8] = &[255, 252, 235, 180, 90, 17, 2, 1, 0];
/// LSF Stage-2 PDF NB variant G (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_G: &[u8] = &[255, 248, 224, 171, 97, 30, 4, 1, 0];
/// LSF Stage-2 PDF NB variant H (RFC 6716 Table 15)
pub const LSF_STAGE2_PDF_NB_H: &[u8] = &[255, 254, 236, 173, 95, 37, 7, 1, 0];

// RFC 6716 Table 16: PDFs for WB Normalized LSF Stage-2 Index Decoding (lines 2750-2768)
/// LSF Stage-2 PDF WB variant I (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_I: &[u8] = &[255, 254, 253, 244, 12, 3, 2, 1, 0];
/// LSF Stage-2 PDF WB variant J (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_J: &[u8] = &[255, 254, 252, 224, 38, 3, 2, 1, 0];
/// LSF Stage-2 PDF WB variant K (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_K: &[u8] = &[255, 254, 251, 209, 57, 4, 2, 1, 0];
/// LSF Stage-2 PDF WB variant L (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_L: &[u8] = &[255, 254, 244, 195, 69, 4, 2, 1, 0];
/// LSF Stage-2 PDF WB variant M (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_M: &[u8] = &[255, 251, 232, 184, 84, 7, 2, 1, 0];
/// LSF Stage-2 PDF WB variant N (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_N: &[u8] = &[255, 254, 240, 186, 86, 14, 2, 1, 0];
/// LSF Stage-2 PDF WB variant O (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_O: &[u8] = &[255, 254, 239, 178, 91, 30, 5, 1, 0];
/// LSF Stage-2 PDF WB variant P (RFC 6716 Table 16)
pub const LSF_STAGE2_PDF_WB_P: &[u8] = &[255, 248, 227, 177, 100, 19, 2, 1, 0];

/// Codebook selection table for NB/MB LSF Stage-2 decoding (RFC 6716 Table 17)
///
/// Maps Stage-1 index (I1) and coefficient index to codebook letter (a-h).
pub const LSF_CB_SELECT_NB: &[[u8; 10]; 32] = &[
    [b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a'], // I1=0
    [b'b', b'd', b'b', b'c', b'c', b'b', b'c', b'b', b'b', b'b'], // I1=1
    [b'c', b'b', b'b', b'b', b'b', b'b', b'b', b'b', b'b', b'b'], // I1=2
    [b'b', b'c', b'c', b'c', b'c', b'b', b'c', b'b', b'b', b'b'], // I1=3
    [b'c', b'd', b'd', b'd', b'd', b'c', b'c', b'c', b'c', b'c'], // I1=4
    [b'a', b'f', b'd', b'd', b'c', b'c', b'c', b'c', b'b', b'b'], // I1=5
    [b'a', b'c', b'c', b'c', b'c', b'c', b'c', b'c', b'c', b'b'], // I1=6
    [b'c', b'd', b'g', b'e', b'e', b'e', b'f', b'e', b'f', b'f'], // I1=7
    [b'c', b'e', b'f', b'f', b'e', b'f', b'e', b'g', b'e', b'e'], // I1=8
    [b'c', b'e', b'e', b'h', b'e', b'f', b'e', b'f', b'f', b'e'], // I1=9
    [b'e', b'd', b'd', b'd', b'c', b'd', b'c', b'c', b'c', b'c'], // I1=10
    [b'b', b'f', b'f', b'g', b'e', b'f', b'e', b'f', b'f', b'f'], // I1=11
    [b'c', b'h', b'e', b'g', b'f', b'f', b'f', b'f', b'f', b'f'], // I1=12
    [b'c', b'h', b'f', b'f', b'f', b'f', b'f', b'g', b'f', b'e'], // I1=13
    [b'd', b'd', b'f', b'e', b'e', b'f', b'e', b'f', b'e', b'e'], // I1=14
    [b'c', b'd', b'd', b'f', b'f', b'e', b'e', b'e', b'e', b'e'], // I1=15
    [b'c', b'e', b'e', b'g', b'e', b'f', b'e', b'f', b'f', b'f'], // I1=16
    [b'c', b'f', b'e', b'g', b'f', b'f', b'f', b'e', b'f', b'e'], // I1=17
    [b'c', b'h', b'e', b'f', b'e', b'f', b'e', b'f', b'f', b'f'], // I1=18
    [b'c', b'f', b'e', b'g', b'h', b'g', b'f', b'g', b'f', b'e'], // I1=19
    [b'd', b'g', b'h', b'e', b'g', b'f', b'f', b'g', b'e', b'f'], // I1=20
    [b'c', b'h', b'g', b'e', b'e', b'e', b'f', b'e', b'f', b'f'], // I1=21
    [b'e', b'f', b'f', b'e', b'g', b'g', b'f', b'g', b'f', b'e'], // I1=22
    [b'c', b'f', b'f', b'g', b'f', b'g', b'e', b'g', b'e', b'e'], // I1=23
    [b'e', b'f', b'f', b'f', b'd', b'h', b'e', b'f', b'f', b'e'], // I1=24
    [b'c', b'd', b'e', b'f', b'f', b'g', b'e', b'f', b'f', b'e'], // I1=25
    [b'c', b'd', b'c', b'd', b'd', b'e', b'c', b'd', b'd', b'd'], // I1=26
    [b'b', b'b', b'c', b'c', b'c', b'c', b'c', b'd', b'c', b'c'], // I1=27
    [b'e', b'f', b'f', b'g', b'g', b'g', b'f', b'g', b'e', b'f'], // I1=28
    [b'd', b'f', b'f', b'e', b'e', b'e', b'e', b'd', b'd', b'c'], // I1=29
    [b'c', b'f', b'd', b'h', b'f', b'f', b'e', b'e', b'f', b'e'], // I1=30
    [b'e', b'e', b'f', b'e', b'f', b'g', b'f', b'g', b'f', b'e'], // I1=31
];

/// Codebook selection table for WB LSF Stage-2 decoding (RFC 6716 Table 18)
///
/// Maps Stage-1 index (I1) and coefficient index to codebook letter (i-p).
pub const LSF_CB_SELECT_WB: &[[u8; 16]; 32] = &[
    [
        b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i',
        b'i',
    ], // I1=0
    [
        b'k', b'l', b'l', b'l', b'l', b'l', b'k', b'k', b'k', b'k', b'k', b'j', b'j', b'j', b'i',
        b'l',
    ], // I1=1
    [
        b'k', b'n', b'n', b'l', b'p', b'm', b'm', b'n', b'k', b'n', b'm', b'n', b'n', b'm', b'l',
        b'l',
    ], // I1=2
    [
        b'i', b'k', b'j', b'k', b'k', b'j', b'j', b'j', b'j', b'j', b'i', b'i', b'i', b'i', b'i',
        b'j',
    ], // I1=3
    [
        b'i', b'o', b'n', b'm', b'o', b'm', b'p', b'n', b'm', b'm', b'm', b'n', b'n', b'm', b'm',
        b'l',
    ], // I1=4
    [
        b'i', b'l', b'n', b'n', b'm', b'l', b'l', b'n', b'l', b'l', b'l', b'l', b'l', b'l', b'k',
        b'm',
    ], // I1=5
    [
        b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i',
        b'i',
    ], // I1=6
    [
        b'i', b'k', b'o', b'l', b'p', b'k', b'n', b'l', b'm', b'n', b'n', b'm', b'l', b'l', b'k',
        b'l',
    ], // I1=7
    [
        b'i', b'o', b'k', b'o', b'o', b'm', b'n', b'm', b'o', b'n', b'm', b'm', b'n', b'l', b'l',
        b'l',
    ], // I1=8
    [
        b'k', b'j', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i',
        b'i',
    ], // I1=9
    [
        b'i', b'j', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i',
        b'j',
    ], // I1=10
    [
        b'k', b'k', b'l', b'm', b'n', b'l', b'l', b'l', b'l', b'l', b'l', b'l', b'k', b'k', b'j',
        b'l',
    ], // I1=11
    [
        b'k', b'k', b'l', b'l', b'm', b'l', b'l', b'l', b'l', b'l', b'l', b'l', b'l', b'k', b'j',
        b'l',
    ], // I1=12
    [
        b'l', b'm', b'm', b'm', b'o', b'm', b'm', b'n', b'l', b'n', b'm', b'm', b'n', b'm', b'l',
        b'm',
    ], // I1=13
    [
        b'i', b'o', b'm', b'n', b'm', b'p', b'n', b'k', b'o', b'n', b'p', b'm', b'm', b'l', b'n',
        b'l',
    ], // I1=14
    [
        b'i', b'j', b'i', b'j', b'j', b'j', b'j', b'j', b'j', b'j', b'i', b'i', b'i', b'i', b'j',
        b'i',
    ], // I1=15
    [
        b'j', b'o', b'n', b'p', b'n', b'm', b'n', b'l', b'm', b'n', b'm', b'm', b'm', b'l', b'l',
        b'm',
    ], // I1=16
    [
        b'j', b'l', b'l', b'm', b'm', b'l', b'l', b'n', b'k', b'l', b'l', b'n', b'n', b'n', b'l',
        b'm',
    ], // I1=17
    [
        b'k', b'l', b'l', b'k', b'k', b'k', b'l', b'k', b'j', b'k', b'j', b'k', b'j', b'j', b'j',
        b'm',
    ], // I1=18
    [
        b'i', b'k', b'l', b'n', b'l', b'l', b'k', b'k', b'k', b'j', b'j', b'i', b'i', b'i', b'i',
        b'i',
    ], // I1=19
    [
        b'l', b'm', b'l', b'n', b'l', b'l', b'k', b'k', b'j', b'j', b'j', b'j', b'j', b'k', b'k',
        b'm',
    ], // I1=20
    [
        b'k', b'o', b'l', b'p', b'p', b'm', b'n', b'm', b'n', b'l', b'n', b'l', b'l', b'k', b'l',
        b'l',
    ], // I1=21
    [
        b'k', b'l', b'n', b'o', b'o', b'l', b'n', b'l', b'm', b'm', b'l', b'l', b'l', b'l', b'k',
        b'm',
    ], // I1=22
    [
        b'j', b'l', b'l', b'm', b'm', b'm', b'm', b'l', b'n', b'n', b'n', b'l', b'j', b'j', b'j',
        b'j',
    ], // I1=23
    [
        b'k', b'n', b'l', b'o', b'o', b'm', b'p', b'm', b'm', b'n', b'l', b'm', b'm', b'l', b'l',
        b'l',
    ], // I1=24
    [
        b'i', b'o', b'j', b'j', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i', b'i',
        b'i',
    ], // I1=25
    [
        b'i', b'o', b'o', b'l', b'n', b'k', b'n', b'n', b'l', b'm', b'm', b'p', b'p', b'm', b'm',
        b'm',
    ], // I1=26
    [
        b'l', b'l', b'p', b'l', b'n', b'm', b'l', b'l', b'l', b'k', b'k', b'l', b'l', b'l', b'k',
        b'l',
    ], // I1=27
    [
        b'i', b'i', b'j', b'i', b'i', b'i', b'k', b'j', b'k', b'j', b'j', b'k', b'k', b'k', b'j',
        b'j',
    ], // I1=28
    [
        b'i', b'l', b'k', b'n', b'l', b'l', b'k', b'l', b'k', b'j', b'i', b'i', b'j', b'i', b'i',
        b'j',
    ], // I1=29
    [
        b'l', b'n', b'n', b'm', b'p', b'n', b'l', b'l', b'k', b'l', b'k', b'k', b'j', b'i', b'j',
        b'i',
    ], // I1=30
    [
        b'k', b'l', b'n', b'l', b'm', b'l', b'l', b'l', b'k', b'j', b'k', b'o', b'm', b'i', b'i',
        b'i',
    ], // I1=31
];

// RFC 6716 Table 19: PDF for Normalized LSF Index Extension Decoding (lines 2928-2934)
// RFC shows PDF: {156, 60, 24, 9, 4, 2, 1}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf extension pdf (RFC 6716 Section 4.2.7)
pub const LSF_EXTENSION_PDF: &[u8] = &[100, 40, 16, 7, 3, 1, 0];

// RFC 6716 Table 20: Prediction Weights for Normalized LSF Decoding (lines 2975-3009)
// These are Q8 values used for backward prediction in residual dequantization
// Lists A and B are for NB/MB (9 coefficients, k=0..8)
// Lists C and D are for WB (15 coefficients, k=0..14)
/// Lsf pred weights nb a (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHTS_NB_A: &[u8] = &[179, 138, 140, 148, 151, 149, 153, 151, 163];
/// Lsf pred weights nb b (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHTS_NB_B: &[u8] = &[116, 67, 82, 59, 92, 72, 100, 89, 92];
/// Lsf pred weights wb c (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHTS_WB_C: &[u8] = &[
    175, 148, 160, 176, 178, 173, 174, 164, 177, 174, 196, 182, 198, 192, 182,
];
/// Lsf pred weights wb d (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHTS_WB_D: &[u8] = &[
    68, 62, 66, 60, 72, 117, 85, 90, 118, 136, 151, 142, 160, 142, 155,
];

// RFC 6716 Table 21: Prediction Weight Selection for NB/MB Normalized LSF Decoding (lines 3035-3114)
// 32 rows (I1 index) × 9 columns (coefficient index)
// Values: b'A' (use LSF_PRED_WEIGHTS_NB_A) or b'B' (use LSF_PRED_WEIGHTS_NB_B)
/// Lsf pred weight sel nb (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHT_SEL_NB: &[[u8; 9]; 32] = &[
    [b'A', b'B', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=0
    [b'B', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=1
    [b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=2
    [b'B', b'B', b'B', b'A', b'A', b'A', b'A', b'B', b'A'], // I1=3
    [b'A', b'B', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=4
    [b'A', b'B', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=5
    [b'B', b'A', b'B', b'B', b'A', b'A', b'A', b'B', b'A'], // I1=6
    [b'A', b'B', b'B', b'A', b'A', b'B', b'B', b'A', b'A'], // I1=7
    [b'A', b'A', b'B', b'B', b'A', b'B', b'A', b'B', b'B'], // I1=8
    [b'A', b'A', b'B', b'B', b'A', b'A', b'B', b'B', b'B'], // I1=9
    [b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=10
    [b'A', b'B', b'A', b'B', b'B', b'B', b'B', b'B', b'A'], // I1=11
    [b'A', b'B', b'A', b'B', b'B', b'B', b'B', b'B', b'A'], // I1=12
    [b'A', b'B', b'B', b'B', b'B', b'B', b'B', b'B', b'A'], // I1=13
    [b'B', b'A', b'B', b'B', b'A', b'B', b'B', b'B', b'B'], // I1=14
    [b'A', b'B', b'B', b'B', b'B', b'B', b'A', b'B', b'A'], // I1=15
    [b'A', b'A', b'B', b'B', b'A', b'B', b'A', b'B', b'A'], // I1=16
    [b'A', b'A', b'B', b'B', b'B', b'A', b'B', b'B', b'B'], // I1=17
    [b'A', b'B', b'B', b'A', b'A', b'B', b'B', b'B', b'A'], // I1=18
    [b'A', b'A', b'A', b'B', b'B', b'B', b'A', b'B', b'A'], // I1=19
    [b'A', b'B', b'B', b'A', b'A', b'B', b'A', b'B', b'A'], // I1=20
    [b'A', b'B', b'B', b'A', b'A', b'A', b'B', b'B', b'A'], // I1=21
    [b'A', b'A', b'A', b'A', b'A', b'B', b'B', b'B', b'B'], // I1=22
    [b'A', b'A', b'B', b'B', b'A', b'A', b'A', b'B', b'B'], // I1=23
    [b'A', b'A', b'A', b'B', b'A', b'B', b'B', b'B', b'B'], // I1=24
    [b'A', b'B', b'B', b'B', b'B', b'B', b'B', b'B', b'A'], // I1=25
    [b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=26
    [b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A'], // I1=27
    [b'A', b'A', b'B', b'A', b'B', b'B', b'A', b'B', b'A'], // I1=28
    [b'B', b'A', b'A', b'B', b'A', b'A', b'A', b'A', b'A'], // I1=29
    [b'A', b'A', b'A', b'B', b'B', b'A', b'B', b'A', b'B'], // I1=30
    [b'B', b'A', b'B', b'B', b'A', b'B', b'B', b'B', b'B'], // I1=31
];

// RFC 6716 Table 22: Prediction Weight Selection for WB Normalized LSF Decoding (lines 3116-3205)
// 32 rows (I1 index) × 15 columns (coefficient index)
// Values: b'C' (use LSF_PRED_WEIGHTS_WB_C) or b'D' (use LSF_PRED_WEIGHTS_WB_D)
/// Lsf pred weight sel wb (RFC 6716 Section 4.2.7)
pub const LSF_PRED_WEIGHT_SEL_WB: &[[u8; 15]; 32] = &[
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=0
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C',
    ], // I1=1
    [
        b'C', b'C', b'D', b'C', b'C', b'D', b'D', b'D', b'C', b'D', b'D', b'D', b'D', b'C', b'C',
    ], // I1=2
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D', b'C', b'C',
    ], // I1=3
    [
        b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'D', b'C', b'D', b'D', b'D', b'D', b'D', b'C',
    ], // I1=4
    [
        b'C', b'C', b'D', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C',
    ], // I1=5
    [
        b'D', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D', b'C', b'D', b'C',
    ], // I1=6
    [
        b'C', b'D', b'D', b'C', b'C', b'C', b'D', b'C', b'D', b'D', b'D', b'C', b'D', b'C', b'D',
    ], // I1=7
    [
        b'C', b'D', b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'D',
    ], // I1=8
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=9
    [
        b'C', b'D', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C',
    ], // I1=10
    [
        b'C', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'D', b'D', b'D', b'C', b'D', b'C', b'C',
    ], // I1=11
    [
        b'C', b'C', b'D', b'C', b'C', b'D', b'C', b'D', b'C', b'D', b'C', b'C', b'D', b'C', b'C',
    ], // I1=12
    [
        b'C', b'C', b'C', b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'C', b'C',
    ], // I1=13
    [
        b'C', b'D', b'C', b'C', b'C', b'D', b'D', b'C', b'D', b'D', b'D', b'C', b'D', b'D', b'D',
    ], // I1=14
    [
        b'C', b'C', b'D', b'D', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D', b'D', b'C',
    ], // I1=15
    [
        b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'D', b'C', b'D', b'C', b'C',
    ], // I1=16
    [
        b'C', b'C', b'D', b'C', b'C', b'C', b'C', b'D', b'C', b'C', b'D', b'D', b'D', b'C', b'C',
    ], // I1=17
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=18
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D', b'C', b'C',
    ], // I1=19
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C',
    ], // I1=20
    [
        b'C', b'D', b'C', b'D', b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'C', b'D', b'D', b'C',
    ], // I1=21
    [
        b'C', b'C', b'D', b'D', b'D', b'D', b'C', b'D', b'D', b'C', b'C', b'D', b'D', b'C', b'C',
    ], // I1=22
    [
        b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'C', b'D', b'C', b'C', b'C', b'C', b'D', b'C',
    ], // I1=23
    [
        b'C', b'C', b'C', b'D', b'D', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'D', b'D', b'D',
    ], // I1=24
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=25
    [
        b'C', b'D', b'D', b'C', b'C', b'C', b'D', b'D', b'C', b'C', b'D', b'D', b'D', b'D', b'D',
    ], // I1=26
    [
        b'C', b'C', b'C', b'C', b'C', b'D', b'C', b'D', b'D', b'D', b'D', b'C', b'D', b'D', b'D',
    ], // I1=27
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=28
    [
        b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D',
    ], // I1=29
    [
        b'D', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'C', b'D', b'C', b'C', b'C',
    ], // I1=30
    [
        b'C', b'C', b'D', b'C', b'C', b'D', b'D', b'D', b'C', b'C', b'D', b'C', b'C', b'D', b'C',
    ], // I1=31
];

// RFC 6716 Table 23: NB/MB Normalized LSF Stage-1 Codebook Vectors (lines 3255-3333)
// 32 vectors (I1 index) × 10 coefficients (Q8 format)
/// Lsf codebook nb (RFC 6716 Section 4.2.7)
pub const LSF_CODEBOOK_NB: &[[u8; 10]; 32] = &[
    [12, 35, 60, 83, 108, 132, 157, 180, 206, 228], // I1=0
    [15, 32, 55, 77, 101, 125, 151, 175, 201, 225], // I1=1
    [19, 42, 66, 89, 114, 137, 162, 184, 209, 230], // I1=2
    [12, 25, 50, 72, 97, 120, 147, 172, 200, 223],  // I1=3
    [26, 44, 69, 90, 114, 135, 159, 180, 205, 225], // I1=4
    [13, 22, 53, 80, 106, 130, 156, 180, 205, 228], // I1=5
    [15, 25, 44, 64, 90, 115, 142, 168, 196, 222],  // I1=6
    [19, 24, 62, 82, 100, 120, 145, 168, 190, 214], // I1=7
    [22, 31, 50, 79, 103, 120, 151, 170, 203, 227], // I1=8
    [21, 29, 45, 65, 106, 124, 150, 171, 196, 224], // I1=9
    [30, 49, 75, 97, 121, 142, 165, 186, 209, 229], // I1=10
    [19, 25, 52, 70, 93, 116, 143, 166, 192, 219],  // I1=11
    [26, 34, 62, 75, 97, 118, 145, 167, 194, 217],  // I1=12
    [25, 33, 56, 70, 91, 113, 143, 165, 196, 223],  // I1=13
    [21, 34, 51, 72, 97, 117, 145, 171, 196, 222],  // I1=14
    [20, 29, 50, 67, 90, 117, 144, 168, 197, 221],  // I1=15
    [22, 31, 48, 66, 95, 117, 146, 168, 196, 222],  // I1=16
    [24, 33, 51, 77, 116, 134, 158, 180, 200, 224], // I1=17
    [21, 28, 70, 87, 106, 124, 149, 170, 194, 217], // I1=18
    [26, 33, 53, 64, 83, 117, 152, 173, 204, 225],  // I1=19
    [27, 34, 65, 95, 108, 129, 155, 174, 210, 225], // I1=20
    [20, 26, 72, 99, 113, 131, 154, 176, 200, 219], // I1=21
    [34, 43, 61, 78, 93, 114, 155, 177, 205, 229],  // I1=22
    [23, 29, 54, 97, 124, 138, 163, 179, 209, 229], // I1=23
    [30, 38, 56, 89, 118, 129, 158, 178, 200, 231], // I1=24
    [21, 29, 49, 63, 85, 111, 142, 163, 193, 222],  // I1=25
    [27, 48, 77, 103, 133, 158, 179, 196, 215, 232], // I1=26
    [29, 47, 74, 99, 124, 151, 176, 198, 220, 237], // I1=27
    [33, 42, 61, 76, 93, 121, 155, 174, 207, 225],  // I1=28
    [29, 53, 87, 112, 136, 154, 170, 188, 208, 227], // I1=29
    [24, 30, 52, 84, 131, 150, 166, 186, 203, 229], // I1=30
    [37, 48, 64, 84, 104, 118, 156, 177, 201, 230], // I1=31
];

// RFC 6716 Table 24: WB Normalized LSF Stage-1 Codebook Vectors (lines 3335-3413)
// 32 vectors (I1 index) × 16 coefficients (Q8 format)
/// Lsf codebook wb (RFC 6716 Section 4.2.7)
pub const LSF_CODEBOOK_WB: &[[u8; 16]; 32] = &[
    [
        7, 23, 38, 54, 69, 85, 100, 116, 131, 147, 162, 178, 193, 208, 223, 239,
    ], // I1=0
    [
        13, 25, 41, 55, 69, 83, 98, 112, 127, 142, 157, 171, 187, 203, 220, 236,
    ], // I1=1
    [
        15, 21, 34, 51, 61, 78, 92, 106, 126, 136, 152, 167, 185, 205, 225, 240,
    ], // I1=2
    [
        10, 21, 36, 50, 63, 79, 95, 110, 126, 141, 157, 173, 189, 205, 221, 237,
    ], // I1=3
    [
        17, 20, 37, 51, 59, 78, 89, 107, 123, 134, 150, 164, 184, 205, 224, 240,
    ], // I1=4
    [
        10, 15, 32, 51, 67, 81, 96, 112, 129, 142, 158, 173, 189, 204, 220, 236,
    ], // I1=5
    [
        8, 21, 37, 51, 65, 79, 98, 113, 126, 138, 155, 168, 179, 192, 209, 218,
    ], // I1=6
    [
        12, 15, 34, 55, 63, 78, 87, 108, 118, 131, 148, 167, 185, 203, 219, 236,
    ], // I1=7
    [
        16, 19, 32, 36, 56, 79, 91, 108, 118, 136, 154, 171, 186, 204, 220, 237,
    ], // I1=8
    [
        11, 28, 43, 58, 74, 89, 105, 120, 135, 150, 165, 180, 196, 211, 226, 241,
    ], // I1=9
    [
        6, 16, 33, 46, 60, 75, 92, 107, 123, 137, 156, 169, 185, 199, 214, 225,
    ], // I1=10
    [
        11, 19, 30, 44, 57, 74, 89, 105, 121, 135, 152, 169, 186, 202, 218, 234,
    ], // I1=11
    [
        12, 19, 29, 46, 57, 71, 88, 100, 120, 132, 148, 165, 182, 199, 216, 233,
    ], // I1=12
    [
        17, 23, 35, 46, 56, 77, 92, 106, 123, 134, 152, 167, 185, 204, 222, 237,
    ], // I1=13
    [
        14, 17, 45, 53, 63, 75, 89, 107, 115, 132, 151, 171, 188, 206, 221, 240,
    ], // I1=14
    [
        9, 16, 29, 40, 56, 71, 88, 103, 119, 137, 154, 171, 189, 205, 222, 237,
    ], // I1=15
    [
        16, 19, 36, 48, 57, 76, 87, 105, 118, 132, 150, 167, 185, 202, 218, 236,
    ], // I1=16
    [
        12, 17, 29, 54, 71, 81, 94, 104, 126, 136, 149, 164, 182, 201, 221, 237,
    ], // I1=17
    [
        15, 28, 47, 62, 79, 97, 115, 129, 142, 155, 168, 180, 194, 208, 223, 238,
    ], // I1=18
    [
        8, 14, 30, 45, 62, 78, 94, 111, 127, 143, 159, 175, 192, 207, 223, 239,
    ], // I1=19
    [
        17, 30, 49, 62, 79, 92, 107, 119, 132, 145, 160, 174, 190, 204, 220, 235,
    ], // I1=20
    [
        14, 19, 36, 45, 61, 76, 91, 108, 121, 138, 154, 172, 189, 205, 222, 238,
    ], // I1=21
    [
        12, 18, 31, 45, 60, 76, 91, 107, 123, 138, 154, 171, 187, 204, 221, 236,
    ], // I1=22
    [
        13, 17, 31, 43, 53, 70, 83, 103, 114, 131, 149, 167, 185, 203, 220, 237,
    ], // I1=23
    [
        17, 22, 35, 42, 58, 78, 93, 110, 125, 139, 155, 170, 188, 206, 224, 240,
    ], // I1=24
    [
        8, 15, 34, 50, 67, 83, 99, 115, 131, 146, 162, 178, 193, 209, 224, 239,
    ], // I1=25
    [
        13, 16, 41, 66, 73, 86, 95, 111, 128, 137, 150, 163, 183, 206, 225, 241,
    ], // I1=26
    [
        17, 25, 37, 52, 63, 75, 92, 102, 119, 132, 144, 160, 175, 191, 212, 231,
    ], // I1=27
    [
        19, 31, 49, 65, 83, 100, 117, 133, 147, 161, 174, 187, 200, 213, 227, 242,
    ], // I1=28
    [
        18, 31, 52, 68, 88, 103, 117, 126, 138, 149, 163, 177, 192, 207, 223, 239,
    ], // I1=29
    [
        16, 29, 47, 61, 76, 90, 106, 119, 133, 147, 161, 176, 193, 209, 224, 240,
    ], // I1=30
    [
        15, 21, 35, 50, 61, 73, 86, 97, 110, 119, 129, 141, 175, 198, 218, 237,
    ], // I1=31
];

// RFC 6716 Table 25: Minimum Spacing for Normalized LSF Coefficients (lines 3479-3517)
// These are Q15 values representing minimum allowed spacing between consecutive LSF coefficients
// For NB/MB: 11 values (coefficients 0-9, plus final spacing after coefficient 9)
// For WB: 17 values (coefficients 0-15, plus final spacing after coefficient 15)
/// Lsf min spacing nb (RFC 6716 Section 4.2.7)
pub const LSF_MIN_SPACING_NB: &[u16] = &[250, 3, 6, 3, 3, 3, 4, 3, 3, 3, 461];
/// Lsf min spacing wb (RFC 6716 Section 4.2.7)
pub const LSF_MIN_SPACING_WB: &[u16] =
    &[100, 3, 40, 3, 3, 3, 5, 14, 14, 10, 11, 3, 8, 9, 7, 3, 347];

// RFC 6716 Section 4.2.7.5.3: Quantization step sizes for residual dequantization (line 3031)
// Q16 format: 11796 ≈ 0.18, 9830 ≈ 0.15
/// Lsf qstep nb (RFC 6716 Section 4.2.7)
pub const LSF_QSTEP_NB: u16 = 11796;
/// Lsf qstep wb (RFC 6716 Section 4.2.7)
pub const LSF_QSTEP_WB: u16 = 9830;

// RFC 6716 Table 26: PDF for Normalized LSF Interpolation Index (lines 3609-3615)
// RFC shows PDF: {13, 22, 29, 11, 181}/256
// Converted to ICDF for ec_dec_icdf()
/// Lsf interp pdf (RFC 6716 Section 4.2.7)
pub const LSF_INTERP_PDF: &[u8] = &[243, 221, 192, 181, 0];

// RFC 6716 Table 27: LSF Ordering for Polynomial Evaluation (lines 3703-3739)
// Reordering improves numerical accuracy during polynomial construction
// NB/MB: 10 coefficients, WB: 16 coefficients
/// Lsf ordering nb (RFC 6716 Section 4.2.7)
pub const LSF_ORDERING_NB: &[usize; 10] = &[0, 9, 6, 3, 4, 5, 8, 1, 2, 7];
/// Lsf ordering wb (RFC 6716 Section 4.2.7)
pub const LSF_ORDERING_WB: &[usize; 16] = &[0, 15, 8, 7, 4, 11, 12, 3, 2, 13, 10, 5, 6, 9, 14, 1];

// RFC 6716 Table 28: Q12 Cosine Table for LSF Conversion (lines 3763-3841)
// Piecewise linear approximation of cos(pi*x) for x in [0,1]
// 129 values (i=0 to i=128) in Q12 format
// Monotonically decreasing from cos(0)=4096 to cos(π)=-4096
// Cosine table from libopus silk/table_LSF_cos.c
// Q12 format representing cos(pi * i / 128) for i = 0..128
/// Lsf cos table q12 (RFC 6716 Section 4.2.7)
pub const LSF_COS_TABLE_Q12: &[i16; 129] = &[
    8192, 8190, 8182, 8170, 8152, 8130, 8104, 8072, 8034, 7994, 7946, 7896, 7840, 7778, 7714, 7644,
    7568, 7490, 7406, 7318, 7226, 7128, 7026, 6922, 6812, 6698, 6580, 6458, 6332, 6204, 6070, 5934,
    5792, 5648, 5502, 5352, 5198, 5040, 4880, 4718, 4552, 4382, 4212, 4038, 3862, 3684, 3502, 3320,
    3136, 2948, 2760, 2570, 2378, 2186, 1990, 1794, 1598, 1400, 1202, 1002, 802, 602, 402, 202, 0,
    -202, -402, -602, -802, -1002, -1202, -1400, -1598, -1794, -1990, -2186, -2378, -2570, -2760,
    -2948, -3136, -3320, -3502, -3684, -3862, -4038, -4212, -4382, -4552, -4718, -4880, -5040,
    -5198, -5352, -5502, -5648, -5792, -5934, -6070, -6204, -6332, -6458, -6580, -6698, -6812,
    -6922, -7026, -7128, -7226, -7318, -7406, -7490, -7568, -7644, -7714, -7778, -7840, -7896,
    -7946, -7994, -8034, -8072, -8104, -8130, -8152, -8170, -8182, -8190, -8192,
];
