//! Literal outputs of actual bundled fixed-point `unquant_coarse_energy`.
//! Regenerate with `python3 packages/opus_native/src/celt/coarse_energy_oracle.py`.
//! Continuous inputs are real consecutive encoder packets. This bounded stage
//! chain deliberately feeds coarse output back as history: it is not a full
//! decoder trace (fine/final energy updates are not included).

use super::*;

const PACKETS: [&str; 3] = [
    "f09fd5dcd0e2977eabecef98ff3f560f5b9c1abf4d0e0f83e0b75614b38d422b6f4d8b6df89d13a8bde303412c8c8f42d40aa00d44dc6c7771bc5ba9",
    "f09e7fa7c18f07cfc4b4ed6f2f27f7b680d73dda6173fa286d5e3dd4bc073e5a3eb39c48eb03c2e520d412db0c99392e4305794e1d5003900c37c8e8",
    "f09cdd1897ac354bab791dd827c73eba1364e3efd874f6e47a46bf446daee38662daa462cbc3c539720792374ae45fe926f9f82af83250c6637e7fe8",
];

fn payload(packet: &str) -> Vec<u8> {
    packet
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .skip(1)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
#[allow(clippy::too_many_lines)] // Literal reference arrays.
fn continuous_coarse_q24_matches_actual_libopus() {
    let expected = [
        [
            0,
            16_777_216,
            14_260_736,
            64_592_384,
            57_042_944,
            73_820_160,
            104_858_112,
            83_047_936,
            52_009_984,
            40_265_728,
            42_782_208,
            42_782_208,
            26_004_992,
            28_521_472,
            28_521_472,
            28_521_472,
            28_521_472,
            11_744_256,
            14_260_736,
            31_037_952,
            28_521_472,
        ],
        [
            -100_663_296,
            -102_341_632,
            -66_270_720,
            -20_972_032,
            5_032_960,
            67_108_864,
            90_597_376,
            52_009_984,
            6_711_296,
            -69_625_344,
            -90_597_376,
            -85_564_416,
            -102_341_632,
            -99_825_152,
            -99_825_152,
            -99_825_152,
            -99_825_152,
            -99_825_152,
            -116_602_368,
            -114_085_888,
            -97_308_672,
        ],
        [
            -115_605_504,
            -114_848_552,
            -68_474_116,
            -28_531_020,
            -1_098_680,
            66_500_608,
            86_093_680,
            38_087_464,
            -1_855_632,
            -78_702_116,
            -96_663_408,
            -103_969_576,
            -125_418_280,
            -151_133_444,
            -155_495_684,
            -132_510_980,
            -189_050_116,
            -187_204_868,
            -107_990_276,
            -120_618_720,
            -99_170_016,
        ],
    ];
    let entropy = [
        (13_974_788, 3_058_384, 65),
        (55_423_380, 47_764_756, 87),
        (1_602_675_712, 1_380_652_194, 98),
    ];
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
    let mut history = [0; CELT_NUM_BANDS];
    for (i, packet) in PACKETS.iter().enumerate() {
        let data = payload(packet);
        let mut rd = RangeDecoder::new(&data).unwrap();
        assert!(!decoder.decode_silence(&mut rd).unwrap());
        assert!(decoder.decode_post_filter(&mut rd).unwrap());
        decoder.decode_post_filter_params(&mut rd).unwrap();
        decoder.decode_transient_flag(&mut rd).unwrap();
        let intra = decoder.decode_intra(&mut rd).unwrap();
        decoder
            .decode_coarse_energy_q24(&mut rd, intra, data.len(), &mut history)
            .unwrap();
        assert_eq!(history, expected[i], "coarse frame {i}");
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            entropy[i],
            "entropy frame {i}"
        );
        let resolution = decoder.decode_tf(&mut rd, data.len()).unwrap();
        assert_eq!(decoder.tf_change, vec![i == 0; 21]);
        assert_eq!(resolution, vec![0; 21]);
        assert_eq!(decoder.tf_select, Some(0));
        let tf_entropy = [
            (123_004_427, 11_564_350, 70),
            (27_535_864, 19_877_240, 88),
            (796_253_301, 574_229_783, 99),
        ];
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            tf_entropy[i]
        );
        assert_eq!(decoder.decode_spread(&mut rd).unwrap(), [2, 1, 1][i]);
        let caps = decoder.compute_caps(2, 1);
        let (boost, _, _) = decoder
            .decode_band_boost(&mut rd, (data.len() * 64) as i32, &caps)
            .unwrap();
        let expected_boost = [
            [
                0, 0, 0, 96, 64, 128, 128, 96, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                0, 0, 0, 64, 96, 128, 128, 96, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                0, 0, 32, 64, 96, 128, 128, 96, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ];
        assert_eq!(boost, expected_boost[i]);
        let trim = decoder
            .decode_allocation_trim(&mut rd, (data.len() * 64) as i32, boost.iter().sum())
            .unwrap();
        assert_eq!(trim, [4, 6, 6][i]);
        let dyn_entropy = [
            (107_314_768, 86_246_253, 118),
            (585_716_736, 424_182_223, 139),
            (8_401_360, 5_908_826, 153),
        ];
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            dyn_entropy[i]
        );
        let total = (data.len() * 64) as i32 - rd.ec_tell_frac() as i32 - 1;
        let (allocation, _, _) = decoder
            .allocation_decode(&mut rd, total, &boost, trim, i == 0)
            .unwrap();
        assert_eq!(allocation.coded_bands, [18, 17, 17][i]);
        let alloc_entropy = [
            (13_414_346, 5_760_177, 121),
            (146_429_184, 131_323_855, 141),
            (537_687_040, 437_285_508, 155),
        ];
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            alloc_entropy[i]
        );
        let expected_shapes = [
            [
                92, 86, 81, 155, 123, 166, 162, 133, 200, 111, 102, 93, 168, 146, 135, 171, 143,
                161, 0, 0, 0,
            ],
            [
                104, 97, 91, 132, 155, 173, 168, 139, 212, 113, 109, 97, 170, 141, 124, 145, 110,
                0, 0, 0, 0,
            ],
            [
                101, 94, 112, 136, 152, 170, 165, 136, 206, 107, 104, 93, 161, 131, 113, 121, 81,
                0, 0, 0, 0,
            ],
        ];
        assert_eq!(allocation.shape_bits, expected_shapes[i]);
        let mut refined = history;
        decoder
            .decode_fine_energy_q24(&mut rd, &mut refined, &allocation.fine_energy_bits)
            .unwrap();
        decoder.transient = i == 0;
        decoder.state.anti_collapse_state.seed = 0;
        let (shapes, trace) = decoder
            .mono_residual(
                &mut rd,
                &allocation,
                [2, 1, 1][i],
                data.len() as i32 * 64 - allocation.anti_collapse_rsv,
            )
            .unwrap();
        for band in 0..21 {
            assert_eq!(
                trace[band].bits, BAND_BITS[i][band],
                "budget packet {i} band {band}"
            );
            assert_eq!(
                trace[band].entropy, BAND_ENTROPY[i][band],
                "entropy packet {i} band {band}"
            );
            assert_eq!(
                trace[band].mask, MASKS[i][band],
                "collapse packet {i} band {band}"
            );
        }
        let flat = shapes.concat();
        for (j, (&actual, &expected)) in flat.iter().zip(&RESIDUAL[i]).enumerate() {
            assert_eq!(actual, expected, "coefficient packet {i} bin {j}");
        }
    }
}

