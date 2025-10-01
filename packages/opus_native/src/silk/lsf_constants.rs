#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

// RFC 6716 Table 14: PDFs for Normalized LSF Stage-1 Index Decoding (lines 2639-2660)
// NOTE: All ICDF tables MUST end with 0 per RFC 6716 Section 4.1.3.3 (line 1534):
//       "the table is terminated by a value of 0 (where fh[k] == ft)."
//       The RFC tables show PDF values; ICDF format requires this terminating zero.

pub const LSF_STAGE1_PDF_NB_MB_INACTIVE: &[u8] = &[
    44, 34, 30, 19, 21, 12, 11, 3, 3, 2, 16, 2, 2, 1, 5, 2, 1, 3, 3, 1, 1, 2, 2, 2, 3, 1, 9, 9, 2,
    7, 2, 1, 0,
];

pub const LSF_STAGE1_PDF_NB_MB_VOICED: &[u8] = &[
    1, 10, 1, 8, 3, 8, 8, 14, 13, 14, 1, 14, 12, 13, 11, 11, 12, 11, 10, 10, 11, 8, 9, 8, 7, 8, 1,
    1, 6, 1, 6, 5, 0,
];

pub const LSF_STAGE1_PDF_WB_INACTIVE: &[u8] = &[
    31, 21, 3, 17, 1, 8, 17, 4, 1, 18, 16, 4, 2, 3, 1, 10, 1, 3, 16, 11, 16, 2, 2, 3, 2, 11, 1, 4,
    9, 8, 7, 3, 0,
];

pub const LSF_STAGE1_PDF_WB_VOICED: &[u8] = &[
    1, 4, 16, 5, 18, 11, 5, 14, 15, 1, 3, 12, 13, 14, 14, 6, 14, 12, 2, 6, 1, 12, 12, 11, 10, 3,
    10, 5, 1, 1, 1, 3, 0,
];

// RFC 6716 Table 15: PDFs for NB/MB Normalized LSF Stage-2 Index Decoding (lines 2695-2715)
pub const LSF_STAGE2_PDF_NB_A: &[u8] = &[1, 1, 1, 15, 224, 11, 1, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_B: &[u8] = &[1, 1, 2, 34, 183, 32, 1, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_C: &[u8] = &[1, 1, 4, 42, 149, 55, 2, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_D: &[u8] = &[1, 1, 8, 52, 123, 61, 8, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_E: &[u8] = &[1, 3, 16, 53, 101, 74, 6, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_F: &[u8] = &[1, 3, 17, 55, 90, 73, 15, 1, 1, 0];
pub const LSF_STAGE2_PDF_NB_G: &[u8] = &[1, 7, 24, 53, 74, 67, 26, 3, 1, 0];
pub const LSF_STAGE2_PDF_NB_H: &[u8] = &[1, 1, 18, 63, 78, 58, 30, 6, 1, 0];

// RFC 6716 Table 16: PDFs for WB Normalized LSF Stage-2 Index Decoding (lines 2718-2737)
pub const LSF_STAGE2_PDF_WB_I: &[u8] = &[1, 1, 1, 9, 232, 9, 1, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_J: &[u8] = &[1, 1, 2, 28, 186, 35, 1, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_K: &[u8] = &[1, 1, 3, 42, 152, 53, 2, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_L: &[u8] = &[1, 1, 10, 49, 126, 65, 2, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_M: &[u8] = &[1, 4, 19, 48, 100, 77, 5, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_N: &[u8] = &[1, 1, 14, 54, 100, 72, 12, 1, 1, 0];
pub const LSF_STAGE2_PDF_WB_O: &[u8] = &[1, 1, 15, 61, 87, 61, 25, 4, 1, 0];
pub const LSF_STAGE2_PDF_WB_P: &[u8] = &[1, 7, 21, 50, 77, 81, 17, 1, 1, 0];

// RFC 6716 Table 17: Codebook Selection for NB/MB Normalized LSF Stage-2 Index Decoding (lines 2751-2849)
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

// RFC 6716 Table 18: Codebook Selection for WB Normalized LSF Stage-2 Index Decoding (lines 2851-2909)
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
pub const LSF_EXTENSION_PDF: &[u8] = &[156, 60, 24, 9, 4, 2, 1, 0];

// RFC 6716 Table 20: Prediction Weights for Normalized LSF Decoding (lines 2975-3009)
// These are Q8 values used for backward prediction in residual dequantization
// Lists A and B are for NB/MB (9 coefficients, k=0..8)
// Lists C and D are for WB (15 coefficients, k=0..14)
pub const LSF_PRED_WEIGHTS_NB_A: &[u8] = &[179, 138, 140, 148, 151, 149, 153, 151, 163];
pub const LSF_PRED_WEIGHTS_NB_B: &[u8] = &[116, 67, 82, 59, 92, 72, 100, 89, 92];
pub const LSF_PRED_WEIGHTS_WB_C: &[u8] = &[
    175, 148, 160, 176, 178, 173, 174, 164, 177, 174, 196, 182, 198, 192, 182,
];
pub const LSF_PRED_WEIGHTS_WB_D: &[u8] = &[
    68, 62, 66, 60, 72, 117, 85, 90, 118, 136, 151, 142, 160, 142, 155,
];

// RFC 6716 Table 21: Prediction Weight Selection for NB/MB Normalized LSF Decoding (lines 3035-3114)
// 32 rows (I1 index) × 9 columns (coefficient index)
// Values: b'A' (use LSF_PRED_WEIGHTS_NB_A) or b'B' (use LSF_PRED_WEIGHTS_NB_B)
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
pub const LSF_MIN_SPACING_NB: &[u16] = &[250, 3, 6, 3, 3, 3, 4, 3, 3, 3, 461];
pub const LSF_MIN_SPACING_WB: &[u16] =
    &[100, 3, 40, 3, 3, 3, 5, 14, 14, 10, 11, 3, 8, 9, 7, 3, 347];

// RFC 6716 Section 4.2.7.5.3: Quantization step sizes for residual dequantization (line 3031)
// Q16 format: 11796 ≈ 0.18, 9830 ≈ 0.15
pub const LSF_QSTEP_NB: u16 = 11796;
pub const LSF_QSTEP_WB: u16 = 9830;
