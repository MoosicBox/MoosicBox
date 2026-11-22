//! Persistent runtime-control and independently decoded transition contracts.
#![cfg(feature = "celt")]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
#[cfg(feature = "hybrid")]
use moosicbox_opus_native::encoder::{EncoderControl, EncoderMode};
use moosicbox_opus_native::{Bandwidth, Encoder, EncoderConfig};
#[cfg(feature = "hybrid")]
use moosicbox_opus_native::{Channels, Decoder};
#[cfg(feature = "silk")]
use std::io::Write;
#[cfg(feature = "silk")]
use std::process::{Command, Stdio};

#[cfg(feature = "hybrid")]
fn input(frame: usize, channels: usize) -> Vec<f32> {
    (0..960)
        .flat_map(|i| {
            let phase = std::f32::consts::TAU * 440.0 * (frame * 960 + i) as f32 / 48000.0;
            (0..channels).map(move |_| 0.2 * phase.sin())
        })
        .collect()
}

#[cfg(feature = "hybrid")]
fn control(bitrate: u32, mode: EncoderMode) -> EncoderControl {
    EncoderControl {
        bitrate,
        bandwidth: Bandwidth::Fullband,
        mode,
    }
}

#[test]
#[cfg(feature = "hybrid")]
fn practical_hybrid_controls_preserve_mode_after_entry() {
    let mut encoder = Encoder::new_auto(EncoderConfig {
        bitrate: 192000,
        ..EncoderConfig::default()
    })
    .unwrap();
    // Exercise low-budget entry through the practical candidate path, not
    // just a bitrate change after an expensive high-bitrate entry frame.
    encoder
        .set_control(control(32000, EncoderMode::Hybrid))
        .unwrap();
    encoder.encode(&input(0, 2)).unwrap();
    assert_eq!(encoder.active_mode(), EncoderMode::Hybrid);
    for bitrate in [32000, 64000] {
        for mode in [EncoderMode::Hybrid, EncoderMode::Auto] {
            encoder.set_control(control(bitrate, mode)).unwrap();
            encoder.encode(&input(1, 2)).unwrap();
            assert_eq!(encoder.control(), control(bitrate, mode));
            assert_eq!(encoder.active_mode(), EncoderMode::Hybrid);
        }
    }
    encoder
        .set_control(control(32000, EncoderMode::Celt))
        .unwrap();
    encoder.encode(&input(2, 2)).unwrap();
    assert_eq!(encoder.active_mode(), EncoderMode::Celt);
}

#[test]
#[cfg(feature = "hybrid")]
fn explicit_hybrid_control_rejection_preserves_next_packet() {
    for bitrate in [32_000, 128_000, 256_000] {
        let mut encoder = Encoder::new_hybrid(EncoderConfig {
            bitrate,
            ..EncoderConfig::default()
        })
        .unwrap();
        let before = encoder.control();
        let mut reference = encoder.clone();
        for requested in [
            EncoderControl {
                bandwidth: Bandwidth::SuperWideband,
                ..before
            },
            EncoderControl {
                mode: EncoderMode::Auto,
                ..before
            },
        ] {
            assert!(encoder.set_control(requested).is_err());
            assert_eq!(encoder.control(), before);
        }
        let pcm = input(0, 2);
        assert_eq!(
            encoder.encode(&pcm).unwrap(),
            reference.encode(&pcm).unwrap()
        );
    }
}