#[test]
// Most lines are literal reference arrays, kept inline beside their case inputs.
#[allow(clippy::too_many_lines)]
fn restricted_bands_and_budget_fallback_match_actual_libopus() {
    type BudgetCase = (
        usize,
        usize,
        usize,
        Channels,
        usize,
        bool,
        &'static [i32],
        (u32, u32, u32),
    );
    let cases: &[BudgetCase] = &[
        (
            2,
            8,
            17,
            Channels::Mono,
            120,
            false,
            &[
                -118_882_304,
                -134_317_568,
                -134_317_568,
                -117_540_352,
                -132_975_616,
                -132_975_616,
                -132_975_616,
                -149_752_832,
                -134_317_568,
            ],
            (119_390_208, 107_266_047, 14),
        ),
        (
            3,
            17,
            21,
            Channels::Stereo,
            960,
            true,
            &[
                -16_777_216,
                -14_260_736,
                -14_260_736,
                2_516_480,
                0,
                0,
                16_777_216,
                14_260_736,
            ],
            (233_666_400, 40_402_591, 13),
        ),
        (
            8,
            0,
            21,
            Channels::Stereo,
            240,
            false,
            &[
                -103_546_880,
                -81_400_832,
                -104_217_600,
                -87_440_384,
                -82_071_552,
                -93_479_936,
                -76_702_720,
                -71_333_888,
                -82_742_272,
                -49_187_840,
                -72_004_608,
                -72_004_608,
                -88_781_824,
                -94_150_656,
                -65_965_056,
                -94_150_656,
                -65_965_056,
                -77_373_440,
                -94_150_656,
                -99_519_488,
                -104_888_320,
                -120_324_096,
                -120_324_096,
                -120_324_096,
                -137_101_312,
                -142_470_144,
                -131_061_760,
                -131_061_760,
                -131_061_760,
                -131_061_760,
                -131_061_760,
                -147_838_976,
                -136_430_592,
                -136_430_592,
                -153_207_808,
                -141_799_424,
                -141_799_424,
                -141_799_424,
                -158_576_640,
                -163_945_472,
                -169_314_304,
                -174_683_136,
            ],
            (30_155_840, 4_892_479, 64),
        ),
    ];
    for &(length, start, end, channels, size, intra, expected, entropy) in cases {
        let data = payload(PACKETS[0]);
        let mut rd = RangeDecoder::new(&data[..length]).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, channels, size).unwrap();
        decoder.set_start_band(start);
        decoder.set_end_band(end);
        let count = if channels == Channels::Stereo { 42 } else { 21 };
        let mut history: Vec<i32> = (0..count).map(|i| -10 * (1 << 24) + i * 12_345).collect();
        let initial = history.clone();
        decoder
            .decode_coarse_energy_q24(&mut rd, intra, length, &mut history)
            .unwrap();
        let mut coded = Vec::new();
        for (i, &value) in history.iter().enumerate() {
            if (start..end).contains(&(i % 21)) {
                coded.push(value);
            } else {
                assert_eq!(value, initial[i], "uncoded band {i}");
            }
        }
        assert_eq!(coded, expected, "budget length {length}");
        assert_eq!((rd.get_range(), rd.get_value(), rd.ec_tell()), entropy);
    }
}

