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
    pub next_celt: bool,
    pub tail: Vec<f32>,
    pub end: usize,
}

impl Adaptive {
    pub(super) const fn new(bitrate: u32) -> Self {
        Self {
            control: EncoderControl {
                bitrate,
                bandwidth: Bandwidth::Fullband,
                mode: EncoderMode::Auto,
            },
            active: EncoderMode::Celt,
            next_celt: false,
            tail: Vec::new(),
            end: 21,
        }
    }

    pub(super) fn target(&self) -> EncoderMode {
        match self.control.mode {
            EncoderMode::Auto if self.control.bandwidth == Bandwidth::Wideband => EncoderMode::Silk,
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
    if control.bandwidth
        != if predictive {
            Bandwidth::Wideband
        } else {
            Bandwidth::Fullband
        }
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
            for shape in 0..4 {
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
        for frame in 0..32 {
            let (mode, bitrate) = match frame {
                0..=3 | 12..=15 | 24..=31 => (EncoderMode::Celt, 64_000),
                4..=11 => (EncoderMode::Hybrid, 32_000),
                _ => (EncoderMode::Hybrid, 64_000),
            };
            let pcm = candidate_hybrid_shape(shape, frame, frame_size, media);
            source.extend_from_slice(&pcm);
            let trace_prefix = matches!(
                (shape, frame_size, frame),
                (3, 480, 4..=6) | (1, 960, 16..=18)
            );
            let incoming_silk = trace_prefix.then(|| encoder.silk.as_ref().unwrap().clone());
            let previous_budget = super::super::hybrid::PREFIX_FRONTIER_BYTES.replace(prefix_bytes);
            let packet = encoder.encode_unredundant_candidate(
                &pcm,
                EncoderControl {
                    bitrate,
                    bandwidth: Bandwidth::Fullband,
                    mode,
                },
            );
            super::super::hybrid::PREFIX_FRONTIER_BYTES.set(previous_budget);
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
        report_unredundant_prefixes(&packets, frame_size, &decoded, &source, encoder.lookahead());
        let aligned = &decoded[encoder.lookahead() * 2..encoder.lookahead() * 2 + source.len()];
        let mut failures = Vec::new();
        // Include each entry/departure, not only settled interior frames.
        for boundary in [4, 12, 16, 24] {
            let range = boundary * frame_size * 2..(boundary + 2) * frame_size * 2;
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
        let prefix_bitrate = super::super::hybrid::packet_lowband_bitrate(
            bitrate,
            encoder.config.channels,
            frame_size,
        );
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
    fn report_unredundant_prefixes(
        packets: &[Vec<u8>],
        frame_size: usize,
        full: &[f32],
        source: &[f32],
        lookahead: usize,
    ) {
        for boundary in [4, 16] {
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
            for (frame, packet) in packets.iter().enumerate().skip(boundary).take(3) {
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
                let reference = &full[frame * frame_size * 2..(frame + 1) * frame_size * 2];
                let low_energy: f64 = low.iter().map(|&x| f64::from(x).powi(2)).sum();
                let residual: f64 = low
                    .iter()
                    .zip(reference)
                    .map(|(&a, &b)| f64::from(a - b).powi(2))
                    .sum();
                let target_start = (frame * frame_size - lookahead) * 2;
                let target = &source[target_start..target_start + frame_size * 2];
                for (label, samples) in [("head5ms", 0..480), ("after5ms", 480..frame_size * 2)] {
                    let mut target_energy = 0.0_f64;
                    let mut target_minus_low = 0.0_f64;
                    let mut output_error = 0.0_f64;
                    let mut added_energy = 0.0_f64;
                    let mut residual_dot_added = 0.0_f64;
                    for i in samples {
                        let t = f64::from(target[i]);
                        let l = f64::from(low[i]);
                        let y = f64::from(reference[i]);
                        target_energy += t * t;
                        target_minus_low += (t - l).powi(2);
                        output_error += (t - y).powi(2);
                        added_energy += (y - l).powi(2);
                        residual_dot_added += (t - l) * (y - l);
                    }
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
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/output_oracle.py"
            ))
            .args(["--decode", "48000", "2"])
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