#[test]
fn explicit_celt_control_reports_encoded_not_input_bandwidth() {
    for sample_rate in [
        moosicbox_opus_native::SampleRate::Hz8000,
        moosicbox_opus_native::SampleRate::Hz12000,
        moosicbox_opus_native::SampleRate::Hz16000,
    ] {
        let frame_size = sample_rate as usize / 50;
        let encoder = Encoder::new(EncoderConfig {
            sample_rate,
            frame_size,
            ..EncoderConfig::default()
        })
        .unwrap();
        assert_eq!(encoder.control().bandwidth, Bandwidth::Fullband);
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE; no automatic C build"]
#[cfg(feature = "hybrid")]
fn reference_hybrid_24k_input_preserves_delay_and_duration() {
    for channels in [Channels::Mono, Channels::Stereo] {
        for frame_size in [240, 480] {
            let count = if channels == Channels::Mono { 1 } else { 2 };
            let mut encoder = Encoder::new_hybrid(EncoderConfig {
                sample_rate: moosicbox_opus_native::SampleRate::Hz24000,
                channels,
                frame_size,
                bitrate: 192000,
            })
            .unwrap();
            assert_eq!(encoder.lookahead(), 204);
            let mut original = Vec::new();
            let mut packets = Vec::new();
            for frame in 0..24 {
                let pcm: Vec<_> = (0..frame_size)
                    .flat_map(|i| {
                        let t = (frame * frame_size + i) as f32 / 24000.0;
                        (0..count).map(move |c| {
                            0.2 * (std::f32::consts::TAU * (440.0 + c as f32 * 110.0) * t).sin()
                        })
                    })
                    .collect();
                original.extend_from_slice(&pcm);
                packets.push(encoder.encode(&pcm).unwrap());
                assert_eq!(encoder.config().frame_size, frame_size);
            }
            packets.extend(encoder.finish().unwrap());
            let output = oracle_decode_rate(&packets, count, 24000);
            assert_eq!(output.len(), packets.len() * frame_size * count);
            let aligned = &output[204 * count..204 * count + original.len()];
            let range = frame_size * 3 * count..original.len() - frame_size * count;
            let signal: f64 = original[range.clone()]
                .iter()
                .map(|&x| f64::from(x).powi(2))
                .sum();
            let error: f64 = range
                .map(|i| f64::from(original[i] - aligned[i]).powi(2))
                .sum();
            assert!(
                10.0 * (signal / error).log10() > 8.0,
                "24k hybrid SNR {}",
                10.0 * (signal / error).log10()
            );
        }
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE; no automatic C build"]
#[cfg(feature = "silk")]
fn reference_silk_high_rate_input_preserves_alignment_and_tail() {
    use moosicbox_opus_native::SampleRate;
    for sample_rate in [SampleRate::Hz24000, SampleRate::Hz48000] {
        let rate = sample_rate as usize;
        for channels in [Channels::Mono, Channels::Stereo] {
            let count = if channels == Channels::Mono { 1 } else { 2 };
            for duration in [10, 20] {
                let frame_size = rate * duration / 1000;
                let mut encoder = Encoder::new_silk(EncoderConfig {
                    sample_rate,
                    channels,
                    frame_size,
                    bitrate: 128000,
                })
                .unwrap();
                let delay = encoder.lookahead();
                eprintln!("SILK {rate} {channels:?} duration={duration} lookahead={delay}");
                assert_eq!(encoder.control().bandwidth, Bandwidth::Wideband);
                let mut packets = Vec::new();
                let mut original = Vec::new();
                for frame in 0..24 {
                    let pcm: Vec<_> = (0..frame_size)
                        .flat_map(|i| {
                            let t = (frame * frame_size + i) as f32 / rate as f32;
                            (0..count).map(move |channel| {
                                let tau = std::f32::consts::TAU;
                                0.18 * (tau * (240.0 * t + 1200.0 * t * t)).sin()
                                    + 0.05 * (tau * (997.0 + channel as f32 * 113.0) * t).sin()
                            })
                        })
                        .collect();
                    original.extend_from_slice(&pcm);
                    packets.push(encoder.encode(&pcm).unwrap());
                }
                packets.extend(encoder.finish().unwrap());
                assert!(encoder.finish().unwrap().is_empty());
                let output = oracle_decode_rate(&packets, count, rate);
                assert_eq!(output.len(), packets.len() * frame_size * count);
                let aligned = &output[delay * count..delay * count + original.len()];
                let range = 3 * frame_size..22 * frame_size;
                let signal: f64 = range
                    .clone()
                    .map(|i| f64::from(original[i * count]).powi(2))
                    .sum();
                let error: f64 = range
                    .clone()
                    .map(|i| f64::from(original[i * count] - aligned[i * count]).powi(2))
                    .sum();
                let snr = 10.0 * (signal / error).log10();
                assert!(snr > 8.0, "SILK {rate} {channels:?} {duration}ms SNR={snr}");
                let best = (-12_i32..=12)
                    .map(|lag| {
                        let error: f64 = range
                            .clone()
                            .map(|i| {
                                let offset = usize::try_from(i as i32 + lag).unwrap();
                                f64::from(original[i * count] - aligned[offset * count]).powi(2)
                            })
                            .sum();
                        (lag, error)
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap()
                    .0;
                assert!(
                    best.abs() <= 2,
                    "SILK {rate} {channels:?} {duration}ms delay mismatch={best}"
                );
            }
        }
    }
}

#[test]
#[cfg(feature = "hybrid")]
fn runtime_bitrate_preserves_history_and_rejects_transactionally() {
    let mut encoder = Encoder::new_auto(EncoderConfig::default()).unwrap();
    let mut clone = encoder.clone();
    assert!(encoder.set_control(control(1, EncoderMode::Celt)).is_err());
    assert_eq!(encoder.control(), clone.control());
    assert_eq!(
        encoder.encode(&input(0, 2)).unwrap(),
        clone.encode(&input(0, 2)).unwrap()
    );
    encoder
        .set_control(control(256000, EncoderMode::Celt))
        .unwrap();
    let continuous = encoder.encode(&input(1, 2)).unwrap();
    let mut fresh = Encoder::new_auto(EncoderConfig {
        bitrate: 256000,
        ..EncoderConfig::default()
    })
    .unwrap();
    assert_ne!(continuous, fresh.encode(&input(1, 2)).unwrap());
    assert_eq!(encoder.lookahead(), 384);
    assert_eq!(encoder.finish().unwrap().len(), 1);
    assert!(encoder.finish().unwrap().is_empty());
    assert!(
        encoder
            .set_control(control(128000, EncoderMode::Celt))
            .is_err()
    );
}

#[cfg(feature = "hybrid")]
fn transitioning(channels: Channels) -> (Vec<Vec<u8>>, Vec<f32>) {
    let count = if channels == Channels::Mono { 1 } else { 2 };
    let mut encoder = Encoder::new_auto(EncoderConfig {
        channels,
        bitrate: 192000,
        ..EncoderConfig::default()
    })
    .unwrap();
    let mut packets = Vec::new();
    let mut original = Vec::new();
    for frame in 0..24 {
        let mode = if !(6..18).contains(&frame) {
            EncoderMode::Celt
        } else {
            EncoderMode::Hybrid
        };
        encoder.set_control(control(192000, mode)).unwrap();
        let pcm = input(frame, count);
        original.extend_from_slice(&pcm);
        let packet = encoder.encode(&pcm).unwrap();
        assert!(packet.len() <= 480);
        packets.push(packet);
    }
    packets.extend(encoder.finish().unwrap());
    (packets, original)
}

#[test]
#[cfg(feature = "hybrid")]
fn mode_changes_keep_packet_duration_and_finish_tail() {
    for channels in [Channels::Mono, Channels::Stereo] {
        let count = if channels == Channels::Mono { 1 } else { 2 };
        let (packets, _) = transitioning(channels);
        assert_eq!(packets.len(), 25);
        let mut decoder = Decoder::new(EncoderConfig::default().sample_rate, channels).unwrap();
        for packet in packets {
            let mut output = vec![0; 960 * count];
            assert_eq!(
                decoder.decode(Some(&packet), &mut output, false).unwrap(),
                960
            );
        }
    }
}

#[test]
#[cfg(feature = "hybrid")]
fn auto_hysteresis_does_not_chatter_between_thresholds() {
    let mut encoder = Encoder::new_auto(EncoderConfig {
        bitrate: 256000,
        ..EncoderConfig::default()
    })
    .unwrap();
    encoder.encode(&input(0, 2)).unwrap();
    assert_eq!(encoder.active_mode(), EncoderMode::Celt);
    for (frame, bitrate) in [192000, 208000, 200000, 208000].into_iter().enumerate() {
        encoder
            .set_control(control(bitrate, EncoderMode::Auto))
            .unwrap();
        encoder.encode(&input(frame + 1, 2)).unwrap();
        assert_eq!(encoder.active_mode(), EncoderMode::Hybrid);
    }
    encoder
        .set_control(control(224000, EncoderMode::Auto))
        .unwrap();
    encoder.encode(&input(5, 2)).unwrap(); // Hybrid bridge carries redundant CELT tail.
    assert_eq!(encoder.active_mode(), EncoderMode::Hybrid);
    encoder.encode(&input(6, 2)).unwrap();
    assert_eq!(encoder.active_mode(), EncoderMode::Celt);
}

#[test]
#[cfg(feature = "hybrid")]
fn wideband_silk_transitions_keep_frame_count() {
    let mut encoder = Encoder::new_auto(EncoderConfig {
        channels: Channels::Mono,
        ..EncoderConfig::default()
    })
    .unwrap();
    let mut decoder = Decoder::new(EncoderConfig::default().sample_rate, Channels::Mono).unwrap();
    for frame in 0..12 {
        let requested = if (3..8).contains(&frame) {
            EncoderControl {
                bitrate: 128000,
                bandwidth: Bandwidth::Wideband,
                mode: EncoderMode::Silk,
            }
        } else {
            control(192000, EncoderMode::Celt)
        };
        encoder.set_control(requested).unwrap();
        let packet = encoder.encode(&input(frame, 1)).unwrap();
        let mut output = vec![0; 960];
        assert_eq!(
            decoder.decode(Some(&packet), &mut output, false).unwrap(),
            960
        );
    }
}

#[cfg(feature = "hybrid")]
fn oracle_decode(packets: &[Vec<u8>], count: usize) -> Vec<f32> {
    oracle_decode_rate(packets, count, 48000)
}

#[cfg(feature = "silk")]
fn oracle_decode_rate(packets: &[Vec<u8>], count: usize, rate: usize) -> Vec<f32> {
    let oracle =
        std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
    let mut wire = Vec::new();
    for packet in packets {
        wire.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
        wire.extend_from_slice(packet);
    }
    let mut child = Command::new("python3")
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/output_oracle.py"
        ))
        .args(["--decode", &rate.to_string(), &count.to_string()])
        .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&wire).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
        .collect()
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE; no automatic C build"]
#[cfg(feature = "hybrid")]
fn reference_silk_and_hybrid_switching_quality() {
    for channels in [Channels::Mono, Channels::Stereo] {
        for frame_size in [480, 960] {
            let count = if channels == Channels::Mono { 1 } else { 2 };
            let mut encoder = Encoder::new_auto(EncoderConfig {
                channels,
                frame_size,
                bitrate: 192000,
                ..EncoderConfig::default()
            })
            .unwrap();
            let mut original = Vec::new();
            let mut packets = Vec::new();
            for frame in 0..48 {
                let requested = match frame / 8 {
                    1 | 4 => EncoderControl {
                        bitrate: 128000,
                        bandwidth: Bandwidth::Wideband,
                        mode: EncoderMode::Silk,
                    },
                    2 => control(192000, EncoderMode::Hybrid),
                    _ => control(192000, EncoderMode::Celt),
                };
                encoder.set_control(requested).unwrap();
                let pcm: Vec<_> = (0..frame_size)
                    .flat_map(|i| {
                        let t = (frame * frame_size + i) as f32 / 48000.0;
                        (0..count).map(move |_| 0.2 * (std::f32::consts::TAU * 440.0 * t).sin())
                    })
                    .collect();
                original.extend_from_slice(&pcm);
                packets.push(encoder.encode(&pcm).unwrap());
            }
            packets.extend(encoder.finish().unwrap());
            let decoded = oracle_decode(&packets, count);
            assert_eq!(decoded.len(), packets.len() * frame_size * count);
            let aligned = &decoded[384 * count..384 * count + original.len()];
            for frame in [3, 11, 19, 27, 35, 43] {
                let mut signal = 0.0_f64;
                let mut error = 0.0_f64;
                for i in frame * frame_size..(frame + 3) * frame_size {
                    let x = f64::from(original[i * count]);
                    let y = f64::from(aligned[i * count]);
                    signal += x * x;
                    error += (x - y) * (x - y);
                }
                let snr = 10.0 * (signal / error).log10();
                assert!(
                    snr > 8.0,
                    "{channels:?} size={frame_size} frame={frame} SNR={snr}"
                );
            }
            for boundary in [8, 16, 24, 32, 40] {
                let boundary = boundary * frame_size;
                let jump = (boundary - 120..boundary + 120)
                    .map(|i| (aligned[i * count] - aligned[(i - 1) * count]).abs())
                    .fold(0.0_f32, f32::max);
                assert!(
                    jump < 0.15,
                    "{channels:?} size={frame_size} boundary={boundary} jump={jump}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE; no automatic C build"]
#[cfg(feature = "hybrid")]
fn reference_nonstationary_content_retains_alignment_across_modes() {
    for channels in [Channels::Mono, Channels::Stereo] {
        let count = if channels == Channels::Mono { 1 } else { 2 };
        let mut encoder = Encoder::new_auto(EncoderConfig {
            channels,
            bitrate: 192000,
            ..EncoderConfig::default()
        })
        .unwrap();
        let mut original = Vec::new();
        let mut packets = Vec::new();
        for frame in 0..36 {
            let requested = match frame / 6 {
                1 | 4 => EncoderControl {
                    bitrate: 128000,
                    bandwidth: Bandwidth::Wideband,
                    mode: EncoderMode::Silk,
                },
                2 => control(192000, EncoderMode::Hybrid),
                _ => control(192000, EncoderMode::Celt),
            };
            encoder.set_control(requested).unwrap();
            let pcm: Vec<_> = (0..960)
                .flat_map(|i| {
                    let t = (frame * 960 + i) as f32 / 48000.0;
                    (0..count).map(move |channel| {
                        let tau = std::f32::consts::TAU;
                        let envelope = 0.12 + 0.06 * (tau * 7.0 * t).sin();
                        let chirp = (tau * (250.0 * t + 1800.0 * t * t)).sin();
                        let harmonics = (tau * (137.0 + channel as f32 * 41.0) * t).sin()
                            + 0.4 * (tau * 1237.0 * t).sin();
                        envelope * (0.6 * chirp + 0.4 * harmonics)
                    })
                })
                .collect();
            original.extend_from_slice(&pcm);
            packets.push(encoder.encode(&pcm).unwrap());
        }
        packets.extend(encoder.finish().unwrap());
        let decoded = oracle_decode(&packets, count);
        let aligned = &decoded[384 * count..384 * count + original.len()];
        for segment in 0..6 {
            let range = (segment * 6 + 2) * 960..(segment * 6 + 5) * 960;
            let signal: f64 = range
                .clone()
                .map(|i| f64::from(original[i * count]).powi(2))
                .sum();
            let error: f64 = range
                .clone()
                .map(|i| f64::from(original[i * count] - aligned[i * count]).powi(2))
                .sum();
            let snr = 10.0 * (signal / error).log10();
            assert!(snr > 6.0, "{channels:?} segment={segment} SNR={snr}");
            // Non-periodic chirp disambiguates delay from a whole tone period.
            // A fixed encoder delay must remain correct before and after switches.
            let best = (-24_i32..=24)
                .map(|lag| {
                    let error: f64 = range
                        .clone()
                        .map(|i| {
                            let offset = usize::try_from(i as i32 + lag).unwrap();
                            f64::from(original[i * count] - aligned[offset * count]).powi(2)
                        })
                        .sum();
                    (lag, error)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap()
                .0;
            assert!(
                best.abs() <= 2,
                "{channels:?} segment={segment} timestamp drift={best}"
            );
        }
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE; no automatic C build"]
#[cfg(feature = "hybrid")]
fn reference_decodes_transitions_with_continuous_timing_and_signal() {
    let oracle =
        std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect("explicit oracle required");
    for channels in [Channels::Mono, Channels::Stereo] {
        let count = if channels == Channels::Mono { 1 } else { 2 };
        let (packets, original) = transitioning(channels);
        let mut wire = Vec::new();
        for packet in &packets {
            wire.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
            wire.extend_from_slice(packet);
        }
        let mut child = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/output_oracle.py"
            ))
            .args(["--decode", "48000", &count.to_string()])
            .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", &oracle)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&wire).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let decoded: Vec<f32> = output
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        assert_eq!(decoded.len(), packets.len() * 960 * count);
        let aligned = &decoded[384 * count..384 * count + original.len()];
        for range in [960..4800, 7000..16000, 19000..22500] {
            let mut error = 0.0_f64;
            let mut signal = 0.0_f64;
            for i in range {
                let x = f64::from(original[i * count]);
                let y = f64::from(aligned[i * count]);
                signal += x * x;
                error += (x - y) * (x - y);
            }
            assert!(
                10.0 * (signal / error).log10() > 8.0,
                "transition segment SNR {}",
                10.0 * (signal / error).log10()
            );
        }
        for boundary in [6 * 960, 18 * 960, 19 * 960] {
            let maximum = (boundary - 120..boundary + 120)
                .map(|i| (aligned[i * count] - aligned[(i - 1) * count]).abs())
                .fold(0.0_f32, f32::max);
            assert!(maximum < 0.15, "transition discontinuity {maximum}");
        }
    }
}