#[test]
fn tf_budget_reservation_exhaustion_and_signed_resolution() {
    let cases = [
        (
            2,
            0,
            120,
            false,
            0,
            21,
            21,
            0,
            (2_147_483_648, 806_715_391, 1),
        ),
        (2, 1, 480, true, 0, 21, 3, 2, (44_236_800, 30_572_543, 7)),
        (2, 2, 480, true, 8, 17, 3, 2, (32_036_133, 18_371_876, 8)),
        (3, 3, 240, false, 0, 21, 13, -1, (17_208_531, 8_869_857, 8)),
        (8, 8, 960, false, 0, 21, 13, -2, (17_208_531, 8_869_753, 8)),
    ];
    for (length, budget, size, transient, start, end, transition, signed, entropy) in cases {
        let data = payload(PACKETS[0]);
        let mut rd = RangeDecoder::new(&data[..length]).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, size).unwrap();
        decoder.transient = transient;
        decoder.set_start_band(start);
        decoder.set_end_band(end);
        let resolution = decoder.decode_tf(&mut rd, budget).unwrap();
        for (band, &actual) in resolution.iter().enumerate().take(end).skip(start) {
            assert_eq!(decoder.tf_change[band], band - start >= transition);
            let expected = if (transient && band - start < transition)
                || (!transient && band - start >= transition)
            {
                signed
            } else {
                0
            };
            assert_eq!(actual, expected);
        }
        assert_eq!((rd.get_range(), rd.get_value(), rd.ec_tell()), entropy);
    }
    let mut rd = RangeDecoder::new(&[27, 0x55, 0xaa, 0x77, 0x11]).unwrap();
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
    decoder.transient = true;
    assert_eq!(decoder.decode_tf(&mut rd, 5).unwrap(), vec![1; 21]);
    assert_eq!(decoder.tf_select, Some(1));
    assert_eq!(
        (rd.get_range(), rd.get_value(), rd.ec_tell()),
        (221_506_597, 213_713_679, 5)
    );
}

#[test]
#[allow(clippy::too_many_lines)] // Literal reference arrays.
fn fine_final_q24_stereo_matches_actual_libopus() {
    // Synthetic allocation schedule (precision 0..8), actual reference functions
    // on the three real payloads. This is not a full allocation/decode trace.
    let fine = std::array::from_fn(|i| (i % 9) as u8);
    let priority = std::array::from_fn(|i| (i % 2) as u8);
    let mut history: Vec<i32> = (0..42).map(|i| i * 12_345 - (1 << 24)).collect();
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();
    decoder.set_start_band(3);
    decoder.set_end_band(19);
    let expected_fine = [
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -21_983_061,
            -9_912_092,
            -15_929_059,
            -12_115_626,
            -15_707_761,
            -21_429_816,
            -16_666_111,
            -12_459_462,
            -22_932_877,
            -23_969_108,
            -13_995_291,
            -16_342_242,
            -24_325_289,
            -15_465_584,
            -20_007_991,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -13_335_208,
            -18_041_455,
            -12_524_086,
            -17_099_261,
            -23_050_692,
            -19_466_635,
            -16_406_866,
            -12_200_217,
            -22_673_632,
            -15_321_255,
            -22_124_654,
            -10_840_117,
            -8_599_548,
            -22_415_299,
            -11_032_458,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -29_323_093,
            -14_892_828,
            -7_802_595,
            -17_292_970,
            -16_559_729,
            -24_280_632,
            -16_666_111,
            -10_362_310,
            -29_224_333,
            -29_211_988,
            -16_616_731,
            -18_177_250,
            -20_524_201,
            -21_298_288,
            -15_584_311,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -10_189_480,
            -24_070_767,
            -17_504_822,
            -25_422_333,
            -31_373_764,
            -18_975_115,
            -16_406_866,
            -10_103_065,
            -24_770_784,
            -7_981_223,
            -28_940_398,
            -14_772_277,
            -7_419_900,
            -28_248_003,
            -7_460_746,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -36_663_125,
            -7_290_652,
            -2_821_859,
            -16_309_930,
            -23_309_937,
            -31_850_040,
            -16_666_111,
            -16_653_766,
            -27_127_181,
            -36_552_020,
            -17_141_019,
            -12_147_938,
            -18_033_833,
            -20_577_392,
            -10_177_591,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -7_043_752,
            -16_468_591,
            -17_766_966,
            -27_191_805,
            -29_997_508,
            -14_879_115,
            -16_406_866,
            -8_005_913,
            -31_062_240,
            -641_191,
            -22_124_654,
            -20_801_589,
            837_636,
            -24_512_451,
            -6_576_010,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
    ];
    let expected_final = [
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -21_983_061,
            -10_174_236,
            -15_929_059,
            -12_181_162,
            -15_707_761,
            -21_429_816,
            -16_666_111,
            -14_556_614,
            -22_932_877,
            -23_969_108,
            -13_995_291,
            -16_342_242,
            -24_325_289,
            -15_465_584,
            -20_007_991,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -13_335_208,
            -18_303_599,
            -12_524_086,
            -17_164_797,
            -23_050_692,
            -19_466_635,
            -16_406_866,
            -14_297_369,
            -22_673_632,
            -15_321_255,
            -22_124_654,
            -10_840_117,
            -8_599_548,
            -22_415_299,
            -11_032_458,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -29_323_093,
            -15_154_972,
            -7_802_595,
            -17_227_434,
            -16_559_729,
            -24_280_632,
            -16_666_111,
            -12_459_462,
            -29_224_333,
            -29_211_988,
            -16_616_731,
            -18_177_250,
            -20_524_201,
            -21_298_288,
            -15_584_311,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -10_189_480,
            -24_332_911,
            -17_504_822,
            -25_487_869,
            -31_373_764,
            -18_975_115,
            -16_406_866,
            -12_200_217,
            -24_770_784,
            -7_981_223,
            -28_940_398,
            -14_772_277,
            -7_419_900,
            -28_248_003,
            -7_460_746,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
        [
            -16_777_216,
            -16_764_871,
            -16_752_526,
            -36_663_125,
            -7_552_796,
            -2_821_859,
            -16_244_394,
            -23_309_937,
            -31_850_040,
            -16_666_111,
            -18_750_918,
            -27_127_181,
            -36_552_020,
            -17_141_019,
            -12_147_938,
            -18_033_833,
            -20_577_392,
            -10_177_591,
            -16_555_006,
            -16_542_661,
            -16_530_316,
            -16_517_971,
            -16_505_626,
            -16_493_281,
            -7_043_752,
            -16_730_735,
            -17_766_966,
            -27_257_341,
            -29_997_508,
            -14_879_115,
            -16_406_866,
            -5_908_761,
            -31_062_240,
            -641_191,
            -22_124_654,
            -20_801_589,
            837_636,
            -24_512_451,
            -6_576_010,
            -16_295_761,
            -16_283_416,
            -16_271_071,
        ],
    ];
    for (i, packet) in PACKETS.iter().enumerate() {
        let data = payload(packet);
        let mut rd = RangeDecoder::new(&data).unwrap();
        decoder
            .decode_fine_energy_q24(&mut rd, &mut history, &fine)
            .unwrap();
        assert_eq!(history, expected_fine[i]);
        assert_eq!(rd.ec_tell(), 139);
        decoder
            .decode_final_energy_q24(&mut rd, &mut history, &fine, &priority, 7)
            .unwrap();
        assert_eq!(history, expected_final[i]);
        assert_eq!(rd.ec_tell(), 145);
    }
}

