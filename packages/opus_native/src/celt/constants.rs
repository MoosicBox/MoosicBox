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

/// Energy probability model for Laplace distribution
///
/// RFC 6716 line 6073: "These parameters are held in the `e_prob_model` table"
/// Reference: `quant_bands.c` in libopus
///
/// Format: `[frame_size_index][intra_flag][band * 2]`
/// * `frame_size_index`: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
/// * `intra_flag`: 0=inter-frame, 1=intra-frame
/// * Each band has 2 values: [`prob_of_zero`, `decay_rate`] both in Q8 precision
pub const ENERGY_PROB_MODEL: [[[u8; 42]; 2]; 4] = [
    [
        [
            72, 127, 65, 129, 66, 128, 65, 128, 64, 128, 62, 128, 64, 128, 64, 128, 92, 78, 92, 79,
            92, 78, 90, 79, 116, 41, 115, 40, 114, 40, 132, 26, 132, 26, 145, 17, 161, 12, 176, 10,
            177, 11,
        ],
        [
            24, 179, 48, 138, 54, 135, 54, 132, 53, 134, 56, 133, 55, 132, 55, 132, 61, 114, 70,
            96, 74, 88, 75, 88, 87, 74, 89, 66, 91, 67, 100, 59, 108, 50, 120, 40, 122, 37, 97, 43,
            78, 50,
        ],
    ],
    [
        [
            83, 78, 84, 81, 88, 75, 86, 74, 87, 71, 90, 73, 93, 74, 93, 74, 109, 40, 114, 36, 117,
            34, 117, 34, 143, 17, 145, 18, 146, 19, 162, 12, 165, 10, 178, 7, 189, 6, 190, 8, 177,
            9,
        ],
        [
            23, 178, 54, 115, 63, 102, 66, 98, 69, 99, 74, 89, 71, 91, 73, 91, 78, 89, 86, 80, 92,
            66, 93, 64, 102, 59, 103, 60, 104, 60, 117, 52, 123, 44, 138, 35, 133, 31, 97, 38, 77,
            45,
        ],
    ],
    [
        [
            61, 90, 93, 60, 105, 42, 107, 41, 110, 45, 116, 38, 113, 38, 112, 38, 124, 26, 132, 27,
            136, 19, 140, 20, 155, 14, 159, 16, 158, 18, 170, 13, 177, 10, 187, 8, 192, 6, 175, 9,
            159, 10,
        ],
        [
            21, 178, 59, 110, 71, 86, 75, 85, 84, 83, 91, 66, 88, 73, 87, 72, 92, 75, 98, 72, 105,
            58, 107, 54, 115, 52, 114, 55, 112, 56, 129, 51, 132, 40, 150, 33, 140, 29, 98, 35, 77,
            42,
        ],
    ],
    [
        [
            42, 121, 96, 66, 108, 43, 111, 40, 117, 44, 123, 32, 120, 36, 119, 33, 127, 33, 134,
            34, 139, 21, 147, 23, 152, 20, 158, 25, 154, 26, 166, 21, 173, 16, 184, 13, 184, 10,
            150, 13, 139, 15,
        ],
        [
            22, 178, 63, 114, 74, 82, 84, 83, 92, 82, 103, 62, 96, 72, 96, 67, 101, 73, 107, 72,
            113, 55, 118, 52, 125, 52, 118, 52, 117, 55, 135, 49, 137, 39, 157, 32, 145, 29, 97,
            33, 77, 40,
        ],
    ],
];

/// Alpha coefficient for inter-frame time-domain prediction (frame-size dependent)
///
/// RFC 6716 line 6062: "depend on the frame size in use when not using intra energy"
/// Reference: `pred_coef[4]` in `quant_bands.c`
///
/// Values: [0.9, 0.8, 0.65, 0.5] = [29440, 26112, 21248, 16384] / 32768
/// Index: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
pub const ENERGY_ALPHA_INTER: [f32; 4] = [
    29440.0 / 32768.0,
    26112.0 / 32768.0,
    21248.0 / 32768.0,
    16384.0 / 32768.0,
];

/// Beta coefficient for intra-frame frequency-domain prediction
///
/// RFC 6716 line 6063: "beta=4915/32768 when using intra energy"
/// Reference: `beta_intra` in `quant_bands.c`
pub const ENERGY_BETA_INTRA: f32 = 4915.0 / 32768.0;

/// Beta coefficient for inter-frame frequency-domain prediction (frame-size dependent)
///
/// Reference: `beta_coef[4]` in `quant_bands.c`
///
/// Values: [30147, 22282, 12124, 6554] / 32768
/// Index: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
pub const ENERGY_BETA_INTER: [f32; 4] = [
    30147.0 / 32768.0,
    22282.0 / 32768.0,
    12124.0 / 32768.0,
    6554.0 / 32768.0,
];
