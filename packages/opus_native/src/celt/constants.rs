//! CELT decoder constants from RFC 6716 Section 4.3
//!
//! This module contains all probability distributions, tables, and
//! constants required for CELT decoding.
//!
//! ## Reference Implementation
//!
//! All constants extracted from xiph/opus (Xiph.Org Foundation):
//! * Repository: <https://gitlab.xiph.org/xiph/opus>
//! * Commit: `34bba701ae97c913de719b1f7c10686f62cddb15`
//! * License: BSD 3-Clause
//! * Verified: 2025-10-02

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
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L77-138>
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
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L67-69>
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
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L67-69>
pub const ENERGY_BETA_INTRA: f32 = 4915.0 / 32768.0;

/// Beta coefficient for inter-frame frequency-domain prediction (frame-size dependent)
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L67-69>
///
/// Values: [30147, 22282, 12124, 6554] / 32768
/// Index: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
pub const ENERGY_BETA_INTER: [f32; 4] = [
    30147.0 / 32768.0,
    22282.0 / 32768.0,
    12124.0 / 32768.0,
    6554.0 / 32768.0,
];

/// Static bit allocation table (RFC Table 57, lines 6234-6290)
///
/// Units: 1/32 bit per MDCT bin
/// Dimensions: [band][quality]
/// * band: 0-20 (21 CELT bands from Table 55)
/// * quality: 0-10 (11 quality levels)
///
/// The allocation is computed as: `channels * N * alloc[band][q] << LM >> 2`
/// where N = number of MDCT bins, LM = `log2(frame_size/120)`
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/static_modes_fixed.h#L246-268>
pub const ALLOCATION_TABLE: [[u8; 11]; CELT_NUM_BANDS] = [
    [0, 90, 110, 118, 126, 134, 144, 152, 162, 172, 200],
    [0, 80, 100, 110, 119, 127, 137, 145, 155, 165, 200],
    [0, 75, 90, 103, 112, 120, 130, 138, 148, 158, 200],
    [0, 69, 84, 93, 104, 114, 124, 132, 142, 152, 200],
    [0, 63, 78, 86, 95, 103, 113, 123, 133, 143, 200],
    [0, 56, 71, 80, 89, 97, 107, 117, 127, 137, 200],
    [0, 49, 65, 75, 83, 91, 101, 111, 121, 131, 200],
    [0, 40, 58, 70, 78, 85, 95, 105, 115, 125, 200],
    [0, 34, 51, 65, 72, 78, 88, 98, 108, 118, 198],
    [0, 29, 45, 59, 66, 72, 82, 92, 102, 112, 193],
    [0, 20, 39, 53, 60, 66, 76, 86, 96, 106, 188],
    [0, 18, 32, 47, 54, 60, 70, 80, 90, 100, 183],
    [0, 10, 26, 40, 47, 54, 64, 74, 84, 94, 178],
    [0, 0, 20, 31, 39, 47, 57, 67, 77, 87, 173],
    [0, 0, 12, 23, 32, 41, 51, 61, 71, 81, 168],
    [0, 0, 0, 15, 25, 35, 45, 55, 65, 75, 163],
    [0, 0, 0, 4, 17, 29, 39, 49, 59, 69, 158],
    [0, 0, 0, 0, 12, 23, 33, 43, 53, 63, 153],
    [0, 0, 0, 0, 1, 16, 26, 36, 46, 56, 148],
    [0, 0, 0, 0, 0, 10, 15, 20, 30, 45, 129],
    [0, 0, 0, 0, 0, 1, 1, 1, 1, 20, 104],
];

/// Allocation trim PDF (RFC Table 58, lines 6394-6397)
///
/// Used to decode the allocation trim parameter (0-10, default=5)
/// * trim < 5: bias towards lower frequencies
/// * trim > 5: bias towards higher frequencies
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/celt.c#L173>
pub const TRIM_PDF: [u16; 11] = [2, 2, 5, 10, 22, 46, 22, 10, 5, 2, 2];