#[test]
fn denormalize_q24_matches_actual_libopus() {
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
    decoder.set_start_band(3);
    decoder.set_end_band(19);
    let energy: Vec<i32> = (0..21)
        .map(|i| (-7 + i % 8) * (1 << 24) + i * 12_345)
        .collect();
    let spectrum: Vec<i16> = (0..480)
        .map(|i| (((i * 7919) % 32768) - 16384) as i16)
        .collect();
    let freq = decoder
        .denormalize_bands_q24(&spectrum, &energy, false)
        .unwrap();
    let hash = freq
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
    assert_eq!(hash, 0x01a4_0dd4_629e_727e); // All 480 actual reference bins.
    let indices = [
        0, 11, 12, 13, 16, 31, 32, 45, 63, 64, 79, 95, 111, 135, 159, 191, 192, 239, 240, 479,
    ];
    let expected = [
        0, 0, 8150, -7301, 12574, -1411, 440, 1243, -3066, -1004, -12341, 23328, 26665, 23393,
        -100, -1088, -693, 1799, 0, 0,
    ];
    for (index, value) in indices.into_iter().zip(expected) {
        assert_eq!(freq[index], value, "bin {index}");
    }
}

#[test]
fn production_energy_uses_q24_history_not_legacy_q8() {
    let mut a = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
    let mut b = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
    for packet in PACKETS {
        let data = payload(packet);
        b.state.prev_energy.fill(1234);
        b.state.prev_prev_energy.fill(-5678);
        let mut ra = RangeDecoder::new(&data).unwrap();
        let mut rb = RangeDecoder::new(&data).unwrap();
        let fa = a.decode_celt_frame(&mut ra, data.len()).unwrap();
        let fb = b.decode_celt_frame(&mut rb, data.len()).unwrap();
        assert_eq!(fa.samples, fb.samples);
        assert_eq!(a.energy_q24.band, b.energy_q24.band);
        assert_eq!(fa.samples.len(), 480);
        assert!(a.energy_q24.band.iter().any(|value| value & 0xffff != 0));
    }
    a.set_frame_size(960).unwrap();
    let saved = a.energy_q24.band;
    a.set_channels(Channels::Stereo);
    assert_eq!(&a.energy_q24.band[..21], &saved[..21]);
    assert_eq!(&a.energy_q24.band[21..], &saved[..21]);
    a.set_channels(Channels::Mono);
    assert_eq!(&a.energy_q24.band[..21], &saved[..21]);
    a.reset();
    assert_eq!(a.energy_q24.band, [0; 42]);
    assert_eq!(a.energy_q24.log, [-28 * (1 << 24); 42]);
}

