//! Runtime policy for the native encoder. Control validation never consumes PCM.
use crate::{Bandwidth, Error, Result};

/// Requested native coding mode. `Auto` is a policy, never a packet TOC mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderMode {
    /// Select a supported mode using bitrate and bandwidth with hysteresis.
    Auto,
    /// Transform coding.
    Celt,
    /// Linear-prediction coding.
    Silk,
    /// Shared-stream SILK low band and CELT high band.
    Hybrid,
}

/// Frame-boundary runtime controls, independent of input framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderControl {
    /// Maximum packet bitrate, including framing, in bits per second.
    pub bitrate: u32,
    /// Requested encoded bandwidth (not the input sample rate).
    pub bandwidth: Bandwidth,
    /// Requested coding mode or adaptive policy.
    pub mode: EncoderMode,
}

#[derive(Clone)]
pub(super) struct Adaptive {
    pub control: EncoderControl,
    pub active: EncoderMode,
    // Explicit Hybrid may share adaptive encoding state, but not its public controls.
    pub fixed_controls: bool,
    pub next_celt: bool,
    #[cfg(feature = "hybrid")]
    pub tail: Vec<f32>,
    pub end: usize,
}

#[cfg(feature = "hybrid")]
impl Adaptive {
    pub(super) const fn new(bitrate: u32) -> Self {
        Self {
            control: EncoderControl {
                bitrate,
                bandwidth: Bandwidth::Fullband,
                mode: EncoderMode::Auto,
            },
            active: EncoderMode::Celt,
            fixed_controls: false,
            next_celt: false,
            tail: Vec::new(),
            end: 21,
        }
    }

    pub(super) fn target(&self) -> EncoderMode {
        match self.control.mode {
            EncoderMode::Auto if self.control.bandwidth == Bandwidth::Wideband => EncoderMode::Silk,
            EncoderMode::Auto
                if self.control.bandwidth == Bandwidth::SuperWideband
                    && self.control.bitrate <= 128_000 =>
            {
                EncoderMode::Hybrid
            }
            EncoderMode::Auto if self.control.bitrate >= 224_000 => EncoderMode::Celt,
            EncoderMode::Auto
                if self.control.bitrate < 160_000 && self.active == EncoderMode::Hybrid =>
            {
                EncoderMode::Hybrid
            }
            EncoderMode::Auto if self.control.bitrate < 160_000 => EncoderMode::Celt,
            EncoderMode::Auto if self.control.bitrate <= 192_000 => EncoderMode::Hybrid,
            EncoderMode::Auto => self.active,
            mode => mode,
        }
    }
}

pub(super) fn unavailable(message: &str) -> Error {
    Error::Unsupported(message.into())
}

pub(super) fn validate_celt(control: EncoderControl, minimum: u32) -> Result<()> {
    if !(minimum..=256_000).contains(&control.bitrate) {
        return Err(unavailable(
            "CELT bitrate outside the validated duration budget",
        ));
    }
    let predictive = control.mode == EncoderMode::Silk
        || (control.mode == EncoderMode::Auto && control.bandwidth == Bandwidth::Wideband);
    let transform_bandwidth = matches!(
        control.bandwidth,
        Bandwidth::Fullband | Bandwidth::SuperWideband
    );
    if (predictive && control.bandwidth != Bandwidth::Wideband)
        || (!predictive && !transform_bandwidth)
    {
        return Err(unavailable(
            "only Wideband SILK and Fullband CELT/hybrid are qualified control targets",
        ));
    }
    Ok(())
}

#[cfg(test)]
// Preserve the qualification signals' separate multiply/add rounding: fused
// arithmetic would change the PCM fixtures being compared against the oracle.
#[allow(clippy::suboptimal_flops)]
mod tests {
    struct Pool48Scope(bool);
    impl Pool48Scope {
        fn enter() -> Self {
            Self(super::super::hybrid::POOL48.replace(true))
        }
    }
    impl Drop for Pool48Scope {
        fn drop(&mut self) {
            super::super::hybrid::POOL48.set(self.0);
        }
    }