/// Maximum allocation caps per band (from `compute_pulse_cache`)
///
/// Dimensions: [LM][stereo][band]
/// * 168 bytes = 21 bands × 8 combinations (4 LM values × 2 stereo modes)
/// * Index formula: `caps[band + 21 * (2*LM + (channels-1))]`
///
/// These caps limit the maximum bits allocated to each band to prevent
/// PVQ encoder/decoder inefficiencies at very high rates.
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/static_modes_fixed.h#L70-75>
pub const CACHE_CAPS: [u8; 168] = [
    224, 224, 224, 224, 224, 224, 224, 224, 160, 160, 160, 160, 185, 185, 185, 178, 178, 168, 134,
    61, 37, 224, 224, 224, 224, 224, 224, 224, 224, 240, 240, 240, 240, 207, 207, 207, 198, 198,
    183, 144, 66, 40, 160, 160, 160, 160, 160, 160, 160, 160, 185, 185, 185, 185, 193, 193, 193,
    183, 183, 172, 138, 64, 38, 240, 240, 240, 240, 240, 240, 240, 240, 207, 207, 207, 207, 204,
    204, 204, 193, 193, 180, 143, 66, 40, 185, 185, 185, 185, 185, 185, 185, 185, 193, 193, 193,
    193, 193, 193, 193, 183, 183, 172, 138, 65, 39, 207, 207, 207, 207, 207, 207, 207, 207, 204,
    204, 204, 204, 201, 201, 201, 188, 188, 176, 141, 66, 40, 193, 193, 193, 193, 193, 193, 193,
    193, 193, 193, 193, 193, 194, 194, 194, 184, 184, 173, 139, 65, 39, 204, 204, 204, 204, 204,
    204, 204, 204, 201, 201, 201, 201, 198, 198, 198, 187, 187, 175, 140, 66, 40,
];

/// Conservative log2 in 1/8 bit units for intensity stereo reservation
///
/// Used to reserve bits for intensity stereo parameter in stereo frames.
/// Index by number of coded bands (end - start).
///
/// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/rate.c#L48-53>
pub const LOG2_FRAC_TABLE: [u8; 24] = [
    0, 8, 13, 16, 19, 21, 23, 24, 26, 27, 28, 29, 30, 31, 32, 32, 33, 34, 34, 35, 36, 36, 37, 37,
];

/// TF resolution adjustment table from RFC 6716 Tables 60-63
///
/// RFC 6716 Section 4.3.4.5 (lines 6633-6697)
///
/// Maps (LM, isTransient, `tf_select`, `tf_change`) → TF resolution adjustment
/// * LM = `log2(frame_size / shortest_frame)`: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
/// * isTransient: 0=non-transient (long MDCT), 1=transient (short MDCTs)
/// * `tf_select`: 0 or 1 (only decoded when it affects result per RFC line 6020-6023)
/// * `tf_change`: 0=no change from base, 1=change from base (per-band flag)
///
/// Index formula: `TF_SELECT_TABLE[LM][isTransient][tf_select][tf_change]`
///
/// Each entry is a signed adjustment to the base TF resolution.
/// * Negative values increase time resolution (shorter transforms)
/// * Positive values increase frequency resolution (longer transforms)
///
/// Reference: RFC 6716 Tables 60-63
pub const TF_SELECT_TABLE: [[[[i8; 2]; 2]; 2]; 4] = [
    // LM=0 (2.5ms frames)
    [
        // Non-transient
        [
            [0, -1], // tf_select=0, [tf_change=0, tf_change=1] (Table 60)
            [0, -1], // tf_select=1, [tf_change=0, tf_change=1] (Table 61)
        ],
        // Transient
        [
            [0, -1], // tf_select=0 (Table 62)
            [0, -1], // tf_select=1 (Table 63)
        ],
    ],
    // LM=1 (5ms frames)
    [
        // Non-transient
        [
            [0, -1], // tf_select=0 (Table 60)
            [0, -2], // tf_select=1 (Table 61)
        ],
        // Transient
        [
            [1, 0],  // tf_select=0 (Table 62)
            [1, -1], // tf_select=1 (Table 63)
        ],
    ],
    // LM=2 (10ms frames)
    [
        // Non-transient
        [
            [0, -2], // tf_select=0 (Table 60)
            [0, -3], // tf_select=1 (Table 61)
        ],
        // Transient
        [
            [2, 0],  // tf_select=0 (Table 62)
            [1, -1], // tf_select=1 (Table 63)
        ],
    ],
    // LM=3 (20ms frames)
    [
        // Non-transient
        [
            [0, -2], // tf_select=0 (Table 60)
            [0, -3], // tf_select=1 (Table 61)
        ],
        // Transient
        [
            [3, 0],  // tf_select=0 (Table 62)
            [1, -1], // tf_select=1 (Table 63)
        ],
    ],
];