#[test]
fn q24_history_transient_and_band_switch_semantics() {
    let mut history = energy::EnergyHistory::new();
    let first: Vec<i32> = (0..42).map(|i| i * 12_345 - (1 << 24)).collect();
    history.update(first.clone(), 2, false, 0, 21);
    assert_eq!(history.log.as_slice(), first);
    let transient: Vec<i32> = first.iter().map(|&value| value - 123).collect();
    history.update(transient.clone(), 2, true, 0, 21);
    assert_eq!(history.log.as_slice(), transient);
    assert_eq!(history.log2, [-28 * (1 << 24); 42]);
    history.update(first.clone(), 2, false, 3, 19);
    for (index, &value) in first.iter().enumerate() {
        if (3..19).contains(&(index % 21)) {
            assert_eq!(history.band[index], value);
            assert_eq!(history.log2[index], transient[index]);
        } else {
            assert_eq!(history.band[index], 0);
            assert_eq!(history.log[index], -28 * (1 << 24));
            assert_eq!(history.log2[index], -28 * (1 << 24));
        }
    }
}

include!("residual_expectations.rs");

include!("sequence_expectations.rs");
#[test]
fn persistent_corpus_mono_residual_and_tdac_match_reference() {
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
    let mut tail = Vec::new();
    for i in 0..4 {
        let data = &SEQUENCE_PACKETS[i][1..];
        let mut rd = RangeDecoder::new(data).unwrap();
        assert!(!decoder.decode_silence(&mut rd).unwrap());
        if decoder.decode_post_filter(&mut rd).unwrap() {
            decoder.decode_post_filter_params(&mut rd).unwrap();
        }
        decoder.decode_transient_flag(&mut rd).unwrap();
        let intra = decoder.decode_intra(&mut rd).unwrap();
        let mut energy = decoder.energy_q24.band[..21].to_vec();
        decoder
            .decode_coarse_energy_q24(&mut rd, intra, data.len(), &mut energy)
            .unwrap();
        decoder.decode_tf(&mut rd, data.len()).unwrap();
        let spread = decoder.decode_spread(&mut rd).unwrap();
        let caps = decoder.compute_caps(3, 1);
        let (boost, _, _) = decoder
            .decode_band_boost(&mut rd, data.len() as i32 * 64, &caps)
            .unwrap();
        let trim = decoder
            .decode_allocation_trim(&mut rd, data.len() as i32 * 64, boost.iter().sum())
            .unwrap();
        let available = data.len() as i32 * 64 - rd.ec_tell_frac() as i32 - 1;
        let (allocation, _, _) = decoder
            .allocation_decode(&mut rd, available, &boost, trim, decoder.transient)
            .unwrap();
        decoder
            .decode_fine_energy_q24(&mut rd, &mut energy, &allocation.fine_energy_bits)
            .unwrap();
        let (mut shapes, trace) = decoder
            .mono_residual(
                &mut rd,
                &allocation,
                spread,
                data.len() as i32 * 64 - allocation.anti_collapse_rsv,
            )
            .unwrap();
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            SEQUENCE_ENTROPY[i],
            "full residual entropy packet {i}"
        );
        for (j, (&actual, &expected)) in shapes
            .concat()
            .iter()
            .zip(&SEQUENCE_RESIDUAL[i])
            .enumerate()
        {
            assert_eq!(actual, expected, "residual packet {i} bin {j}");
        }
        let anti = decoder
            .decode_anti_collapse_bit(&mut rd, allocation.anti_collapse_rsv > 0)
            .unwrap();
        let unused = data.len() * 8 - rd.ec_tell() as usize;
        decoder
            .decode_final_energy_q24(
                &mut rd,
                &mut energy,
                &allocation.fine_energy_bits,
                &allocation.fine_priority,
                unused,
            )
            .unwrap();
        assert_eq!(energy, SEQUENCE_ENERGY[i], "energy packet {i}");
        let masks: Vec<u8> = trace.iter().map(|entry| entry.mask as u8).collect();
        let pulses = allocation.shape_bits.map(|v| u16::try_from(v).unwrap());
        decoder
            .apply_anti_collapse_q24(&mut shapes, &energy, 0, &masks, &pulses, anti)
            .unwrap();
        for (j, (&actual, &expected)) in shapes
            .concat()
            .iter()
            .zip(&SEQUENCE_ANTI_COEFFICIENTS[i])
            .enumerate()
        {
            assert_eq!(actual, expected, "anti packet {i} bin {j}");
        }
        let mut spectrum = shapes.concat();
        spectrum.resize(960, 0);
        let frequency = decoder
            .denormalize_bands_q24(&spectrum, &energy, false)
            .unwrap();
        let samples = transform::synthesize(&frequency, decoder.transient, &mut tail);
        for (j, (&actual, &expected)) in samples.iter().zip(&SEQUENCE_SYNTHESIS[i]).enumerate() {
            assert_eq!(actual, expected, "TDAC packet {i} sample {j}");
        }
        decoder
            .energy_q24
            .update(energy, 1, decoder.transient, 0, 21);
        decoder.state.anti_collapse_state.seed = rd.get_range();
    }
}

