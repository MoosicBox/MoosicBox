//! Crate-internal SILK entropy tables shared by encoder and decoder.

// RFC 6716 Table 6: Stereo weight PDFs (lines 2225-2238)
// RFC shows PDF Stage 1: {7, 2, 1, 1, 1, 10, 24, 8, 1, 1, 3, 23, 92, 23, 3, 1, 1, 8, 24, 10, 1, 1, 1, 2, 7}/256
// Converted to ICDF for ec_dec_icdf()
pub const STEREO_WEIGHT_PDF_STAGE1: &[u8] = &[
    249, 247, 246, 245, 244, 234, 210, 202, 201, 200, 197, 174, 82, 59, 56, 55, 54, 46, 22, 12, 11,
    10, 9, 7, 0,
];

// RFC shows PDF Stage 2: {85, 86, 85}/256
// Converted to ICDF for ec_dec_icdf()
pub const STEREO_WEIGHT_PDF_STAGE2: &[u8] = &[171, 85, 0];

// RFC shows PDF Stage 3: {51, 51, 52, 51, 51}/256
// Converted to ICDF for ec_dec_icdf()
pub const STEREO_WEIGHT_PDF_STAGE3: &[u8] = &[205, 154, 102, 51, 0];

pub const STEREO_WEIGHT_TABLE_Q13: &[i16] = &[
    -13732, -10050, -8266, -7526, -6500, -5000, -2950, -820, 820, 2950, 5000, 6500, 7526, 8266,
    10050, 13732,
];

// RFC 6716 Tables 11-13: Gain PDFs (lines 2485-2545)
// RFC shows PDF INACTIVE: {32, 112, 68, 29, 12, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const GAIN_PDF_INACTIVE: &[u8] = &[224, 112, 44, 15, 3, 2, 1, 0];

// RFC shows PDF UNVOICED: {2, 17, 45, 60, 62, 47, 19, 4}/256
// Converted to ICDF for ec_dec_icdf()
pub const GAIN_PDF_UNVOICED: &[u8] = &[254, 237, 192, 132, 70, 23, 4, 0];

// RFC shows PDF VOICED: {1, 3, 26, 71, 94, 50, 9, 2}/256
// Converted to ICDF for ec_dec_icdf()
pub const GAIN_PDF_VOICED: &[u8] = &[255, 252, 226, 155, 61, 11, 2, 0];

// RFC shows PDF LSB: {32, 32, 32, 32, 32, 32, 32, 32}/256
// Converted to ICDF for ec_dec_icdf()
pub const GAIN_PDF_LSB: &[u8] = &[224, 192, 160, 128, 96, 64, 32, 0];

// RFC shows PDF DELTA: {6, 5, 11, 31, 132, 21, 8, 4, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
pub const GAIN_PDF_DELTA: &[u8] = &[
    250, 245, 234, 203, 71, 50, 42, 38, 35, 33, 31, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18,
    17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Tables 9-10: Frame type PDFs (lines 2419-2445)
// RFC shows PDF INACTIVE: {26, 230}/256 (only 2 frame types for inactive)
// Converted to ICDF for ec_dec_icdf()
pub const FRAME_TYPE_PDF_INACTIVE: &[u8] = &[230, 0];

// RFC shows PDF ACTIVE: {24, 74, 148, 10}/256 (4 frame types for active)
// Converted to ICDF for ec_dec_icdf()
pub const FRAME_TYPE_PDF_ACTIVE: &[u8] = &[232, 158, 10, 0];
