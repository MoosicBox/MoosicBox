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