include!("sequence_pcm.rs");
#[test]
fn persistent_raw_corpus_production_pcm_matches_exactly() {
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
    let mut output = crate::output::CeltOutput::default();
    for i in 0..4 {
        let data = &SEQUENCE_PACKETS[i][1..];
        let mut rd = RangeDecoder::new(data).unwrap();
        let frame = decoder.decode_celt_frame(&mut rd, data.len()).unwrap();
        let mut pcm = [0_i16; 960];
        assert_eq!(
            output
                .process(&frame.samples, 960, 1, 48000, &mut pcm)
                .unwrap(),
            960
        );
        for (j, (&actual, &expected)) in pcm.iter().zip(&SEQUENCE_PCM[i]).enumerate() {
            assert_eq!(actual, expected, "raw PCM packet {i} sample {j}");
        }
    }
}

include!("nb_expectations.rs");
include!("stereo_expectations.rs");
#[test]
#[allow(clippy::too_many_lines)]
fn actual_stereo_payload_stages_and_output_channel_pcm() {
    let mut shared_output_mismatches = Vec::new();
    let mut stage = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 960).unwrap();
    let mut decoders = [
        CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 960).unwrap(),
        CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 960).unwrap(),
    ];
    let mut q8_memory = [[0_i64; 2]; 2];
    let mut output = [
        crate::output::CeltOutput::default(),
        crate::output::CeltOutput::default(),
    ];
    for (case_index, case) in STEREO_CASES.iter().enumerate() {
        if case.reset {
            stage.reset();
            for decoder in &mut decoders {
                decoder.reset();
            }
            q8_memory = [[0_i64; 2]; 2];
            output = [
                crate::output::CeltOutput::default(),
                crate::output::CeltOutput::default(),
            ];
        }
        stage.set_frame_size(case.n).unwrap();
        stage.set_end_band(case.end);
        let data = &case.packet[1..];
        let mut rd = RangeDecoder::new(data).unwrap();
        assert!(!stage.decode_silence(&mut rd).unwrap());
        if stage.decode_post_filter(&mut rd).unwrap() {
            stage.decode_post_filter_params(&mut rd).unwrap();
        }
        stage.decode_transient_flag(&mut rd).unwrap();
        let intra = stage.decode_intra(&mut rd).unwrap();
        let mut energy = stage.energy_q24.band.to_vec();
        stage
            .decode_coarse_energy_q24(&mut rd, intra, data.len(), &mut energy)
            .unwrap();
        stage.decode_tf(&mut rd, data.len()).unwrap();
        let spread = stage.decode_spread(&mut rd).unwrap();
        let lm = stage.frame_duration_index();
        let caps = stage.compute_caps(u8::try_from(lm).unwrap(), 2);
        let (boost, _, _) = stage
            .decode_band_boost(&mut rd, data.len() as i32 * 64, &caps)
            .unwrap();
        let trim = stage
            .decode_allocation_trim(&mut rd, data.len() as i32 * 64, boost.iter().sum())
            .unwrap();
        let remaining = data.len() as i32 * 64 - rd.ec_tell_frac() as i32 - 1;
        let (allocation, intensity, dual) = stage
            .allocation_decode(&mut rd, remaining, &boost, trim, stage.transient)
            .unwrap();
        stage
            .decode_fine_energy_q24(&mut rd, &mut energy, &allocation.fine_energy_bits)
            .unwrap();
        let (shapes, _) = stage
            .stereo_residual(
                &mut rd,
                &allocation,
                spread,
                data.len() as i32 * 64 - allocation.anti_collapse_rsv,
                intensity,
                dual,
            )
            .unwrap();
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            case.entropy,
            "case {case_index} complete residual entropy"
        );
        let widths = crate::celt::constants::CELT_BINS_2_5MS.map(|n| usize::from(n) << lm);
        for (channel, channel_shapes) in shapes.iter().enumerate().take(2) {
            let mut offset = 0;
            for (band, (shape, &width)) in channel_shapes
                .iter()
                .zip(&widths)
                .enumerate()
                .take(case.end)
            {
                for (bin, &value) in shape.iter().enumerate() {
                    assert_eq!(
                        value,
                        case.residual[channel * case.n + offset + bin],
                        "case {case_index} channel {channel} band {band} bin {bin}"
                    );
                }
                offset += width;
            }
        }
        stage
            .decode_anti_collapse_bit(&mut rd, allocation.anti_collapse_rsv > 0)
            .unwrap();
        let unused = data.len() * 8 - rd.ec_tell() as usize;
        stage
            .decode_final_energy_q24(
                &mut rd,
                &mut energy,
                &allocation.fine_energy_bits,
                &allocation.fine_priority,
                unused,
            )
            .unwrap();
        stage
            .energy_q24
            .update(energy, 2, stage.transient, 0, case.end);
        stage.state.anti_collapse_state.seed = rd.get_range();
        for (index, channels) in [Channels::Mono, Channels::Stereo].into_iter().enumerate() {
            let decoder = &mut decoders[index];
            decoder.configure_synthesis_channels(channels);
            decoder.set_frame_size(case.n).unwrap();
            decoder.set_end_band(case.end);
            let mut rd = RangeDecoder::new(data).unwrap();
            let frame = decoder.decode_celt_frame(&mut rd, data.len()).unwrap();
            let expected = if index == 0 { case.mono } else { case.stereo };
            for (sample_index, &sample) in frame.samples.iter().enumerate() {
                let channel = sample_index % (index + 1);
                let filtered = (i64::from(sample) + q8_memory[index][channel])
                    .clamp(-536_870_911, 536_870_911);
                q8_memory[index][channel] = (27853 * filtered) >> 15;
                let reference_pcm =
                    ((((filtered + 8) >> 4) + 128) >> 8).clamp(-32768, 32767) as i16;
                assert_eq!(
                    reference_pcm,
                    expected[sample_index],
                    "Q8 case {case_index} output {} sample {sample_index}",
                    index + 1
                );
            }
            let mut pcm = vec![0_i16; case.n * (index + 1)];
            assert_eq!(frame.channels, channels);
            output[index]
                .process(&frame.samples, case.n, index + 1, 48000, &mut pcm)
                .unwrap();
            let expected = if index == 0 {
                case.contract_mono
            } else {
                case.contract_stereo
            };
            for (j, (&actual, &expected)) in pcm.iter().zip(expected).enumerate() {
                if actual != expected {
                    shared_output_mismatches.push((case_index, index + 1, j, actual, expected));
                }
            }
        }
    }
    assert!(
        shared_output_mismatches.is_empty(),
        "shared output {} differences; first {:?}",
        shared_output_mismatches.len(),
        shared_output_mismatches.first()
    );
}