    #[test]
    #[ignore = "same committed production snapshot versus pool48 at exact short-frame boundaries"]
    fn prototype_pool48_common_snapshot_diagnostic() {
        let media: Vec<f32> =
            std::fs::read(std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").unwrap())
                .unwrap()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .collect();
        for shape in [2, 3] {
            let config = EncoderConfig {
                frame_size: 480,
                bitrate: 64000,
                ..EncoderConfig::default()
            };
            let mut production = Encoder::new_auto(config).unwrap();
            for frame in 0..18 {
                let (mode, bitrate) = if (4..12).contains(&frame) {
                    (EncoderMode::Hybrid, 32000)
                } else if (16..24).contains(&frame) {
                    (EncoderMode::Auto, 64000)
                } else {
                    (EncoderMode::Celt, 64000)
                };
                production
                    .set_control(EncoderControl {
                        mode,
                        bitrate,
                        bandwidth: Bandwidth::Fullband,
                    })
                    .unwrap();
                let pcm = candidate_hybrid_shape(shape, frame, 480, &media);
                if [4, 6, 16].contains(&frame) {
                    let mut pool = production.clone();
                    eprintln!(
                        "COMMON_SNAPSHOT shape={shape} frame={frame} mode={mode:?} bitrate={bitrate}"
                    );
                    super::super::hybrid::POOL_DIAGNOSE_SKIP.set(0);
                    super::super::hybrid::POOL_DIAGNOSE.set(12);
                    let _scope = Pool48Scope::enter();
                    let candidate = pool.encode(&pcm).unwrap();
                    eprintln!(
                        "COMMON_PACKET shape={shape} frame={frame} pool_bytes={}",
                        candidate.len()
                    );
                    super::super::hybrid::POOL_DIAGNOSE.set(0);
                }
                if [4, 6, 16].contains(&frame) {
                    super::super::hybrid::POOL_DIAGNOSE.set(1);
                }
                let packet = production.encode(&pcm).unwrap();
                super::super::hybrid::POOL_DIAGNOSE.set(0);
                eprintln!(
                    "COMMON_PRODUCTION shape={shape} frame={frame} packet_bytes={}",
                    packet.len()
                );
            }
        }
    }

    #[test]
    #[ignore = "full versus pool48 same-state winner diagnostic; selected unchanged layout assertion"]
    fn prototype_pool48_winner_diagnostic() {
        let _scope = Pool48Scope::enter();
        super::super::hybrid::POOL_DIAGNOSE.set(24);
        super::super::hybrid::POOL_DIAGNOSE_SKIP.set(0);
        reference_practical_layout_quality();
        super::super::hybrid::POOL_DIAGNOSE.set(0);
    }

    #[test]
    #[ignore = "test-only pool48; unchanged public assertions, not production acceptance"]
    fn prototype_pool48_public_matrix() {
        let _scope = Pool48Scope::enter();
        reference_public_hybrid_auto_matrix();
    }

    #[test]
    #[ignore = "test-only pool48; unchanged 40-layout assertions"]
    fn prototype_pool48_layout_matrix() {
        let _scope = Pool48Scope::enter();
        reference_practical_layout_quality();
    }

    #[test]
    #[ignore = "test-only pool48; unchanged 24 cold/practical assertions"]
    fn prototype_pool48_cold_matrix() {
        let _scope = Pool48Scope::enter();
        reference_public_practical_rd_quality();
    }

    use super::super::{Encoder, EncoderConfig};
    #[cfg(feature = "hybrid")]
    use super::{Bandwidth, EncoderControl, EncoderMode};
    use crate::Channels;

    fn packet_wire(packets: &[Vec<u8>]) -> Vec<u8> {
        let mut wire = Vec::new();
        for packet in packets {
            wire.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
            wire.extend_from_slice(packet);
        }
        wire
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle required; candidate only, public hybrid guard unchanged"]
    fn reference_practical_hybrid_split_candidates() {
        run_hybrid_candidates(None, &[]);
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle and real-media PCM required; public hybrid guard unchanged"]
    fn reference_practical_hybrid_policy_quality() {
        qualify_hybrid_policy(3);
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle and real-media PCM required; experimental split4, public guards unchanged"]
    fn reference_practical_hybrid_policy_split4_quality() {
        qualify_hybrid_policy(4);
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle and real-media PCM required; actual public constructor"]
    fn reference_practical_hybrid_constructor_quality() {
        qualify_hybrid_policy(5);
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle and real-media PCM required; intermediate public-constructor rates"]
    fn reference_practical_hybrid_intermediate_quality() {
        qualify_hybrid_policy(6);
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "explicit oracle and real-media PCM required; persistent actual Auto controls"]
    fn reference_practical_auto_steady_quality() {
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        let bytes = std::fs::read(
            std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("real-media PCM required"),
        )
        .unwrap();
        assert_eq!(bytes.len() % 4, 0);
        let media: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        assert!(media.len() >= 32 * 960 * 2);
        for frame_size in [480, 960] {
            for shape in 0..4 {
                qualify_auto_steady_case(frame_size, shape, &media, &oracle);
            }
        }
    }

    #[cfg(feature = "hybrid")]
    fn qualify_auto_steady_case(
        frame_size: usize,
        shape: usize,
        media: &[f32],
        oracle: &std::ffi::OsStr,
    ) {
        let mut encoder = Encoder::new_auto(EncoderConfig {
            bitrate: 192_000,
            frame_size,
            ..EncoderConfig::default()
        })
        .unwrap();
        let mut source = Vec::new();
        let mut packets = Vec::new();
        for frame in 0..32 {
            let bitrate = match frame {
                0..=3 => 192_000,
                4..=15 => 32_000,
                16..=27 => 64_000,
                _ => 224_000,
            };
            encoder
                .set_control(EncoderControl {
                    bitrate,
                    bandwidth: Bandwidth::Fullband,
                    mode: EncoderMode::Auto,
                })
                .unwrap();
            let pcm = candidate_hybrid_shape(shape, frame, frame_size, media);
            source.extend_from_slice(&pcm);
            let packet = encoder.encode(&pcm).unwrap();
            assert!(packet.len() <= bitrate as usize * frame_size / 384_000);
            if (4..28).contains(&frame) {
                assert_eq!(encoder.active_mode(), EncoderMode::Hybrid);
            }
            packets.push(packet);
        }
        assert_eq!(encoder.active_mode(), EncoderMode::Celt);
        packets.extend(encoder.finish().unwrap());
        let decoded = decode_candidate_hybrid(&packets, oracle);
        assert_auto_native_parity(&packets, frame_size, &decoded);
        assert_eq!(decoded.len(), packets.len() * frame_size * 2);
        let aligned = &decoded[encoder.lookahead() * 2..encoder.lookahead() * 2 + source.len()];
        // Keep both low-rate segments and their bitrate boundaries in acceptance.
        for (start, end) in [(4, 16), (16, 28)] {
            let range = start * frame_size * 2..end * frame_size * 2;
            let signal: f64 = source[range.clone()]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = source[range.clone()]
                .iter()
                .zip(&aligned[range])
                .map(|(&a, &b)| f64::from(a - b).powi(2))
                .sum();
            let snr = 10.0 * (signal / error).log10();
            eprintln!(
                "Auto steady shape={shape} duration={frame_size} frames={start}..{end} SNR={snr}"
            );
            assert!(snr.is_finite() && snr > 3.0, "Auto steady quality={snr}");
        }
    }

    #[cfg(feature = "hybrid")]
    #[test]
    #[ignore = "experimental no-redundancy boundaries; explicit oracle and real-media PCM required"]
    fn reference_practical_unredundant_transition_quality() {
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        let bytes = std::fs::read(
            std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("real-media PCM required"),
        )
        .unwrap();
        assert_eq!(bytes.len() % 4, 0);
        let media: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        assert!(media.len() >= 32 * 960 * 2);
        let mut failures = Vec::new();
        for frame_size in [480, 960] {
            let shapes = if PUBLIC_RD_QUALIFIER.get() { 5 } else { 4 };
            for shape in 0..shapes {
                failures.extend(qualify_unredundant_case(frame_size, shape, &media, &oracle));
            }
        }
        assert!(
            failures.is_empty(),
            "unqualified no-redundancy transitions: {failures:?}"
        );
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "explicit oracle and real-media PCM required; budget frontier only, no public policy change"]
    fn reference_unredundant_prefix_budget_frontier() {
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        let bytes = std::fs::read(
            std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("real-media PCM required"),
        )
        .unwrap();
        assert_eq!(bytes.len() % 4, 0);
        let media: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        assert!(media.len() >= 32 * 960 * 2);
        for prefix_bytes in [31, 33, 35] {
            let failures =
                qualify_unredundant_case_with_budget(480, 3, &media, &oracle, Some(prefix_bytes));
            eprintln!(
                "prefix frontier bytes={prefix_bytes} suffix_reserve={} failures={failures:?}",
                39 - prefix_bytes
            );
        }
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "explicit frozen oracle and real PCM required; candidate layouts, public guards unchanged"]
    #[allow(clippy::too_many_lines)] // Oracle matrix keeps each encode/decode quality gate together.
    fn reference_unredundant_mono_swb_matrix() {
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        let bytes = std::fs::read(
            std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("real PCM required"),
        )
        .unwrap();
        assert_eq!(bytes.len() % 4, 0);
        let media: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        assert!(media.len() >= 32 * 960 * 2);
        let mut failures = Vec::new();
        for channels in [Channels::Mono, Channels::Stereo] {
            let count = super::super::channel_count(channels);
            for bandwidth in [Bandwidth::SuperWideband, Bandwidth::Fullband] {
                for size in [480, 960] {
                    for rate in [32_000, 48_000, 64_000] {
                        for shape in 0..4 {
                            let label =
                                format!("{channels:?}/{bandwidth:?}/{size}/{rate}/shape{shape}");
                            let mut encoder = Encoder::new_auto(EncoderConfig {
                                channels,
                                frame_size: size,
                                bitrate: 64_000,
                                ..EncoderConfig::default()
                            })
                            .unwrap();
                            let mut source = Vec::new();
                            let mut packets = Vec::new();
                            let mut encode_error = None;
                            for frame in 0..32 {
                                let stereo = candidate_hybrid_shape(shape, frame, size, &media);
                                let pcm = if count == 2 {
                                    stereo
                                } else {
                                    stereo
                                        .as_chunks::<2>()
                                        .0
                                        .iter()
                                        .map(|x| f32::midpoint(x[0], x[1]))
                                        .collect()
                                };
                                source.extend_from_slice(&pcm);
                                let hybrid = (4..12).contains(&frame) || (16..24).contains(&frame);
                                let bitrate = if hybrid { rate } else { 64_000 };
                                match encoder.encode_unredundant_candidate(
                                    &pcm,
                                    EncoderControl {
                                        bitrate,
                                        bandwidth,
                                        mode: if hybrid {
                                            EncoderMode::Hybrid
                                        } else {
                                            EncoderMode::Celt
                                        },
                                    },
                                ) {
                                    Ok(packet) => {
                                        assert!(packet.len() <= bitrate as usize * size / 384_000);
                                        packets.push(packet);
                                    }
                                    Err(error) => {
                                        encode_error =
                                            Some(format!("{label} frame{frame}: {error}"));
                                        break;
                                    }
                                }
                            }
                            if let Some(error) = encode_error {
                                failures.push(error);
                                continue;
                            }
                            match encoder.finish() {
                                Ok(tail) => packets.extend(tail),
                                Err(error) => {
                                    failures.push(format!("{label} finish: {error}"));
                                    continue;
                                }
                            }
                            let decoded = decode_candidate_layout(&packets, &oracle, count);
                            assert_eq!(decoded.len(), packets.len() * size * count);
                            let mut native =
                                crate::Decoder::new(crate::SampleRate::Hz48000, channels).unwrap();
                            for (packet, expected) in
                                packets.iter().zip(decoded.chunks_exact(size * count))
                            {
                                let mut pcm = vec![0_i16; size * count];
                                let samples = match native.decode(Some(packet), &mut pcm, false) {
                                    Ok(samples) => samples,
                                    Err(error) => {
                                        failures.push(format!("{label} native decode: {error}"));
                                        break;
                                    }
                                };
                                assert_eq!(samples, size);
                                let peak = pcm
                                    .iter()
                                    .zip(expected)
                                    .map(|(&a, &b)| {
                                        (i32::from(a) - (b * 32768.0).round() as i32).abs()
                                    })
                                    .max()
                                    .unwrap();
                                assert!(peak <= 2, "{label} C parity={peak}");
                            }
                            let delay = encoder.lookahead() * count;
                            let aligned = &decoded[delay..delay + source.len()];
                            for (start, end) in
                                [(4, 6), (6, 12), (12, 14), (16, 18), (18, 24), (24, 26)]
                            {
                                let range = start * size * count..end * size * count;
                                let signal: f64 = source[range.clone()]
                                    .iter()
                                    .map(|&x| f64::from(x).powi(2))
                                    .sum();
                                let error: f64 = source[range.clone()]
                                    .iter()
                                    .zip(&aligned[range.clone()])
                                    .map(|(&a, &b)| f64::from(a - b).powi(2))
                                    .sum();
                                let snr = 10.0 * (signal / error).log10();
                                let pass = if signal <= 1e-12 {
                                    (error / range.len() as f64).sqrt() <= 1.0 / 32768.0
                                } else {
                                    snr.is_finite() && snr > 3.0
                                };
                                eprintln!(
                                    "layout {label} region={start}..{end} SNR={snr} pass={pass}"
                                );
                                if !pass {
                                    failures.push(format!("{label} {start}..{end} SNR={snr}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "unqualified candidate layouts: {failures:?}"
        );
    }

    #[cfg(feature = "hybrid")]
    fn qualify_unredundant_case(
        frame_size: usize,
        shape: usize,
        media: &[f32],
        oracle: &std::ffi::OsStr,
    ) -> Vec<String> {
        qualify_unredundant_case_with_budget(frame_size, shape, media, oracle, None)
    }

    #[cfg(feature = "hybrid")]
    std::thread_local! {
        static CLOSED_LOOP_PROOF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static PUBLIC_RD_QUALIFIER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "bounded native closed-loop proof; explicit frozen oracle and PCM required"]
    fn reference_unredundant_closed_loop_energy_proof() {
        CLOSED_LOOP_PROOF.set(true);
        reference_practical_unredundant_transition_quality();
        CLOSED_LOOP_PROOF.set(false);
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "new layouts internal qualification only; public guards remain closed; explicit frozen oracle and PCM"]
    fn reference_practical_layout_quality() {
        let oracle = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle");
        let media: Vec<f32> =
            std::fs::read(std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("real PCM"))
                .unwrap()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .collect();
        // Optional exact cell selection: RATE:CHANNELS:DURATION_MS:SHAPE.
        // Absence always executes the complete mandatory matrix.
        let cell = std::env::var("MOOSICBOX_OPUS_LAYOUT_CELL")
            .ok()
            .map(|value| {
                let values: Vec<usize> = value
                    .split(':')
                    .map(|part| part.parse().expect("numeric layout cell"))
                    .collect();
                let cell: [usize; 4] = values
                    .try_into()
                    .expect("layout cell requires rate:channels:duration_ms:shape");
                assert!(
                    matches!(cell[0], 24000 | 48000)
                        && matches!(cell[1], 1 | 2)
                        && matches!(cell[2], 10 | 20)
                        && cell[3] < 5,
                    "unsupported layout cell"
                );
                cell
            });
        let mut executed = 0;
        let mut failures = Vec::new();
        for rate in [crate::SampleRate::Hz24000, crate::SampleRate::Hz48000] {
            for channels in [Channels::Mono, Channels::Stereo] {
                for duration in [10, 20] {
                    for shape in 0..5 {
                        let current = [
                            rate as usize,
                            super::super::channel_count(channels),
                            duration,
                            shape,
                        ];
                        if cell.is_some_and(|selected| selected != current) {
                            continue;
                        }
                        executed += 1;
                        failures.extend(qualify_practical_layout(
                            rate, channels, duration, shape, &media, &oracle,
                        ));
                    }
                }
            }
        }
        assert_eq!(executed, if cell.is_some() { 1 } else { 40 });
        eprintln!(
            "layout qualification executed={executed} filtered={}",
            cell.is_some()
        );
        assert!(failures.is_empty(), "unqualified layouts: {failures:?}");
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Complete native-clock stream, oracle parity, budgets, tails and quality windows.
    fn qualify_practical_layout(
        rate: crate::SampleRate,
        channels: Channels,
        duration: usize,
        shape: usize,
        media: &[f32],
        oracle: &std::ffi::OsStr,
    ) -> Vec<String> {
        let count = super::super::channel_count(channels);
        let factor = if rate == crate::SampleRate::Hz24000 {
            2
        } else {
            1
        };
        let size = 48 * duration / factor;
        let config = EncoderConfig {
            sample_rate: rate,
            channels,
            bitrate: 64_000,
            frame_size: size,
        };
        // This single constructor hook admits candidate layouts without opening
        // ordinary constructors. Controls/packets/finish use the public API.
        let mut encoder = Encoder::new_auto_layout(config, true).unwrap();
        assert_eq!(encoder.lookahead(), if factor == 2 { 204 } else { 384 });
        let mut packets = Vec::new();
        let mut source = Vec::new();
        for frame in 0..32 {
            let control = EncoderControl {
                mode: if (4..12).contains(&frame) {
                    EncoderMode::Hybrid
                } else if (16..24).contains(&frame) {
                    EncoderMode::Auto
                } else {
                    EncoderMode::Celt
                },
                bitrate: if (4..12).contains(&frame) {
                    32_000
                } else {
                    64_000
                },
                bandwidth: Bandwidth::Fullband,
            };
            let stereo = candidate_hybrid_shape(
                if shape == 4 { 3 } else { shape },
                frame,
                size * factor,
                media,
            );
            let pcm: Vec<_> = stereo
                .as_chunks::<2>()
                .0
                .iter()
                .step_by(factor)
                .flat_map(|pair| {
                    let gain = if shape == 4 { 0.01 } else { 1.0 };
                    if count == 1 {
                        vec![gain * f32::midpoint(pair[0], pair[1])]
                    } else {
                        vec![gain * pair[0], gain * pair[1]]
                    }
                })
                .collect();
            encoder.set_control(control).unwrap();
            let packet = encoder.encode(&pcm).unwrap();
            assert_eq!(packet[0] & 4 != 0, count == 2, "actual TOC channels");
            assert!(packet.len() <= control.bitrate as usize * size * factor / 384_000);
            source.extend(pcm);
            packets.push(packet);
        }
        let tail = encoder.finish().unwrap();
        assert_eq!(tail.len(), encoder.lookahead().div_ceil(size));
        assert!(
            tail.iter()
                .all(|p| p.len() <= 64_000 * size * factor / 384_000)
        );
        packets.extend(tail);
        let decoded = decode_candidate_rate(&packets, oracle, count, rate as u32);
        let mut native = crate::Decoder::new(rate, channels).unwrap();
        for (packet, expected) in packets.iter().zip(decoded.chunks_exact(size * count)) {
            let mut output = vec![0_i16; size * count];
            assert_eq!(
                native.decode(Some(packet), &mut output, false).unwrap(),
                size
            );
            for (&a, &b) in output.iter().zip(expected) {
                assert!((f32::from(a) / 32768.0 - b).abs() <= 1.0 / 32768.0);
            }
        }
        trace_layout_lowband(&packets, &decoded, &source, config, encoder.lookahead());
        let aligned =
            &decoded[encoder.lookahead() * count..encoder.lookahead() * count + source.len()];
        let mut failures = Vec::new();
        for (begin, end) in [(4, 6), (6, 12), (12, 14), (16, 18), (18, 24), (24, 26)] {
            let range = begin * size * count..end * size * count;
            let signal: f64 = source[range.clone()]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = source[range.clone()]
                .iter()
                .zip(&aligned[range.clone()])
                .map(|(&a, &b)| f64::from(a - b).powi(2))
                .sum();
            let snr = 10.0 * (signal / error).log10();
            eprintln!(
                "layout rate={rate:?} channels={channels:?} duration={duration} shape={shape} boundary={begin} SNR={snr} error={error}"
            );
            if !(if signal == 0.0 {
                error <= range.len() as f64 / (32768.0 * 32768.0)
            } else {
                snr > 3.0
            }) {
                failures.push(format!(
                    "{rate:?}/{channels:?}/{duration}/shape{shape}/boundary{begin}: {snr}"
                ));
            }
        }
        failures
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Test-only native-clock partition and ideal/coded synthesis decomposition.
    fn trace_layout_lowband(
        packets: &[Vec<u8>],
        decoded: &[f32],
        source: &[f32],
        config: EncoderConfig,
        lookahead: usize,
    ) {
        if std::env::var_os("MOOSICBOX_OPUS_RD_TRACE").is_none() {
            return;
        }
        let count = super::super::channel_count(config.channels);
        let size = config.frame_size;
        let internal = size * super::super::rate_factor(config.sample_rate) / 3;
        for boundary in [4, 16] {
            let mut silk = crate::silk::SilkDecoder::new(
                crate::SampleRate::Hz16000,
                config.channels,
                (internal / 16) as u8,
            )
            .unwrap();
            let mut converters: Vec<_> = (0..count)
                .map(|_| {
                    crate::silk::resampler::SilkResampler::new(16000, config.sample_rate as u32)
                        .unwrap()
                })
                .collect();
            let mut ideal_filter = super::super::hybrid::LowBand::new(count).unwrap();
            let factor = super::super::rate_factor(config.sample_rate);
            let mut ideal_input =
                (factor != 1).then(|| super::super::resample::Interpolator::new(count, factor));
            for warm in source.chunks_exact(size * count).take(boundary) {
                let upsampled = Encoder::upsample_hybrid_input(&mut ideal_input, warm);
                ideal_filter.process(upsampled.as_deref().unwrap_or(warm));
            }
            let mut ideal_converters: Vec<_> = (0..count)
                .map(|_| {
                    crate::silk::resampler::SilkResampler::new(16000, config.sample_rate as u32)
                        .unwrap()
                })
                .collect();
            let mut ideal_history = vec![0_i16; count];
            for (frame, packet) in packets.iter().enumerate().skip(boundary).take(8) {
                let input = &source[frame * size * count..(frame + 1) * size * count];
                let upsampled = Encoder::upsample_hybrid_input(&mut ideal_input, input);
                let ideal_low = ideal_filter.process(upsampled.as_deref().unwrap_or(input));
                let ideal_planar: Vec<_> = ideal_converters
                    .iter_mut()
                    .enumerate()
                    .map(|(c, converter)| {
                        let mut samples: Vec<_> = ideal_low
                            .iter()
                            .skip(c)
                            .step_by(count)
                            .map(|x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
                            .collect();
                        let last = *samples.last().unwrap();
                        samples.rotate_right(1);
                        samples[0] = ideal_history[c];
                        ideal_history[c] = last;
                        converter.process(&samples).unwrap()
                    })
                    .collect();
                let mut range = crate::range::RangeDecoder::new(&packet[1..]).unwrap();
                silk.begin_packet();
                silk.decode_header_bits(&mut range, count == 2).unwrap();
                let mut low = vec![0_i16; internal * count];
                silk.decode_silk_frame(&mut range, true, &mut low).unwrap();
                let planar: Vec<_> = converters
                    .iter_mut()
                    .enumerate()
                    .map(|(c, converter)| {
                        converter
                            .process(
                                &low.iter()
                                    .skip(c)
                                    .step_by(count)
                                    .copied()
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap()
                    })
                    .collect();
                assert!(planar.iter().all(|p| p.len() == size));
                if frame < boundary + 3 {
                    let fade = config.sample_rate as usize / 400;
                    let mut cuts = vec![0, fade, 2 * fade, lookahead, size];
                    cuts.sort_unstable();
                    cuts.dedup();
                    for cut in cuts.windows(2) {
                        let (begin, end) = (cut[0], cut[1]);
                        let (mut signal, mut low_error, mut full_error, mut added) =
                            (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
                        let mut ideal_error = 0.0_f64;
                        for i in begin..end {
                            for c in 0..count {
                                let t =
                                    f64::from(source[(frame * size + i - lookahead) * count + c]);
                                let l = f64::from(planar[c][i]) / 32768.0;
                                let y = f64::from(decoded[(frame * size + i) * count + c]);
                                signal += t * t;
                                ideal_error +=
                                    (t - f64::from(ideal_planar[c][i]) / 32768.0).powi(2);
                                low_error += (t - l).powi(2);
                                full_error += (t - y).powi(2);
                                added += (y - l).powi(2);
                            }
                        }
                        eprintln!(
                            "entry partition boundary={boundary} frame={frame} native={begin}..{end} source={}..{} signal={signal} ideal_error={ideal_error} low_error={low_error} full_error={full_error} added={added} cross={}",
                            frame * size + begin - lookahead,
                            frame * size + end - lookahead,
                            full_error - low_error - added
                        );
                    }
                }
                let mut low_error = 0.0_f64;
                let mut full_error = 0.0_f64;
                let mut added_energy = 0.0_f64;
                let mut peak = 0_i32;
                for i in 0..size {
                    for c in 0..count {
                        let l = f64::from(planar[c][i]) / 32768.0;
                        let y = f64::from(decoded[(frame * size + i) * count + c]);
                        let t = f64::from(source[(frame * size + i - lookahead) * count + c]);
                        low_error += (t - l).powi(2);
                        full_error += (t - y).powi(2);
                        added_energy += (y - l).powi(2);
                        peak = peak.max(i32::from(planar[c][i]).abs());
                    }
                }
                eprintln!(
                    "layout decomposition rate={:?} channels={:?} boundary={boundary} frame={frame} prefix_bits={} low_error={low_error} full_error={full_error} added_energy={added_energy} low_peak={peak}",
                    config.sample_rate,
                    config.channels,
                    range.ec_tell()
                );
            }
        }
    }

    // Parse the actual cold packets, retaining prefix/resampler history. The
    // difference full-minus-prefix includes CELT and its normative entry fade.
    #[cfg(feature = "hybrid")]
    fn trace_public_entry(
        packets: &[Vec<u8>],
        decoded: &[f32],
        source: &[f32],
        config: EncoderConfig,
        lookahead: usize,
    ) {
        if std::env::var_os("MOOSICBOX_OPUS_PUBLIC_ENTRY_TRACE").is_none() {
            return;
        }
        let count = super::super::channel_count(config.channels);
        let size = config.frame_size;
        let internal = size * super::super::rate_factor(config.sample_rate) / 3;
        let mut silk = crate::silk::SilkDecoder::new(
            crate::SampleRate::Hz16000,
            config.channels,
            (internal / 16) as u8,
        )
        .unwrap();
        let mut converters: Vec<_> = (0..count)
            .map(|_| {
                crate::silk::resampler::SilkResampler::new(16000, config.sample_rate as u32)
                    .unwrap()
            })
            .collect();
        for (frame, packet) in packets.iter().take(3).enumerate() {
            eprintln!("public entry packet={frame} lookahead={lookahead} bytes={packet:02x?}");
            if !(12..16).contains(&(packet[0] >> 3)) {
                return;
            }
            let mut range = crate::range::RangeDecoder::new(&packet[1..]).unwrap();
            silk.begin_packet();
            silk.decode_header_bits(&mut range, count == 2).unwrap();
            let mut low = vec![0_i16; internal * count];
            silk.decode_silk_frame(&mut range, true, &mut low).unwrap();
            let planar: Vec<_> = converters
                .iter_mut()
                .enumerate()
                .map(|(c, converter)| {
                    converter
                        .process(
                            &low.iter()
                                .skip(c)
                                .step_by(count)
                                .copied()
                                .collect::<Vec<_>>(),
                        )
                        .unwrap()
                })
                .collect();
            assert!(planar.iter().all(|p| p.len() == size));
            let mut cuts = vec![
                0,
                config.sample_rate as usize / 400,
                lookahead.min(size),
                size,
            ];
            cuts.sort_unstable();
            cuts.dedup();
            for cut in cuts.windows(2) {
                let mut low_error = 0.0_f64;
                let mut full_error = 0.0_f64;
                let mut added_energy = 0.0_f64;
                let mut cross = 0.0_f64;
                let mut peaks = [0.0_f64; 3];
                for (c, channel) in planar.iter().enumerate() {
                    for (i, &sample) in channel.iter().enumerate().take(cut[1]).skip(cut[0]) {
                        let position = frame * size + i;
                        let target = position
                            .checked_sub(lookahead)
                            .map_or(0.0, |p| f64::from(source[p * count + c]));
                        let prefix = f64::from(sample) / 32768.0;
                        let full = f64::from(decoded[position * count + c]);
                        let added = full - prefix;
                        low_error += (target - prefix).powi(2);
                        full_error += (target - full).powi(2);
                        added_energy += added.powi(2);
                        cross += 2.0 * (target - prefix) * added;
                        for (peak, value) in peaks.iter_mut().zip([prefix, added, full]) {
                            *peak = peak.max(value.abs());
                        }
                    }
                }
                eprintln!(
                    "public entry frame={frame} cut={cut:?} prefix_bits={} low_error={low_error} full_error={full_error} added_energy={added_energy} cross={cross} peaks={peaks:?}",
                    range.ec_tell()
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "hybrid")]
    fn ordinary_practical_constructor_and_layout_guards() {
        for frame_size in [480, 960] {
            for bitrate in [32_000, 64_000] {
                let config = EncoderConfig {
                    bitrate,
                    frame_size,
                    ..EncoderConfig::default()
                };
                let mut explicit = Encoder::new_hybrid(config).unwrap();
                let mut automatic = Encoder::new_auto(config).unwrap();
                automatic
                    .set_control(EncoderControl {
                        mode: EncoderMode::Hybrid,
                        bitrate,
                        bandwidth: Bandwidth::Fullband,
                    })
                    .unwrap();
                let pcm = candidate_hybrid_shape(1, 0, frame_size, &[]);
                assert_eq!(
                    explicit.encode(&pcm).unwrap(),
                    automatic.encode(&pcm).unwrap()
                );
                assert_eq!(explicit.active_mode(), EncoderMode::Hybrid);
                assert_eq!(explicit.finish().unwrap(), automatic.finish().unwrap());
                for (sample_rate, channels, size) in [
                    (crate::SampleRate::Hz24000, Channels::Mono, frame_size / 2),
                    (crate::SampleRate::Hz24000, Channels::Stereo, frame_size / 2),
                    (crate::SampleRate::Hz48000, Channels::Mono, frame_size),
                ] {
                    let mut admitted = Encoder::new_hybrid(EncoderConfig {
                        sample_rate,
                        channels,
                        frame_size: size,
                        ..config
                    })
                    .unwrap();
                    let factor = super::super::rate_factor(sample_rate);
                    let pcm: Vec<_> = candidate_hybrid_shape(1, 0, frame_size, &[])
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .step_by(factor)
                        .flat_map(|pair| {
                            if channels == Channels::Mono {
                                vec![f32::midpoint(pair[0], pair[1])]
                            } else {
                                pair.to_vec()
                            }
                        })
                        .collect();
                    admitted.encode(&pcm).unwrap();
                    assert_eq!(admitted.active_mode(), EncoderMode::Hybrid);
                    assert!(
                        Encoder::new_hybrid(EncoderConfig {
                            sample_rate,
                            channels,
                            frame_size: size / 4,
                            ..config
                        })
                        .is_err()
                    );
                }
            }
        }
    }

    #[cfg(feature = "hybrid")]
    fn public_pcm_reference(sample: f32) -> f32 {
        (sample * 32768.0).round().clamp(-32768.0, 32767.0) / 32768.0
    }

    #[test]
    #[cfg(feature = "hybrid")]
    fn public_sub_lsb_reference_has_zero_optimum() {
        for input in [-0.495_f32 / 32768.0, 0.36 / 32768.0, 0.495 / 32768.0] {
            assert_ne!(input, 0.0);
            assert_eq!(public_pcm_reference(input), 0.0);
            let optimum = f64::from(input).powi(2);
            for output in [-1_i16, 0, 1] {
                let error = (f64::from(input) - f64::from(output) / 32768.0).powi(2);
                assert!(error >= optimum);
            }
        }
    }

    /// Public-only matrix: RATE:CHANNELS:MS:BITRATE:MODE:SCENARIO:BANDWIDTH:SHAPE.
    /// MODE 0/1 = Hybrid/Auto; SCENARIO 0/1/2 = cold/steady/transitions;
    /// BANDWIDTH 0/1 = SWB/FB; SHAPE 0..4 retains the layout matrix, 5 is silence.
    /// Cold explicit-Hybrid/SWB cells are negative controls: the constructor has
    /// no bandwidth argument. The other 1,584 cells require actual stream gates.
    /// This supplementary grid does not redefine the agreed acceptance scope.
    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "public acceptance; explicit frozen oracle and reviewed PCM; run bounded cells"]
    fn reference_public_hybrid_auto_matrix() {
        let oracle = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle");
        let media: Vec<f32> = std::fs::read(
            std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM").expect("reviewed real PCM"),
        )
        .unwrap()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
        .collect();
        let cells = public_hybrid_auto_cells();
        let selected = std::env::var("MOOSICBOX_OPUS_PUBLIC_CELL")
            .ok()
            .map(|text| {
                let values: Vec<usize> = text
                    .split(':')
                    .map(|v| v.parse().expect("numeric cell"))
                    .collect();
                let cell: [usize; 8] = values.try_into().expect("eight cell fields required");
                assert!(cells.contains(&cell), "unknown public cell: {cell:?}");
                cell
            });
        // No PUBLIC_RD_QUALIFIER, CLOSED_LOOP_PROOF or internal constructor:
        // rejected intended configurations remain failures until production is ready.
        assert!(!PUBLIC_RD_QUALIFIER.get());
        let mut executed = 0;
        let mut failures = Vec::new();
        for cell in cells {
            if selected.is_some_and(|wanted| wanted != cell) {
                continue;
            }
            executed += 1;
            let result =
                std::panic::catch_unwind(|| qualify_public_hybrid_auto(cell, &media, &oracle));
            match result {
                Ok(Ok(())) => eprintln!("public cell={cell:?} PASS"),
                Ok(Err(error)) => {
                    eprintln!("public cell={cell:?} FAIL {error}");
                    failures.push(format!("{cell:?}: {error}"));
                }
                Err(_) => {
                    eprintln!("public cell={cell:?} FAIL assertion/oracle");
                    failures.push(format!("{cell:?}: assertion/oracle"));
                }
            }
        }
        assert_eq!(executed, if selected.is_some() { 1 } else { 1728 });
        eprintln!(
            "public coverage executed={executed} failed={} filtered={}",
            failures.len(),
            selected.is_some()
        );
        assert!(
            failures.is_empty(),
            "unqualified public cells: {failures:?}"
        );
    }

    #[cfg(feature = "hybrid")]
    fn public_hybrid_auto_cells() -> Vec<[usize; 8]> {
        let mut cells = Vec::new();
        for rate in [24000, 48000] {
            for channels in [1, 2] {
                for ms in [10, 20] {
                    for bitrate in [32000, 64000, 128_000] {
                        for mode in 0..2 {
                            for scenario in 0..3 {
                                for bandwidth in 0..2 {
                                    for shape in 0..6 {
                                        cells.push([
                                            rate, channels, ms, bitrate, mode, scenario, bandwidth,
                                            shape,
                                        ]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        cells
    }

    #[test]
    #[cfg(feature = "hybrid")]
    fn public_hybrid_auto_matrix_coverage_is_complete() {
        let cells = public_hybrid_auto_cells();
        assert_eq!(cells.len(), 1728);
        assert_eq!(
            cells
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1728
        );
        // The old mandatory fullband five-shape layouts are a strict subset;
        // neither a filter nor new negative controls can erase them.
        for cell in cells {
            assert!(matches!(cell[0], 24000 | 48000));
            assert!(matches!(cell[3], 32000 | 64000 | 128_000));
        }
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Public construction/control failures and every stream gate retain exact cell context.
    fn qualify_public_hybrid_auto(
        cell: [usize; 8],
        media: &[f32],
        oracle: &std::ffi::OsStr,
    ) -> std::result::Result<(), String> {
        let [rate, count, ms, bitrate, mode, scenario, bandwidth, shape] = cell;
        let sample_rate = crate::SampleRate::from_hz(rate as u32).unwrap();
        let channels = if count == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let frame_size = rate * ms / 1000;
        let bandwidth = if bandwidth == 0 {
            Bandwidth::SuperWideband
        } else {
            Bandwidth::Fullband
        };
        let mode = if mode == 0 {
            EncoderMode::Hybrid
        } else {
            EncoderMode::Auto
        };
        let initial_bitrate = if scenario == 0 {
            bitrate as u32
        } else {
            128_000
        };
        let config = EncoderConfig {
            sample_rate,
            channels,
            frame_size,
            bitrate: initial_bitrate,
        };
        let cold_explicit = scenario == 0 && mode == EncoderMode::Hybrid;
        if cold_explicit && bandwidth == Bandwidth::SuperWideband {
            // The explicit constructor has no bandwidth argument and promises
            // fullband output. Exercise rejection, not fictitious SWB support.
            // Use an established constructor rate so a bitrate guard cannot
            // accidentally stand in for the bandwidth negative control.
            let mut negative = Encoder::new_hybrid(EncoderConfig {
                bitrate: 128_000,
                ..config
            })
            .map_err(|e| format!("negative-control constructor: {e}"))?;
            let before = negative.control();
            let requested = EncoderControl {
                mode,
                bitrate: 128_000,
                bandwidth,
            };
            if negative.set_control(requested).is_ok() {
                return Err(
                    "explicit constructor unexpectedly accepted SWB runtime control".into(),
                );
            }
            if negative.control() != before {
                return Err("rejected explicit control mutated state".into());
            }
            eprintln!("public negative-control cell={cell:?} explicit-SWB rejection PASS");
            return Ok(());
        }
        let mut encoder = if cold_explicit {
            Encoder::new_hybrid(config)
        } else {
            // Steady and transition scenarios require runtime controls. Warm
            // Hybrid via the adaptive constructor, never a nonadaptive encoder.
            Encoder::new_auto(config)
        }
        .map_err(|e| format!("public constructor: {e}"))?;
        let lookahead = encoder.lookahead();
        let mut packets = Vec::new();
        let mut source = Vec::new();
        for frame in 0..32 {
            let requested_mode =
                if scenario == 2 && !(4..12).contains(&frame) && !(16..24).contains(&frame) {
                    EncoderMode::Celt
                } else if scenario == 1 && frame < 4 {
                    EncoderMode::Hybrid
                } else {
                    mode
                };
            let requested_rate = if (scenario == 2 && requested_mode == EncoderMode::Celt)
                || (scenario == 1 && frame < 4)
            {
                128_000
            } else {
                bitrate as u32
            };
            let control = EncoderControl {
                mode: requested_mode,
                bitrate: requested_rate,
                bandwidth,
            };
            if !cold_explicit {
                encoder
                    .set_control(control)
                    .map_err(|e| format!("frame={frame} public set_control {control:?}: {e}"))?;
            }
            let stereo = if shape == 5 {
                vec![0.0; 48 * ms * 2]
            } else {
                candidate_hybrid_shape(if shape == 4 { 3 } else { shape }, frame, 48 * ms, media)
            };
            let factor = 48000 / rate;
            let pcm: Vec<f32> = stereo
                .as_chunks::<2>()
                .0
                .iter()
                .step_by(factor)
                .flat_map(|pair| {
                    let gain = if shape == 4 { 0.01 } else { 1.0 };
                    if count == 1 {
                        vec![gain * f32::midpoint(pair[0], pair[1])]
                    } else {
                        vec![gain * pair[0], gain * pair[1]]
                    }
                })
                .collect();
            let packet = encoder
                .encode(&pcm)
                .map_err(|e| format!("frame={frame} public encode: {e}"))?;
            if packet.len() * 8 * rate > requested_rate as usize * frame_size {
                return Err(format!("frame={frame} packet budget exceeded"));
            }
            if (packet[0] & 4 != 0) != (count == 2) {
                return Err(format!("frame={frame} TOC channels"));
            }
            if requested_mode == EncoderMode::Hybrid && encoder.active_mode() != EncoderMode::Hybrid
            {
                return Err(format!(
                    "frame={frame} explicit Hybrid silently routed elsewhere"
                ));
            }
            source.extend(pcm);
            packets.push(packet);
        }
        let last_rate = if scenario == 2 {
            128_000
        } else {
            bitrate as u32
        };
        let tail = encoder
            .finish()
            .map_err(|e| format!("public finish: {e}"))?;
        if tail.len() != lookahead.div_ceil(frame_size) {
            return Err("finish tail count".into());
        }
        if tail
            .iter()
            .any(|p| p.len() * 8 * rate > last_rate as usize * frame_size)
        {
            return Err("finish packet budget".into());
        }
        packets.extend(tail);
        let decoded = decode_candidate_rate(&packets, oracle, count, rate as u32);
        if decoded.len() != packets.len() * frame_size * count {
            return Err("C duration".into());
        }
        let mut native = crate::Decoder::new(sample_rate, channels).unwrap();
        for (index, (packet, expected)) in packets
            .iter()
            .zip(decoded.chunks_exact(frame_size * count))
            .enumerate()
        {
            let mut output = vec![0_i16; frame_size * count];
            if native
                .decode(Some(packet), &mut output, false)
                .map_err(|e| format!("native packet={index}: {e}"))?
                != frame_size
            {
                return Err(format!("packet={index} native duration"));
            }
            if output
                .iter()
                .zip(expected)
                .any(|(&a, &b)| (f32::from(a) / 32768.0 - b).abs() > 1.0 / 32768.0)
            {
                return Err(format!("packet={index} native/C parity > 1 LSB"));
            }
        }
        // Supplementary public matrix compares in its decoder's int16 domain.
        // Keep the original floating PCM as encoder input; do not change any
        // established corpus/window/threshold or the original layout matrix.
        let source: Vec<f32> = source.iter().map(|&x| public_pcm_reference(x)).collect();
        trace_public_entry(&packets, &decoded, &source, config, lookahead);
        let aligned = &decoded[lookahead * count..lookahead * count + source.len()];
        let mut quality_failures: Vec<String> = Vec::new();
        for (begin, end) in [
            (0, 2),
            (2, 4),
            (4, 6),
            (6, 12),
            (12, 14),
            (16, 18),
            (18, 24),
            (24, 26),
            (26, 32),
        ] {
            let range = begin * frame_size * count..end * frame_size * count;
            let signal: f64 = source[range.clone()]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = source[range.clone()]
                .iter()
                .zip(&aligned[range.clone()])
                .map(|(&a, &b)| f64::from(a - b).powi(2))
                .sum();
            let snr = 10.0 * (signal / error).log10();
            let pass = if signal <= 1e-12 {
                (error / range.len() as f64).sqrt() <= 1.0 / 32768.0
            } else {
                snr.is_finite() && snr > 3.0
            };
            eprintln!(
                "public quality cell={cell:?} region={begin}..{end} signal={signal} error={error} SNR={snr} pass={pass}"
            );
            if !pass {
                quality_failures.push(format!(
                    "quality region={begin}..{end} SNR={snr} signal={signal} error={error}"
                ));
            }
        }
        if quality_failures.is_empty() {
            Ok(())
        } else {
            Err(quality_failures.join("; "))
        }
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "ordinary public constructor/control quality qualification; explicit oracle and PCM"]
    fn reference_public_practical_rd_quality() {
        PUBLIC_RD_QUALIFIER.set(true);
        reference_practical_unredundant_transition_quality();
        PUBLIC_RD_QUALIFIER.set(false);
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "focused public RD diagnostic; full reference_public_practical_rd_quality remains mandatory"]
    fn reference_public_practical_rd_voiced_10ms() {
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        PUBLIC_RD_QUALIFIER.set(true);
        let failures = qualify_unredundant_case(480, 1, &[], &oracle);
        PUBLIC_RD_QUALIFIER.set(false);
        assert!(
            failures.is_empty(),
            "unqualified voiced steady region: {failures:?}"
        );
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "diagnostic production/proof state equivalence; does not replace full public quality qualification"]
    fn practical_rd_voiced_proof_packet_identity() {
        let config = EncoderConfig {
            bitrate: 64_000,
            frame_size: 480,
            ..EncoderConfig::default()
        };
        let mut public = Encoder::new_auto(config).unwrap();
        public.practical = Some(Box::new(
            super::super::hybrid::PracticalSynthesis::new().unwrap(),
        ));
        let mut proof = Encoder::new_auto(config).unwrap();
        let mut synthesis =
            crate::Decoder::new(crate::SampleRate::Hz48000, Channels::Stereo).unwrap();
        let mut history = vec![0_i16; 384 * 2];
        for frame in 0..24 {
            let pcm = candidate_hybrid_shape(1, frame, 480, &[]);
            history.extend(
                pcm.iter()
                    .map(|x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16),
            );
            let target: Vec<_> = history.drain(..pcm.len()).collect();
            let mode = if (4..12).contains(&frame) || frame >= 16 {
                EncoderMode::Hybrid
            } else {
                EncoderMode::Celt
            };
            let control = EncoderControl {
                mode,
                bitrate: if (4..12).contains(&frame) {
                    32_000
                } else {
                    64_000
                },
                bandwidth: Bandwidth::Fullband,
            };
            public
                .set_control(EncoderControl {
                    mode: if frame >= 16 { EncoderMode::Auto } else { mode },
                    ..control
                })
                .unwrap();
            let actual = public.encode(&pcm).unwrap();
            let expected =
                closed_loop_packet(&mut proof, &mut synthesis, &pcm, control, &target).unwrap();
            assert_eq!(
                actual, expected,
                "production/proof packet divergence at frame {frame}"
            );
            assert_eq!(
                public.active_mode(),
                proof.active_mode(),
                "active mode at frame {frame}"
            );
        }
    }

    #[test]
    #[cfg(feature = "hybrid")]
    #[ignore = "explicit prepared/unfactored candidate identity qualification; shared Cargo slot required"]
    fn practical_prepared_prefix_exact_identity() {
        for frame_size in [480, 960] {
            let mut prepared = Encoder::new_auto(EncoderConfig {
                bitrate: 64_000,
                frame_size,
                ..EncoderConfig::default()
            })
            .unwrap();
            prepared.practical = Some(Box::new(
                super::super::hybrid::PracticalSynthesis::new().unwrap(),
            ));
            let mut reference = prepared.clone();
            let mut actual_synthesis =
                crate::Decoder::new(crate::SampleRate::Hz48000, Channels::Stereo).unwrap();
            let mut expected_synthesis = actual_synthesis.clone();
            for frame in 0..12 {
                let control = EncoderControl {
                    mode: if (2..10).contains(&frame) {
                        EncoderMode::Hybrid
                    } else {
                        EncoderMode::Celt
                    },
                    bitrate: if (2..6).contains(&frame) {
                        32_000
                    } else if (8..10).contains(&frame) {
                        128_000
                    } else {
                        64_000
                    },
                    bandwidth: Bandwidth::Fullband,
                };
                prepared.set_control(control).unwrap();
                reference.set_control(control).unwrap();
                let pcm = candidate_hybrid_shape(frame % 3, frame, frame_size, &[]);
                super::super::RD_VERIFY_CANDIDATES.set(true);
                let actual = prepared.encode(&pcm).unwrap();
                super::super::RD_VERIFY_CANDIDATES.set(false);
                super::super::RD_UNFACTORED_REFERENCE.set(true);
                let expected = reference.encode(&pcm).unwrap();
                super::super::RD_UNFACTORED_REFERENCE.set(false);
                assert_eq!(
                    actual, expected,
                    "winner duration={frame_size} frame={frame}"
                );
                let mut actual_pcm = vec![0; frame_size * 2];
                let mut expected_pcm = actual_pcm.clone();
                actual_synthesis
                    .decode(Some(&actual), &mut actual_pcm, false)
                    .unwrap();
                expected_synthesis
                    .decode(Some(&expected), &mut expected_pcm, false)
                    .unwrap();
                assert_eq!(actual_pcm, expected_pcm);
                if frame == 9 {
                    let mut prepared_tail = prepared.clone();
                    let mut reference_tail = reference.clone();
                    super::super::RD_VERIFY_CANDIDATES.set(true);
                    let actual = prepared_tail.finish().unwrap();
                    super::super::RD_VERIFY_CANDIDATES.set(false);
                    super::super::RD_UNFACTORED_REFERENCE.set(true);
                    let expected = reference_tail.finish().unwrap();
                    super::super::RD_UNFACTORED_REFERENCE.set(false);
                    assert_eq!(actual, expected, "Hybrid finish duration={frame_size}");
                }
            }
            super::super::RD_VERIFY_CANDIDATES.set(true);
            let actual_tail = prepared.finish().unwrap();
            super::super::RD_VERIFY_CANDIDATES.set(false);
            super::super::RD_UNFACTORED_REFERENCE.set(true);
            let expected_tail = reference.finish().unwrap();
            super::super::RD_UNFACTORED_REFERENCE.set(false);
            assert_eq!(actual_tail, expected_tail);
            for (actual, expected) in actual_tail.iter().zip(&expected_tail) {
                let mut actual_pcm = vec![0; frame_size * 2];
                let mut expected_pcm = actual_pcm.clone();
                actual_synthesis
                    .decode(Some(actual), &mut actual_pcm, false)
                    .unwrap();
                expected_synthesis
                    .decode(Some(expected), &mut expected_pcm, false)
                    .unwrap();
                assert_eq!(actual_pcm, expected_pcm);
            }
        }
    }

    #[test]
    #[cfg(feature = "hybrid")]
    fn practical_shadow_preserves_nominal_high_rate_packets() {
        let config = EncoderConfig {
            bitrate: 224_000,
            ..EncoderConfig::default()
        };
        let mut nominal = Encoder::new_auto(config).unwrap();
        nominal.practical = None;
        let mut shadowed = nominal.clone();
        shadowed.practical = Some(Box::new(
            super::super::hybrid::PracticalSynthesis::new().unwrap(),
        ));
        for frame in 0..4 {
            let pcm = candidate_hybrid_shape(1, frame, 960, &[]);
            assert_eq!(
                nominal.encode(&pcm).unwrap(),
                shadowed.encode(&pcm).unwrap()
            );
        }
        assert_eq!(nominal.finish().unwrap(), shadowed.finish().unwrap());
    }

    #[cfg(feature = "hybrid")]
    struct ClosedLoopCandidate {
        error: i128,
        encoder: Encoder,
        synthesis: crate::Decoder,
        packet: Vec<u8>,
        db: i32,
    }

    #[cfg(feature = "hybrid")]
    fn closed_loop_packet(
        encoder: &mut Encoder,
        decoder: &mut crate::Decoder,
        pcm: &[f32],
        control: EncoderControl,
        target: &[i16],
    ) -> crate::Result<Vec<u8>> {
        let mut best: Option<ClosedLoopCandidate> = None;
        let mut nominal_error = None;
        for db in [0, -1, -2, -3, -6] {
            if db != 0 && (control.mode != EncoderMode::Hybrid || control.bitrate >= 128_000) {
                continue;
            }
            let mut candidate = encoder.clone();
            let mut synthesis = decoder.clone();
            let previous = super::super::HYBRID_TEST_ENERGY_DB.replace(db);
            let result = candidate.encode_unredundant_candidate(pcm, control);
            super::super::HYBRID_TEST_ENERGY_DB.set(previous);
            let Ok(packet) = result else { continue };
            if packet.len() > control.bitrate as usize * encoder.config.frame_size / 384_000 {
                continue;
            }
            let mut output = vec![0_i16; target.len()];
            let Ok(samples) = synthesis.decode(Some(&packet), &mut output, false) else {
                continue;
            };
            if samples * super::super::channel_count(encoder.config.channels) != target.len() {
                continue;
            }
            let distortion: i128 = output
                .iter()
                .zip(target)
                .map(|(&a, &b)| i128::from(i32::from(a) - i32::from(b)).pow(2))
                .sum();
            if db == 0 {
                nominal_error = Some(distortion);
            }
            if best.as_ref().is_none_or(|old| distortion < old.error) {
                best = Some(ClosedLoopCandidate {
                    error: distortion,
                    encoder: candidate,
                    synthesis,
                    packet,
                    db,
                });
            }
        }
        let Some(ClosedLoopCandidate {
            error,
            encoder: candidate,
            synthesis,
            packet,
            db,
        }) = best
        else {
            return Err(super::unavailable("no finalized native RD candidate"));
        };
        if let Some(nominal) = nominal_error {
            assert!(error <= nominal);
        }
        eprintln!("closed loop selected db={db} distortion={error} nominal={nominal_error:?}");
        *encoder = candidate;
        *decoder = synthesis;
        Ok(packet)
    }

    #[cfg(feature = "hybrid")]
    fn trace_practical_frame_errors(
        source: &[f32],
        aligned: &[f32],
        frame_size: usize,
        shape: usize,
    ) {
        if !PUBLIC_RD_QUALIFIER.get() || std::env::var_os("MOOSICBOX_OPUS_RD_TRACE").is_none() {
            return;
        }
        for frame in 0..32 {
            let start = frame * frame_size * 2;
            let end = start + frame_size * 2;
            let signal: f64 = source[start..end]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = source[start..end]
                .iter()
                .zip(&aligned[start..end])
                .map(|(&a, &b)| f64::from(a - b).powi(2))
                .sum();
            eprintln!(
                "practical RD oracle shape={shape} frame_size={frame_size} frame={frame} signal={signal} error={error} SNR={}",
                10.0 * (signal / error).log10()
            );
        }
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Persistent packet matrix retains ordered control, oracle, tail and quality gates.
    fn qualify_unredundant_case_with_budget(
        frame_size: usize,
        shape: usize,
        media: &[f32],
        oracle: &std::ffi::OsStr,
        prefix_bytes: Option<usize>,
    ) -> Vec<String> {
        let mut encoder = Encoder::new_auto(EncoderConfig {
            bitrate: 64_000,
            frame_size,
            ..EncoderConfig::default()
        })
        .unwrap();
        let mut source = Vec::new();
        let mut packets = Vec::new();
        if PUBLIC_RD_QUALIFIER.get() {
            assert!(
                encoder.practical.is_some(),
                "ordinary new_auto must create synthesis state"
            );
        }
        let mut proof_decoder =
            crate::Decoder::new(crate::SampleRate::Hz48000, Channels::Stereo).unwrap();
        for frame in 0..32 {
            let (mode, bitrate) = match frame {
                0..=3 | 12..=15 | 24..=31 => (EncoderMode::Celt, 64_000),
                4..=11 => (EncoderMode::Hybrid, 32_000),
                _ => (EncoderMode::Hybrid, 64_000),
            };
            let mut pcm = candidate_hybrid_shape(
                if shape == 4 { 3 } else { shape },
                frame,
                frame_size,
                media,
            );
            if shape == 4 {
                // Quiet real music retains the relative >3 dB gate; it is not
                // reclassified as silence to make low-level errors disappear.
                for sample in &mut pcm {
                    *sample *= 0.01;
                }
            }
            source.extend_from_slice(&pcm);
            let trace_prefix = matches!(
                (shape, frame_size, frame),
                (3, 480, 4..=6) | (1, 960, 16..=18)
            );
            let incoming_silk = trace_prefix.then(|| encoder.silk.as_ref().unwrap().clone());
            // The frontier experiment varies only the 32k prefix. Pin the
            // other frames to its original control allocation on the same
            // per-encoder override path as production RD candidates.
            encoder.practical_prefix_bitrate = prefix_bytes.map(|bytes| {
                if bitrate == 32_000 {
                    ((bytes + 1) * 384_000 / frame_size) as u32
                } else {
                    super::super::hybrid::packet_lowband_bitrate(
                        bitrate,
                        Channels::Stereo,
                        frame_size,
                    )
                }
            });
            let control = EncoderControl {
                bitrate,
                bandwidth: Bandwidth::Fullband,
                mode: if PUBLIC_RD_QUALIFIER.get() && (16..24).contains(&frame) {
                    EncoderMode::Auto
                } else {
                    mode
                },
            };
            let packet = if PUBLIC_RD_QUALIFIER.get() {
                if std::env::var_os("MOOSICBOX_OPUS_RD_TRACE").is_some() {
                    eprintln!(
                        "practical RD input shape={shape} frame_size={frame_size} frame={frame} mode={:?} bitrate={bitrate}",
                        control.mode
                    );
                }
                assert_eq!(
                    super::super::HYBRID_TEST_ENERGY_DB.get(),
                    0,
                    "public RD must use only the production field bias"
                );
                encoder
                    .set_control(control)
                    .and_then(|()| encoder.encode(&pcm))
            } else if CLOSED_LOOP_PROOF.get() {
                let delay = encoder.lookahead() * 2;
                let begin = frame * frame_size * 2;
                let target: Vec<_> = (begin..begin + pcm.len())
                    .map(|i| {
                        let sample = i.checked_sub(delay).map_or(0.0, |j| source[j]);
                        (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16
                    })
                    .collect();
                closed_loop_packet(&mut encoder, &mut proof_decoder, &pcm, control, &target)
            } else {
                encoder.encode_unredundant_candidate(&pcm, control)
            };
            encoder.practical_prefix_bitrate = None;
            let packet = packet.unwrap();
            if let Some(incoming_silk) = incoming_silk {
                dump_cold_entry(&encoder, shape, frame, bitrate, &packet, incoming_silk);
            }
            assert!(packet.len() <= bitrate as usize * frame_size / 384_000);
            assert_eq!(encoder.active_mode(), mode);
            packets.push(packet);
        }
        packets.extend(encoder.finish().unwrap());
        let decoded = decode_candidate_hybrid(&packets, oracle);
        assert_eq!(decoded.len(), packets.len() * frame_size * 2);
        assert_auto_native_parity(&packets, frame_size, &decoded);
        report_unredundant_prefixes(
            &packets,
            frame_size,
            &decoded,
            &source,
            encoder.lookahead(),
            shape,
        );
        let aligned = &decoded[encoder.lookahead() * 2..encoder.lookahead() * 2 + source.len()];
        trace_practical_frame_errors(&source, aligned, frame_size, shape);
        let mut failures = Vec::new();
        // Include each entry/departure, not only settled interior frames.
        let regions: &[(usize, usize)] = if PUBLIC_RD_QUALIFIER.get() || CLOSED_LOOP_PROOF.get() {
            &[(4, 6), (6, 12), (12, 14), (16, 18), (18, 24), (24, 26)]
        } else {
            &[(4, 6), (12, 14), (16, 18), (24, 26)]
        };
        for &(boundary, end) in regions {
            let range = boundary * frame_size * 2..end * frame_size * 2;
            let count = (range.end - range.start) as f64;
            let signal: f64 = source[range.clone()]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = source[range.clone()]
                .iter()
                .zip(&aligned[range])
                .map(|(&a, &b)| f64::from(a - b).powi(2))
                .sum();
            let snr = 10.0 * (signal / error).log10();
            eprintln!(
                "no-redundancy shape={shape} duration={frame_size} boundary={boundary} SNR={snr}"
            );
            // Exact silence has undefined SNR: require RMS <= one signed-16
            // PCM step. Nonzero-energy attacks retain the original >3 dB gate.
            let qualified = if signal == 0.0 {
                error <= count / (32768.0 * 32768.0)
            } else {
                snr > 3.0
            };
            eprintln!(
                "no-redundancy signal_energy={signal} error_energy={error} rms_error={}",
                (error / count).sqrt()
            );
            if !qualified {
                failures.push(format!("shape={shape} duration={frame_size} boundary={boundary} SNR={snr} error={error}"));
            }
        }
        failures
    }

    /// Called only by the explicitly ignored oracle transition candidate.
    #[cfg(feature = "hybrid")]
    fn dump_cold_entry(
        encoder: &Encoder,
        shape: usize,
        frame: usize,
        bitrate: u32,
        packet: &[u8],
        mut incoming_silk: super::super::silk::SilkEncoder,
    ) {
        use std::fmt::Write as _;

        let frame_size = encoder.config.frame_size;
        let path = format!("/tmp/opus-cold-entry-{shape}-{frame_size}-{frame}.f32");
        let bytes: Vec<_> = encoder
            .adaptive_low
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        std::fs::write(&path, bytes).unwrap();
        // Preserve the transmitted shared prefix for stateful replay. These
        // three-frame traces distinguish a good cold frame from a bad successor;
        // replaying every PCM file with a fresh SILK encoder would hide that bug.
        std::fs::write(format!("{path}.packet"), packet).unwrap();
        let prefix_bitrate = encoder.silk.as_ref().unwrap().bitrate();
        let prefix_bytes = (prefix_bitrate as usize * frame_size / 384_000).saturating_sub(1);
        let payload_bytes = (bitrate as usize * frame_size / 384_000).saturating_sub(1);
        // Prefix is the first entropy operation in encode_coding_frame. This
        // fresh coder therefore has the exact incoming state (not a decoder's
        // post-header tell). No SILK/private predictive state is inferred here.
        let incoming = crate::range::RangeEncoder::new(payload_bytes).unwrap();
        let channels = super::super::channel_count(encoder.config.channels);
        let reset = matches!(frame, 4 | 16);
        let metadata = format!(
            "format=f32le\nlayout=interleaved\nchannels={channels}\nsample_rate=16000\nsample_count={}\nsamples_per_channel={}\nshape={shape}\nsource_frame_size_48k={frame_size}\nsource_frame={frame}\npacket_bitrate={bitrate}\nprefix_bitrate={prefix_bitrate}\nprefix_budget_bytes={prefix_bytes}\nprefix_budget_bits={}\npacket_payload_bytes={payload_bytes}\nincoming_range_tell={}\nsilk_reset={reset}\nanalysis_history=preserved\n",
            encoder.adaptive_low.len(),
            encoder.adaptive_low.len() / channels,
            prefix_bytes * 8,
            incoming.tell(),
        );
        std::fs::write(format!("{path}.meta"), &metadata).unwrap();
        if reset {
            incoming_silk.reset();
        }
        incoming_silk.set_bitrate(prefix_bitrate).unwrap();
        let mut states = format!("incoming={incoming:#?}\n");
        for ranked in [false, true] {
            // Both trials start from the actual pre-frame predictive snapshot,
            // not independently reset input. Only ranked is the production
            // choice at these bitrates; neither replay mutates the real encoder.
            let mut silk = incoming_silk.clone();
            let mut coder = incoming.clone();
            let result = if ranked {
                silk.encode_prefix_ranked(&encoder.adaptive_low, &mut coder)
            } else {
                silk.encode_prefix(&encoder.adaptive_low, &mut coder)
            };
            writeln!(
                states,
                "ranked={ranked} result={result:?} tell={} range={} state={coder:#?}",
                coder.tell(),
                coder.range()
            )
            .unwrap();
        }
        std::fs::write(format!("{path}.coder"), states).unwrap();
        eprintln!("cold entry dump {path}\n{metadata}");
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Diagnostic entropy replay follows SILK prefix decoding order.
    fn report_unredundant_prefixes(
        packets: &[Vec<u8>],
        frame_size: usize,
        full: &[f32],
        source: &[f32],
        lookahead: usize,
        shape: usize,
    ) {
        for boundary in [4, 16] {
            // At a CELT->hybrid boundary the new layers are reset. A fresh
            // decoder therefore reconstructs the same new-mode sum, without
            // the outgoing CELT concealment/fade applied by a continuous stream.
            let mut isolated =
                crate::Decoder::new(crate::SampleRate::Hz48000, Channels::Stereo).unwrap();
            let mut decoder = crate::silk::SilkDecoder::new(
                crate::SampleRate::Hz16000,
                Channels::Stereo,
                (frame_size / 48) as u8,
            )
            .unwrap();
            let mut resamplers = [
                crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap(),
                crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap(),
            ];
            let mut gate_low = Vec::new();
            let mut gate_isolated = Vec::new();
            // Replay all eight Hybrid packets from the reset boundary so
            // steady-state failures retain the exact predictor/resampler history.
            let count = if PUBLIC_RD_QUALIFIER.get() { 8 } else { 3 };
            for (frame, packet) in packets.iter().enumerate().skip(boundary).take(count) {
                let mut isolated_pcm = vec![0_i16; frame_size * 2];
                assert_eq!(
                    isolated
                        .decode(Some(packet), &mut isolated_pcm, false)
                        .unwrap(),
                    frame_size
                );
                let mut range = crate::range::RangeDecoder::new(&packet[1..]).unwrap();
                decoder.begin_packet();
                decoder.decode_header_bits(&mut range, true).unwrap();
                let mut pcm = vec![0_i16; frame_size / 3 * 2];
                decoder
                    .decode_silk_frame(&mut range, true, &mut pcm)
                    .unwrap();
                let bits = range.ec_tell();
                let planar: Vec<_> = resamplers
                    .iter_mut()
                    .enumerate()
                    .map(|(channel, resampler)| {
                        resampler
                            .process(
                                &pcm.iter()
                                    .skip(channel)
                                    .step_by(2)
                                    .copied()
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap()
                    })
                    .collect();
                let low: Vec<_> = planar[0]
                    .iter()
                    .zip(&planar[1])
                    .flat_map(|(&left, &right)| {
                        [f32::from(left) / 32768.0, f32::from(right) / 32768.0]
                    })
                    .collect();
                gate_low.extend_from_slice(&low);
                gate_isolated.extend(isolated_pcm.iter().map(|&x| f32::from(x) / 32768.0));
                let reference = &full[frame * frame_size * 2..(frame + 1) * frame_size * 2];
                let low_energy: f64 = low.iter().map(|&x| f64::from(x).powi(2)).sum();
                let residual: f64 = low
                    .iter()
                    .zip(reference)
                    .map(|(&a, &b)| f64::from(a - b).powi(2))
                    .sum();
                let target_start = (frame * frame_size - lookahead) * 2;
                let target = &source[target_start..target_start + frame_size * 2];
                for (label, samples) in [
                    ("old_only_2p5ms", 0..240),
                    ("crossfade_2p5ms", 240..480),
                    ("head5ms", 0..480),
                    ("after5ms", 480..frame_size * 2),
                ] {
                    let mut target_energy = 0.0_f64;
                    let mut target_minus_low = 0.0_f64;
                    let mut output_error = 0.0_f64;
                    let mut added_energy = 0.0_f64;
                    let mut residual_dot_added = 0.0_f64;
                    let mut isolated_error = 0.0_f64;
                    let mut highband_energy = 0.0_f64;
                    let mut transition_energy = 0.0_f64;
                    let mut transition_cross = 0.0_f64;
                    for i in samples {
                        let t = f64::from(target[i]);
                        let l = f64::from(low[i]);
                        let y = f64::from(reference[i]);
                        let z = f64::from(isolated_pcm[i]) / 32768.0;
                        isolated_error += (t - z).powi(2);
                        highband_energy += (z - l).powi(2);
                        transition_energy += (y - z).powi(2);
                        transition_cross += (t - z) * (y - z);
                        target_energy += t * t;
                        target_minus_low += (t - l).powi(2);
                        output_error += (t - y).powi(2);
                        added_energy += (y - l).powi(2);
                        residual_dot_added += (t - l) * (y - l);
                    }
                    // z is reset new-mode synthesis without the outgoing fade.
                    // E(t-y)=E(t-z)+E(y-z)-2 dot(t-z,y-z). Outside the first
                    // 5 ms, y-z should be only native/C rounding differences;
                    // a larger residual falsifies this reset-state comparison.
                    eprintln!(
                        "entry isolated boundary={boundary} frame={frame} region={label} isolated_error={isolated_error} highband_energy={highband_energy} transition_energy={transition_energy} transition_cross={transition_cross} full_error={output_error}"
                    );
                    // y-l contains the high band PLUS transition concealment/
                    // fade; it is not an isolated CELT decoder reconstruction.
                    // Error identity: E(t-y)=E(t-l)+E(y-l)-2 dot(t-l,y-l).
                    eprintln!(
                        "entry sum boundary={boundary} frame={frame} region={label} target_energy={target_energy} target_minus_low={target_minus_low} output_error={output_error} added_energy={added_energy} residual_dot_added={residual_dot_added}"
                    );
                }
                let peak = pcm.iter().map(|x| i32::from(*x).abs()).max().unwrap();
                // Residual includes decoder old-mode concealment/crossfade on
                // the entry packet, not just the CELT high-band suffix.
                eprintln!(
                    "entry prefix boundary={boundary} frame={frame} bits={bits} pcm_peak={peak} low_energy={low_energy} reference_minus_low={residual}"
                );
            }
            if shape == 3 && frame_size == 480 && boundary == 4 {
                let base = "/tmp/opus-entry-reconstruction-3-480-4";
                let target_start = (boundary * frame_size - lookahead) * 2;
                for (name, pcm) in [
                    (
                        "target",
                        &source[target_start..target_start + gate_low.len()],
                    ),
                    ("low", gate_low.as_slice()),
                    ("isolated", gate_isolated.as_slice()),
                    (
                        "full",
                        &full
                            [boundary * frame_size * 2..boundary * frame_size * 2 + gate_low.len()],
                    ),
                ] {
                    let bytes: Vec<_> = pcm.iter().flat_map(|x| x.to_le_bytes()).collect();
                    std::fs::write(format!("{base}-{name}.f32"), bytes).unwrap();
                }
                std::fs::write(format!("{base}.meta"), format!("format=f32le\nchannels=2\nsample_rate=48000\npacket_frames=4,5,6\npacket_samples_per_channel=480\ntarget_source_start={}\ngate_offset_samples_per_channel={lookahead}\ngate_length_samples_per_channel=960\n", target_start / 2)).unwrap();
            }
            let mut low_error = 0.0_f64;
            let mut full_error = 0.0_f64;
            let mut suffix_energy = 0.0_f64;
            let mut cross = 0.0_f64;
            let mut fade_energy = 0.0_f64;
            for i in 0..frame_size * 4 {
                let t = f64::from(source[boundary * frame_size * 2 + i]);
                let l = f64::from(gate_low[lookahead * 2 + i]);
                let z = f64::from(gate_isolated[lookahead * 2 + i]);
                let y = f64::from(full[boundary * frame_size * 2 + lookahead * 2 + i]);
                low_error += (t - l).powi(2);
                full_error += (t - y).powi(2);
                suffix_energy += (y - l).powi(2);
                cross += (t - l) * (y - l);
                fade_energy += (y - z).powi(2);
            }
            eprintln!(
                "entry exact gate boundary={boundary} low_error={low_error} full_error={full_error} suffix_energy={suffix_energy} correction_cross={cross} fade_energy={fade_energy}"
            );
        }
    }

    #[cfg(feature = "hybrid")]
    fn assert_auto_native_parity(packets: &[Vec<u8>], frame_size: usize, reference: &[f32]) {
        let mut decoder =
            crate::Decoder::new(crate::SampleRate::Hz48000, Channels::Stereo).unwrap();
        for (packet, expected) in packets.iter().zip(reference.chunks_exact(frame_size * 2)) {
            let mut output = vec![0_i16; frame_size * 2];
            assert_eq!(
                decoder.decode(Some(packet), &mut output, false).unwrap(),
                frame_size
            );
            let maximum = output
                .iter()
                .zip(expected)
                .map(|(&native, &external)| {
                    (i32::from(native) - (external * 32768.0).round() as i32).abs()
                })
                .max()
                .unwrap();
            assert!(maximum <= 2, "Auto native/reference PCM mismatch={maximum}");
        }
    }

    #[cfg(feature = "hybrid")]
    fn qualify_hybrid_policy(short_split: usize) {
        // A required independent fixture prevents synthetic-only qualification.
        // Supply headerless 48 kHz stereo signed-16 LE PCM, at least 0.64 s.
        let path = std::env::var_os("MOOSICBOX_OPUS_HYBRID_REAL_PCM")
            .expect("real-media PCM fixture required");
        let bytes = std::fs::read(path).expect("read real-media PCM");
        assert_eq!(bytes.len() % 4, 0, "complete stereo PCM samples required");
        let pcm: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        assert!(
            pcm.len() >= 32 * 960 * 2,
            "at least 0.64 s of real media required"
        );
        run_hybrid_candidates(Some(short_split), &pcm);
    }

    #[cfg(feature = "hybrid")]
    fn hybrid_candidate_selected(
        short_split: Option<usize>,
        bitrate: u32,
        frame_size: usize,
        split: usize,
    ) -> bool {
        let Some(short_split) = short_split else {
            return split < 5;
        };
        if bitrate == 16_000 {
            return false;
        }
        // Selection 5 exercises the public constructor and production allocation.
        if short_split >= 5 {
            return split == 5;
        }
        // Compare short-packet alternatives explicitly; keep split3 at
        // 32k/20ms and the established split at 64k unchanged.
        let selected = match (bitrate, frame_size) {
            (32_000, 480) => short_split,
            (32_000, _) => 3,
            _ => 0,
        };
        split == selected
    }

    #[cfg(feature = "hybrid")]
    fn hybrid_candidate_rates(short_split: Option<usize>) -> &'static [u32] {
        match short_split {
            Some(6) => &[40_000, 48_000, 96_000],
            Some(_) => &[32_000, 64_000],
            None => &[16_000, 32_000, 64_000],
        }
    }

    #[cfg(feature = "hybrid")]
    fn run_hybrid_candidates(short_split: Option<usize>, media: &[f32]) {
        let policy = short_split.is_some();
        let oracle =
            std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
        let mut policy_failures = Vec::new();
        for shape in 0..if policy { 4 } else { 1 } {
            let mut failures = Vec::new();
            for frame_size in [480, 960] {
                for &bitrate in hybrid_candidate_rates(short_split) {
                    // 16k is diagnostic-only, explicitly not a supported policy.
                    if policy && bitrate == 16_000 {
                        continue;
                    }
                    for split in 0..6 {
                        if !hybrid_candidate_selected(short_split, bitrate, frame_size, split) {
                            continue;
                        }
                        let (mut encoder, low) =
                            hybrid_candidate_encoder(bitrate, frame_size, split);
                        let mut source = Vec::new();
                        let mut packets = Vec::new();
                        let mut failure = None;
                        for frame in 0..32 {
                            let pcm = candidate_hybrid_shape(shape, frame, frame_size, media);
                            source.extend_from_slice(&pcm);
                            match encoder.encode(&pcm) {
                                Ok(packet) => {
                                    assert!(
                                        packet.len() <= bitrate as usize * frame_size / 384_000
                                    );
                                    packets.push(packet);
                                }
                                Err(error) => {
                                    failure = Some(format!("frame={frame} {error}"));
                                    break;
                                }
                            }
                        }
                        if let Some(error) = failure {
                            failures.push(format!("{bitrate}/{frame_size}/split{split}: {error}"));
                            continue;
                        }
                        match encoder.finish() {
                            Ok(tail) => packets.extend(tail),
                            Err(error) => {
                                failures.push(format!(
                                    "{bitrate}/{frame_size}/split{split}: flush {error}"
                                ));
                                continue;
                            }
                        }
                        let decoded = decode_candidate_hybrid(&packets, &oracle);
                        assert_eq!(decoded.len(), packets.len() * frame_size * 2);
                        let lowband = report_hybrid_components(
                            shape, bitrate, frame_size, &packets, &decoded,
                        );
                        report_hybrid_error_split(frame_size, &source, &decoded, &lowband);
                        if shape == 0 {
                            report_hybrid_music_error(frame_size, &source, &decoded, &lowband);
                        }
                        let aligned = &decoded
                            [encoder.lookahead() * 2..encoder.lookahead() * 2 + source.len()];
                        let range = frame_size * 4..source.len() - frame_size * 2;
                        let signal: f64 = source[range.clone()]
                            .iter()
                            .map(|&x| f64::from(x).powi(2))
                            .sum();
                        let error: f64 = range
                            .map(|i| f64::from(source[i] - aligned[i]).powi(2))
                            .sum();
                        let snr = 10.0 * (signal / error).log10();
                        report_hybrid_waveform(shape, bitrate, frame_size, &source, aligned);
                        eprintln!(
                            "hybrid candidate bitrate={bitrate} duration={frame_size} split={split} low={low} SNR={snr}"
                        );
                        if !snr.is_finite() || snr <= 3.0 {
                            failures
                                .push(format!("{bitrate}/{frame_size}/split{split}: SNR={snr}"));
                        }
                    }
                }
            }
            if policy {
                policy_failures.extend(
                    failures
                        .into_iter()
                        .map(|failure| format!("shape={shape}: {failure}")),
                );
            } else {
                eprintln!("experimental split failures (including unsupported 16k): {failures:#?}");
            }
        }
        if policy {
            assert!(
                policy_failures.is_empty(),
                "unqualified policy: {policy_failures:#?}"
            );
        }
    }

    #[cfg(feature = "hybrid")]
    fn report_hybrid_components(
        shape: usize,
        bitrate: u32,
        frame_size: usize,
        packets: &[Vec<u8>],
        reference: &[f32],
    ) -> Vec<f32> {
        // Decode the transmitted prefix, not an encoder-side reconstruction.
        // This intentionally uses an independent native SILK decoder; the full
        // hybrid PCM above remains decoded by the external reference oracle.
        let mut decoder = crate::silk::SilkDecoder::new(
            crate::SampleRate::Hz16000,
            Channels::Stereo,
            (frame_size / 48) as u8,
        )
        .unwrap();
        let mut resamplers = [
            crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap(),
            crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap(),
        ];
        let mut lowband = Vec::new();
        let mut prefix_bits = Vec::new();
        for packet in packets {
            let mut range = crate::range::RangeDecoder::new(&packet[1..]).unwrap();
            decoder.begin_packet();
            decoder.decode_header_bits(&mut range, true).unwrap();
            let mut pcm = vec![0_i16; frame_size / 3 * 2];
            decoder
                .decode_silk_frame(&mut range, true, &mut pcm)
                .unwrap();
            prefix_bits.push(range.ec_tell());
            let planar: Vec<_> = resamplers
                .iter_mut()
                .enumerate()
                .map(|(channel, resampler)| {
                    let input: Vec<_> = pcm.iter().skip(channel).step_by(2).copied().collect();
                    resampler.process(&input).unwrap()
                })
                .collect();
            assert_eq!(planar[0].len(), frame_size);
            for (&left, &right) in planar[0].iter().zip(&planar[1]) {
                lowband.extend([f32::from(left) / 32768.0, f32::from(right) / 32768.0]);
            }
        }
        let mut suffix_energy = 0.0_f64;
        let mut low_energy = 0.0_f64;
        let mut low_peak = 0.0_f32;
        let mut clipped = 0;
        for i in frame_size * 4..reference.len() - frame_size * 2 {
            suffix_energy += f64::from(reference[i] - lowband[i]).powi(2);
            low_energy += f64::from(lowband[i]).powi(2);
            low_peak = low_peak.max(lowband[i].abs());
            clipped += usize::from(lowband[i].abs() >= 0.999);
        }
        eprintln!("hybrid prefix bits per packet: {prefix_bits:?}");
        eprintln!(
            "hybrid components shape={shape} bitrate={bitrate} duration={frame_size} low_peak={low_peak} low_clipped={clipped} low_energy={low_energy} reference_minus_native_low_energy={suffix_energy}"
        );
        lowband
    }

    #[cfg(feature = "hybrid")]
    fn report_hybrid_error_split(frame_size: usize, source: &[f32], full: &[f32], low: &[f32]) {
        let (mut low_error, mut suffix_energy, mut cross) = (0.0_f64, 0.0_f64, 0.0_f64);
        let delay = super::super::hybrid::DELAY * 2;
        let range = frame_size * 4..source.len() - frame_size * 2;
        for ((&original, &decoded), &lowband) in source[range.clone()]
            .iter()
            .zip(&full[range.start + delay..range.end + delay])
            .zip(&low[range.start + delay..range.end + delay])
        {
            let error = f64::from(original - lowband);
            let suffix = f64::from(decoded - lowband);
            low_error += error * error;
            suffix_energy += suffix * suffix;
            cross += error * suffix;
        }
        // Full error = low error + suffix energy - twice the correction cross term.
        eprintln!(
            "hybrid error decomposition low_error={low_error} suffix_energy={suffix_energy} correction_cross={cross} full_error={}",
            low_error + suffix_energy - 2.0 * cross
        );
    }

    #[cfg(feature = "hybrid")]
    fn report_hybrid_music_error(frame_size: usize, source: &[f32], full: &[f32], low: &[f32]) {
        let delay = super::super::hybrid::DELAY * 2;
        let range = frame_size * 4..source.len() - frame_size * 2;
        let (mut quiet, mut burst) = ([0.0_f64; 2], [0.0_f64; 2]);
        for (i, &original) in source.iter().enumerate().take(range.end).skip(range.start) {
            let bucket = if (300..420).contains(&((i / 2) % 960)) {
                &mut burst
            } else {
                &mut quiet
            };
            bucket[0] += f64::from(original - low[i + delay]).powi(2);
            bucket[1] += f64::from(original - full[i + delay]).powi(2);
        }
        eprintln!("hybrid music error [low,full] quiet={quiet:?} burst={burst:?}");
        for channel in 0..2 {
            for frequency in [440.0 + channel as f64 * 220.0, 1320.0] {
                let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
                let mut count = 0;
                for (sample_index, &sample) in low[range.start + delay..range.end + delay]
                    .iter()
                    .skip(channel)
                    .step_by(2)
                    .enumerate()
                {
                    let phase =
                        std::f64::consts::TAU * frequency * (range.start / 2 + sample_index) as f64
                            / 48000.0;
                    real += f64::from(sample) * phase.sin();
                    imaginary += f64::from(sample) * phase.cos();
                    count += 1;
                }
                let amplitude = 2.0 * real.hypot(imaginary) / f64::from(count);
                eprintln!(
                    "hybrid low tone channel={channel} frequency={frequency} amplitude={amplitude}"
                );
            }
        }
    }

    #[cfg(feature = "hybrid")]
    fn report_hybrid_waveform(
        shape: usize,
        bitrate: u32,
        frame_size: usize,
        source: &[f32],
        decoded: &[f32],
    ) {
        for channel in 0..2 {
            let (mut input_energy, mut output_energy, mut cross) = (0.0_f64, 0.0_f64, 0.0_f64);
            let mut peak = 0.0_f32;
            let mut clipped = 0;
            for i in (frame_size * 4 + channel..source.len() - frame_size * 2).step_by(2) {
                let input = f64::from(source[i]);
                let output = f64::from(decoded[i]);
                input_energy += input * input;
                output_energy += output * output;
                cross += input * output;
                peak = peak.max(decoded[i].abs());
                clipped += usize::from(decoded[i].abs() >= 0.999);
            }
            let gain = cross / input_energy;
            let correlation = cross / (input_energy * output_energy).sqrt();
            eprintln!(
                "hybrid waveform shape={shape} bitrate={bitrate} duration={frame_size} channel={channel} gain={gain} correlation={correlation} peak={peak} clipped={clipped}"
            );
        }
    }

    #[cfg(feature = "hybrid")]
    fn hybrid_candidate_encoder(bitrate: u32, frame_size: usize, split: usize) -> (Encoder, u32) {
        if split == 5 {
            let encoder = Encoder::new_hybrid(EncoderConfig {
                bitrate,
                frame_size,
                ..EncoderConfig::default()
            })
            .expect("public constructor must accept the qualified configuration");
            let low =
                super::super::hybrid::packet_lowband_bitrate(bitrate, Channels::Stereo, frame_size);
            return (encoder, low);
        }
        let mut encoder = Encoder::new_hybrid(EncoderConfig {
            frame_size,
            bitrate: 128_000,
            ..EncoderConfig::default()
        })
        .unwrap();
        encoder.config.bitrate = bitrate;
        let low = match split {
            0 => super::super::hybrid::lowband_bitrate(bitrate, Channels::Stereo),
            1 => super::super::hybrid::candidate_lowband_bitrate(
                bitrate,
                Channels::Stereo,
                frame_size,
            ),
            2 => (bitrate / 2).max(8_000),
            3 => super::super::hybrid::candidate_lowband_first_bitrate(bitrate, frame_size),
            _ => super::super::hybrid::candidate_short_prefix_bitrate(bitrate, frame_size),
        };
        encoder.silk.as_mut().unwrap().set_bitrate(low).unwrap();
        (encoder, low)
    }

    #[cfg(feature = "hybrid")]
    fn candidate_hybrid_shape(
        shape: usize,
        frame: usize,
        frame_size: usize,
        media: &[f32],
    ) -> Vec<f32> {
        if shape == 0 {
            return candidate_hybrid_music(frame, frame_size);
        }
        let start = frame * frame_size * 2;
        if shape == 3 {
            return media[start..start + frame_size * 2].to_vec();
        }
        (0..frame_size)
            .flat_map(|i| {
                let sample = frame * frame_size + i;
                (0..2).map(move |channel| {
                    let t = sample as f32 / 48000.0;
                    let tau = std::f32::consts::TAU;
                    if shape == 1 {
                        // Voiced harmonic stack, distinct stereo fundamentals.
                        (1..=8)
                            .map(|h| {
                                0.12 / h as f32
                                    * (tau * (140.0 + channel as f32 * 37.0) * h as f32 * t).sin()
                            })
                            .sum()
                    } else {
                        // Abrupt attacks with substantial hybrid high-band energy.
                        let envelope = (1.0 - (sample % 2400) as f32 / 600.0).max(0.0);
                        envelope
                            * (0.2 * (tau * 730.0 * t).sin()
                                + 0.1 * (tau * (9300.0 + channel as f32 * 1100.0) * t).sin())
                    }
                })
            })
            .collect()
    }

    #[cfg(feature = "hybrid")]
    fn decode_candidate_hybrid(packets: &[Vec<u8>], oracle: &std::ffi::OsStr) -> Vec<f32> {
        decode_candidate_layout(packets, oracle, 2)
    }

    #[cfg(feature = "hybrid")]
    fn decode_candidate_layout(
        packets: &[Vec<u8>],
        oracle: &std::ffi::OsStr,
        channels: usize,
    ) -> Vec<f32> {
        decode_candidate_rate(packets, oracle, channels, 48000)
    }

    #[cfg(feature = "hybrid")]
    fn decode_candidate_rate(
        packets: &[Vec<u8>],
        oracle: &std::ffi::OsStr,
        channels: usize,
        rate: u32,
    ) -> Vec<f32> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/output_oracle.py"
            ))
            .args(["--decode", &rate.to_string(), &channels.to_string()])
            .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", oracle)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&packet_wire(packets))
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        output
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect()
    }

    #[cfg(feature = "hybrid")]
    fn candidate_hybrid_music(frame: usize, frame_size: usize) -> Vec<f32> {
        (0..frame_size)
            .flat_map(|i| {
                let sample = frame * frame_size + i;
                let t = sample as f32 / 48000.0;
                (0..2).map(move |channel| {
                    let tau = std::f32::consts::TAU;
                    let base = 0.16 * (tau * (440.0 + channel as f32 * 220.0) * t).sin()
                        + 0.07 * (tau * 1320.0 * t).sin();
                    if (300..420).contains(&(sample % 960)) {
                        base + 0.2 * (tau * 9000.0 * t).sin()
                    } else {
                        base
                    }
                })
            })
            .collect()
    }

    #[test]
    fn mono_short_frame_candidate_budget_never_overflows() {
        candidate_budget(120, 64000);
    }

    #[test]
    fn mono_five_ms_candidate_budget_never_overflows() {
        candidate_budget(240, 32000);
    }

    fn candidate_budget(frame_size: usize, bitrate: u32) {
        // Exercise the candidate budget without relaxing the public constructor
        // until both this stress case and independent reference decode qualify.
        for kind in 0..5 {
            let mut encoder = Encoder::new(EncoderConfig {
                channels: Channels::Mono,
                frame_size,
                ..EncoderConfig::default()
            })
            .unwrap();
            encoder.config.bitrate = bitrate;
            let mut random = 12345_u32;
            for frame in 0..80 {
                let pcm: Vec<_> = (0..frame_size)
                    .map(|i| {
                        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let t = (frame * frame_size + i) as f32 / 48000.0;
                        match kind {
                            0 => 0.0,
                            1 => 0.2 * (std::f32::consts::TAU * 440.0 * t).sin(),
                            2 => {
                                if i == 60 && frame % 3 == 0 {
                                    0.8
                                } else {
                                    0.0
                                }
                            }
                            3 => (f64::from(random) / f64::from(u32::MAX) * 0.4 - 0.2) as f32,
                            _ => 0.8 * (std::f32::consts::TAU * (200.0 * t + 6000.0 * t * t)).sin(),
                        }
                    })
                    .collect();
                let packet = encoder
                    .encode(&pcm)
                    .unwrap_or_else(|e| panic!("kind={kind} frame={frame}: {e}"));
                assert_eq!(packet.len(), 20);
            }
            assert_eq!(encoder.finish().unwrap().len(), 1);
        }
    }

    #[cfg(feature = "silk")]
    #[test]
    #[ignore = "explicit MOOSICBOX_OPUS_REFERENCE_ORACLE required"]
    fn reference_high_rate_long_silk_candidate_duration_and_quality() {
        use crate::SampleRate;
        use std::io::Write;
        use std::process::{Command, Stdio};
        let oracle = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE")
            .expect("explicit reference required");
        for sample_rate in [SampleRate::Hz24000, SampleRate::Hz48000] {
            let rate = sample_rate as usize;
            for channels in [Channels::Mono, Channels::Stereo] {
                let count = if channels == Channels::Mono { 1 } else { 2 };
                for duration in [40, 60] {
                    // Test-only internal framing override, not a shipped bypass.
                    let mut encoder = Encoder::new_silk(EncoderConfig {
                        sample_rate,
                        channels,
                        frame_size: rate / 50,
                        bitrate: 128_000,
                    })
                    .unwrap();
                    encoder.config.frame_size = rate * duration / 1000;
                    encoder.silk = Some(
                        super::super::silk::SilkEncoder::new(EncoderConfig {
                            sample_rate: SampleRate::Hz16000,
                            channels,
                            frame_size: 16 * duration,
                            bitrate: 128_000,
                        })
                        .unwrap(),
                    );
                    let delay = encoder.lookahead();
                    let frame_size = encoder.config.frame_size;
                    let mut source = Vec::new();
                    let mut packets = Vec::new();
                    for frame in 0..12 {
                        let pcm: Vec<_> = (0..frame_size)
                            .flat_map(|i| {
                                let t = (frame * frame_size + i) as f32 / rate as f32;
                                (0..count).map(move |c| {
                                    0.2 * (std::f32::consts::TAU * (240.0 * t + 800.0 * t * t))
                                        .sin()
                                        + 0.04
                                            * (std::f32::consts::TAU
                                                * (997.0 + c as f32 * 113.0)
                                                * t)
                                                .sin()
                                })
                            })
                            .collect();
                        source.extend_from_slice(&pcm);
                        let packet = encoder.encode(&pcm).unwrap();
                        assert!(packet.len() * 8 * rate <= 128_000 * frame_size);
                        assert_eq!(packet[0] & 3, 3);
                        assert_eq!(usize::from(packet[1] & 0x3f), duration / 20);
                        packets.push(packet);
                    }
                    packets.extend(encoder.finish().unwrap());
                    assert_eq!(packets.len(), 13);
                    assert_eq!(encoder.finish().unwrap().len(), 0);
                    let wire = packet_wire(&packets);
                    let mut child = Command::new("python3")
                        .arg("-B")
                        .arg(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/tests/output_oracle.py"
                        ))
                        .args(["--decode", &rate.to_string(), &count.to_string()])
                        .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", &oracle)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::inherit())
                        .spawn()
                        .unwrap();
                    child.stdin.take().unwrap().write_all(&wire).unwrap();
                    let output = child.wait_with_output().unwrap();
                    assert!(output.status.success());
                    let decoded: Vec<_> = output
                        .stdout
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                        .collect();
                    assert_eq!(decoded.len(), packets.len() * frame_size * count);
                    let aligned = &decoded[delay * count..delay * count + source.len()];
                    let range = frame_size * count..source.len() - frame_size * count;
                    let signal: f64 = source[range.clone()]
                        .iter()
                        .map(|&x| f64::from(x).powi(2))
                        .sum();
                    let error: f64 = range
                        .map(|i| f64::from(source[i] - aligned[i]).powi(2))
                        .sum();
                    let snr = 10.0 * (signal / error).log10();
                    assert!(
                        snr > 8.0,
                        "{rate} {channels:?} {duration}ms delay={delay} SNR={snr}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "explicit MOOSICBOX_OPUS_REFERENCE_ORACLE required"]
    fn reference_mono_short_frame_candidate_quality() {
        candidate_reference_quality(120, 64000);
    }

    #[test]
    #[ignore = "explicit MOOSICBOX_OPUS_REFERENCE_ORACLE required"]
    fn reference_mono_five_ms_candidate_quality() {
        candidate_reference_quality(240, 32000);
    }

    fn candidate_reference_quality(frame_size: usize, bitrate: u32) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let oracle = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE")
            .expect("explicit reference required");
        let mut encoder = Encoder::new(EncoderConfig {
            channels: Channels::Mono,
            frame_size,
            ..EncoderConfig::default()
        })
        .unwrap();
        encoder.config.bitrate = bitrate;
        let mut source = Vec::new();
        let mut packets = Vec::new();
        for frame in 0..160 {
            let pcm: Vec<_> = (0..frame_size)
                .map(|i| {
                    let t = (frame * frame_size + i) as f32 / 48000.0;
                    0.2 * (std::f32::consts::TAU * (250.0 * t + 1800.0 * t * t)).sin()
                        + 0.05 * (std::f32::consts::TAU * 1440.0 * t).sin()
                })
                .collect();
            source.extend_from_slice(&pcm);
            packets.push(encoder.encode(&pcm).unwrap());
        }
        packets.extend(encoder.finish().unwrap());
        let wire = packet_wire(&packets);
        let mut child = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/output_oracle.py"
            ))
            .args(["--decode", "48000", "1"])
            .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", oracle)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&wire).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let decoded: Vec<_> = output
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        assert_eq!(decoded.len(), packets.len() * frame_size);
        let aligned = &decoded[120..120 + source.len()];
        let signal: f64 = source[960..].iter().map(|&x| f64::from(x).powi(2)).sum();
        let error: f64 = source[960..]
            .iter()
            .zip(&aligned[960..])
            .map(|(&x, &y)| f64::from(x - y).powi(2))
            .sum();
        let snr = 10.0 * (signal / error).log10();
        assert!(
            snr > 8.0,
            "short mono bitrate={bitrate} size={frame_size} SNR={snr}"
        );
    }
}
