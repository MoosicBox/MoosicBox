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