#[test]
fn actual_nb_mono_stage_boundary() {
    for case_index in 0..2 {
        let mut stage = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        stage.set_frame_size(480).unwrap();
        stage.set_end_band(13);
        let data = &NB_PACKET[case_index][1..];
        let mut rd = RangeDecoder::new(data).unwrap();
        assert!(!stage.decode_silence(&mut rd).unwrap());
        if stage.decode_post_filter(&mut rd).unwrap() {
            stage.decode_post_filter_params(&mut rd).unwrap();
        }
        stage.decode_transient_flag(&mut rd).unwrap();
        let intra = stage.decode_intra(&mut rd).unwrap();
        let mut energy = stage.energy_q24.band[..21].to_vec();
        stage
            .decode_coarse_energy_q24(&mut rd, intra, data.len(), &mut energy)
            .unwrap();
        stage.decode_tf(&mut rd, data.len()).unwrap();
        let spread = stage.decode_spread(&mut rd).unwrap();
        let lm = stage.frame_duration_index();
        let caps = stage.compute_caps(u8::try_from(lm).unwrap(), 1);
        let (boost, _, _) = stage
            .decode_band_boost(&mut rd, data.len() as i32 * 64, &caps)
            .unwrap();
        let trim = stage
            .decode_allocation_trim(&mut rd, data.len() as i32 * 64, boost.iter().sum())
            .unwrap();
        let remaining = data.len() as i32 * 64 - rd.ec_tell_frac() as i32 - 1;
        let (allocation, intensity, dual) = stage
            .allocation_decode(&mut rd, remaining, &boost, trim, stage.transient)
            .unwrap();
        stage
            .decode_fine_energy_q24(&mut rd, &mut energy, &allocation.fine_energy_bits)
            .unwrap();
        let (shapes, _) = stage
            .mono_residual(
                &mut rd,
                &allocation,
                spread,
                data.len() as i32 * 64 - allocation.anti_collapse_rsv,
            )
            .unwrap();
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            NB_ENTROPY[case_index],
            "case {case_index} entropy"
        );
        let coefficients: Vec<i16> = shapes.iter().flatten().copied().collect();
        for (j, (&actual, &expected)) in coefficients
            .iter()
            .zip(&NB_RESIDUAL[case_index][..80])
            .enumerate()
        {
            assert_eq!(actual, expected, "case {case_index} coefficient {j}");
        }
        let anti = stage
            .decode_anti_collapse_bit(&mut rd, allocation.anti_collapse_rsv > 0)
            .unwrap();
        let unused = data.len() * 8 - rd.ec_tell() as usize;
        stage
            .decode_final_energy_q24(
                &mut rd,
                &mut energy,
                &allocation.fine_energy_bits,
                &allocation.fine_priority,
                unused,
            )
            .unwrap();
        assert_eq!(energy, NB_ENERGY[case_index], "case {case_index} energy");
        let _ = (anti, intensity, dual, NB_SYNTHESIS[case_index]);
    }
}

include!("hybrid_expectations.rs");
fn hybrid_prefix(case: &HybridCase) -> RangeDecoder {
    let mut rd = RangeDecoder::new(&case.packet[1..]).unwrap();
    for &(kind, args, expected) in case.prefix {
        let actual = match kind {
            0 => u32::from(rd.ec_dec_bit_logp(args[0]).unwrap()),
            1 => rd
                .ec_dec_icdf(
                    &args[1..]
                        .iter()
                        .map(|&v| u8::try_from(v).unwrap())
                        .collect::<Vec<_>>(),
                    args[0],
                )
                .unwrap(),
            2 => rd.ec_dec_uint(args[0]).unwrap(),
            3 => rd.ec_dec_bits(args[0]).unwrap(),
            _ => panic!("unknown oracle operation"),
        };
        assert_eq!(actual, expected, "SILK/redundancy entropy operation {kind}");
    }
    assert_eq!(
        (rd.get_range(), rd.get_value(), rd.ec_tell()),
        case.entry,
        "shared highband entry entropy"
    );
    rd
}
#[test]
fn frozen_hybrid_highband_shared_entropy_and_q12_synthesis() {
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
    for (case_index, case) in HYBRID_CASES.iter().enumerate() {
        let channels = if case.channels == 2 {
            Channels::Stereo
        } else {
            Channels::Mono
        };
        if case.reset {
            decoder = CeltDecoder::new(SampleRate::Hz48000, channels, case.n).unwrap();
        }
        decoder.configure_synthesis_channels(channels);
        decoder
            .set_output_rate(SampleRate::from_hz(case.rate).unwrap())
            .unwrap();
        let mut rd = hybrid_prefix(case);
        let frame = decoder
            .decode_hybrid_highband(&mut rd, case.packet.len() - 1, case.n, case.end)
            .unwrap();
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            case.final_entropy,
            "case {case_index} highband final entropy"
        );
        assert_eq!(frame.samples.len(), case.synthesis.len());
        for (i, (&actual, &expected)) in frame.samples.iter().zip(case.synthesis).enumerate() {
            assert_eq!(
                actual, expected,
                "case {case_index} highband Q12 sample {i}"
            );
        }
        let _ = (case.entropy, case.residual);
    }
}

