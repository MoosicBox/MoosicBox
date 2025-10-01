#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use crate::silk::decoder::Bandwidth;

pub const LCG_SEED_PDF: &[u8] = &[192, 128, 64, 0];

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

// RFC 6716 Table 45: PDFs for the Rate Level (lines 4883-4891)
// RFC shows PDF Inactive/Unvoiced: {15, 51, 12, 46, 45, 13, 33, 27, 14}/256
// Converted to ICDF for ec_dec_icdf()
pub const RATE_LEVEL_PDF_INACTIVE: &[u8] = &[241, 190, 178, 132, 87, 74, 41, 14, 0];

// RFC 6716 Table 45: PDFs for the Rate Level (lines 4883-4891)
// RFC shows PDF Voiced: {33, 30, 36, 17, 34, 49, 18, 21, 18}/256
// Converted to ICDF for ec_dec_icdf()
pub const RATE_LEVEL_PDF_VOICED: &[u8] = &[223, 193, 157, 140, 106, 57, 39, 18, 0];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 0: {131, 74, 25, 8, 3, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_0: &[u8] = &[
    125, 51, 26, 18, 15, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 1: {58, 93, 60, 23, 7, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_1: &[u8] = &[
    198, 105, 45, 22, 15, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 2: {43, 51, 46, 33, 24, 16, 11, 8, 6, 3, 3, 3, 2, 1, 1, 2, 1, 2}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_2: &[u8] = &[
    213, 162, 116, 83, 59, 43, 32, 24, 18, 15, 12, 9, 7, 6, 5, 3, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 3: {17, 52, 71, 57, 31, 12, 5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_3: &[u8] = &[
    239, 187, 116, 59, 28, 16, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 4: {6, 21, 41, 53, 49, 35, 21, 11, 6, 3, 2, 2, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_4: &[u8] = &[
    250, 229, 188, 135, 86, 51, 30, 19, 13, 10, 8, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 5: {7, 14, 22, 28, 29, 28, 25, 20, 17, 13, 11, 9, 7, 5, 4, 4, 3, 10}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_5: &[u8] = &[
    249, 235, 213, 185, 156, 128, 103, 83, 66, 53, 42, 33, 26, 21, 17, 13, 10, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 6: {2, 5, 14, 29, 42, 46, 41, 31, 19, 11, 6, 3, 2, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_6: &[u8] = &[
    254, 249, 235, 206, 164, 118, 77, 46, 27, 16, 10, 7, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 7: {1, 2, 4, 10, 19, 29, 35, 37, 34, 28, 20, 14, 8, 5, 4, 2, 2, 2}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_7: &[u8] = &[
    255, 253, 249, 239, 220, 191, 156, 119, 85, 57, 37, 23, 15, 10, 6, 4, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 8: {1, 2, 2, 5, 9, 14, 20, 24, 27, 28, 26, 23, 20, 15, 11, 8, 6, 15}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_8: &[u8] = &[
    255, 253, 251, 246, 237, 223, 203, 179, 152, 124, 98, 75, 55, 40, 29, 21, 15, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 9: {1, 1, 1, 6, 27, 58, 56, 39, 25, 14, 10, 6, 3, 3, 2, 1, 1, 2}/256
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_9: &[u8] = &[
    255, 254, 253, 247, 220, 162, 106, 67, 42, 28, 18, 12, 9, 6, 4, 3, 2, 0,
];

// RFC 6716 Table 46: PDFs for the Pulse Count (lines 4935-4973)
// RFC shows PDF Level 10: {2, 1, 6, 27, 58, 56, 39, 25, 14, 10, 6, 3, 3, 2, 1, 1, 2, 0}/256
// NOTE: Last PDF entry is 0, not a terminator - ICDF has TWO trailing zeros
// Converted to ICDF for ec_dec_icdf()
pub const PULSE_COUNT_PDF_LEVEL_10: &[u8] = &[
    254, 253, 247, 220, 162, 106, 67, 42, 28, 18, 12, 9, 6, 4, 3, 2, 0, 0,
];