fn assert_hybrid_residual_coefficients(
    case_index: usize,
    case: &HybridCase,
    shapes: &[Vec<Vec<i16>>],
    lm: usize,
) {
    let widths = crate::celt::constants::CELT_BINS_2_5MS.map(|n| usize::from(n) << lm);
    for (channel, channel_shapes) in shapes.iter().enumerate().take(case.channels) {
        let mut offset = 0;
        for (band, (shape, &width)) in channel_shapes
            .iter()
            .zip(&widths)
            .enumerate()
            .take(case.end)
        {
            if band >= 17 {
                for (bin, &actual) in shape.iter().enumerate() {
                    assert_eq!(
                        actual,
                        case.residual[channel * case.n + offset + bin],
                        "case {case_index} channel {channel} band {band} bin {bin}"
                    );
                }
            }
            offset += width;
        }
    }
}

#[test]
fn frozen_hybrid_highband_residual_stages() {
    let mut stage = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
    for (case_index, case) in HYBRID_CASES.iter().enumerate() {
        if case.reset {
            stage = CeltDecoder::new(
                SampleRate::Hz48000,
                if case.channels == 1 {
                    Channels::Mono
                } else {
                    Channels::Stereo
                },
                case.n,
            )
            .unwrap();
        }
        stage.set_frame_size(case.n).unwrap();
        stage.set_end_band(case.end);
        stage.set_start_band(17);
        let data = &case.packet[1..];
        let mut rd = hybrid_prefix(case);
        assert!(!stage.decode_silence(&mut rd).unwrap());
        stage.decode_transient_flag(&mut rd).unwrap();
        let intra = stage.decode_intra(&mut rd).unwrap();
        let mut energy = stage.energy_q24.band[..21 * case.channels].to_vec();
        stage
            .decode_coarse_energy_q24(&mut rd, intra, data.len(), &mut energy)
            .unwrap();
        stage.decode_tf(&mut rd, data.len()).unwrap();
        let spread = stage.decode_spread(&mut rd).unwrap();
        let lm = stage.frame_duration_index();
        let caps = stage.compute_caps(u8::try_from(lm).unwrap(), case.channels);
        let (boost, _, _) = stage
            .decode_band_boost(&mut rd, data.len() as i32 * 64, &caps)
            .unwrap();
        let trim = stage
            .decode_allocation_trim(&mut rd, data.len() as i32 * 64, boost.iter().sum())
            .unwrap();
        let remaining = data.len() as i32 * 64 - rd.ec_tell_frac() as i32 - 1;
        let (allocation, intensity, dual) = stage
            .allocation_decode(&mut rd, remaining, &boost, trim, stage.transient)
            .unwrap();
        stage
            .decode_fine_energy_q24(&mut rd, &mut energy, &allocation.fine_energy_bits)
            .unwrap();
        let shapes = if case.channels == 1 {
            let (shapes, _) = stage
                .mono_residual(
                    &mut rd,
                    &allocation,
                    spread,
                    data.len() as i32 * 64 - allocation.anti_collapse_rsv,
                )
                .unwrap();
            vec![shapes]
        } else {
            stage
                .stereo_residual(
                    &mut rd,
                    &allocation,
                    spread,
                    data.len() as i32 * 64 - allocation.anti_collapse_rsv,
                    intensity,
                    dual,
                )
                .unwrap()
                .0
        };
        assert_eq!(
            (rd.get_range(), rd.get_value(), rd.ec_tell()),
            case.entropy,
            "case {case_index} hybrid residual entropy"
        );
        assert_hybrid_residual_coefficients(case_index, case, &shapes, lm);
        stage
            .decode_anti_collapse_bit(&mut rd, allocation.anti_collapse_rsv > 0)
            .unwrap();
        let unused = data.len() * 8 - rd.ec_tell() as usize;
        stage
            .decode_final_energy_q24(
                &mut rd,
                &mut energy,
                &allocation.fine_energy_bits,
                &allocation.fine_priority,
                unused,
            )
            .unwrap();
        stage
            .energy_q24
            .update(energy, case.channels, stage.transient, 17, case.end);
        stage.state.anti_collapse_state.seed = rd.get_range();
    }
}
