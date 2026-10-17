//! Native encoder contracts and independent public-libopus interoperability.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig};
use std::io::Write;
use std::process::{Command, Stdio};

fn channel_layout(channels: usize) -> Channels {
    if channels == 1 {
        Channels::Mono
    } else {
        Channels::Stereo
    }
}

fn signal(frames: usize, channels: usize, kind: usize) -> Vec<f32> {
    let mut random = 17_u32;
    (0..frames * 960)
        .flat_map(|i| {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (random as i32 as f32) / 2147483648.0;
            (0..channels).map(move |c| {
                let t = i as f32 / 48000.0;
                let tau = std::f32::consts::TAU;
                match kind {
                    0 => {
                        0.25 * (tau * (if c == 0 { 440.0 } else { 660.0 }) * t).sin()
                            + 0.08 * (tau * 1200.0 * t).sin()
                    }
                    1 => 0.12 * noise,
                    2 => 0.2 * (tau * (200.0 * t + 10000.0 * t * t)).sin(),
                    3 => {
                        if i % 997 == 0 {
                            0.8
                        } else {
                            0.0
                        }
                    }
                    4 => 0.0,
                    5 => 0.999 * (tau * 997.0 * t).sin(),
                    _ => unreachable!(),
                }
            })
        })
        .collect()
}

fn packets(input: &[f32], channels: usize, bitrate: u32) -> (Vec<Vec<u8>>, usize) {
    let mut encoder = Encoder::new(EncoderConfig {
        channels: channel_layout(channels),
        bitrate,
        ..EncoderConfig::default()
    })
    .unwrap();
    let mut packets = Vec::new();
    for frame in input.chunks_exact(960 * channels) {
        packets.push(encoder.encode(frame).unwrap_or_else(|error| {
            panic!(
                "channels={channels} bitrate={bitrate} frame={} {error}",
                packets.len()
            )
        }));
    }
    packets.extend(encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    (packets, encoder.lookahead())
}

fn native_decode(packets: &[Vec<u8>], channels: usize) -> Vec<i16> {
    let mut native = Decoder::new(
        EncoderConfig::default().sample_rate,
        channel_layout(channels),
    )
    .unwrap();
    let mut pcm = Vec::new();
    for packet in packets {
        let mut output = vec![0_i16; 960 * channels];
        let frames = native.decode(Some(packet), &mut output, false).unwrap();
        assert_eq!(frames, 960);
        pcm.extend(output);
    }
    pcm
}

#[test]
fn validates_input_and_finish_state() {
    let mut encoder = Encoder::new(EncoderConfig::default()).unwrap();
    assert!(encoder.encode(&[0.0]).is_err());
    for value in [f32::NAN, f32::INFINITY, 1.01, -1.01] {
        let mut invalid = vec![0.0; 1920];
        invalid[0] = value;
        assert!(encoder.encode(&invalid).is_err());
    }
    assert!(encoder.finish().unwrap().is_empty());
    assert!(encoder.finish().unwrap().is_empty());
    assert!(encoder.encode(&vec![0.0; 1920]).is_err());
}

#[test]
fn integer_encoder_is_deterministic_across_signals_and_budgets() {
    for channels in [1, 2] {
        for bitrate in [32000, 64000, 128000, 256000] {
            for kind in 0..6 {
                let input = signal(3, channels, kind);
                let (first, delay) = packets(&input, channels, bitrate);
                let (second, _) = packets(&input, channels, bitrate);
                assert_eq!(
                    first, second,
                    "channels={channels} bitrate={bitrate} signal={kind}"
                );
                assert_eq!(delay, 120);
                assert!(first.iter().all(|p| p.len() == bitrate as usize / 400));
                let pcm = native_decode(&first, channels);
                if kind == 4 {
                    assert!(pcm.iter().all(|x| x.abs() <= 2));
                } else {
                    assert!(pcm.iter().any(|x| x.abs() > 100));
                }
            }
        }
    }
}

fn oracle_decode(packets: &[Vec<u8>], channels: usize, rate: usize) -> Vec<i16> {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE")
        .expect("live oracle requires MOOSICBOX_OPUS_REFERENCE_ORACLE; see libopus/README.md");
    let mut stream = Vec::new();
    for packet in packets {
        stream.extend_from_slice(&(packet.len() as u32).to_le_bytes());
        stream.extend_from_slice(packet);
    }
    let mut child = Command::new("python3")
        .arg("-B")
        .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", executable)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/output_oracle.py"
        ))
        .args(["--decode", &rate.to_string(), &channels.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&stream).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    result
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|x| i16::from_le_bytes(*x))
        .collect()
}

/// Explicit opt-in only for the external C build prerequisite, not for native
/// encoder acceptance: all native signal/budget tests above always run.
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_decodes_nonzero_audio() {
    for channels in [1, 2] {
        for kind in 0..6 {
            let input = signal(12, channels, kind);
            let (packets, lookahead) = packets(&input, channels, 128000);
            let decoded = oracle_decode(&packets, channels, 48000);
            let native = native_decode(&packets, channels);
            assert_eq!(decoded.len(), packets.len() * 960 * channels);
            let max = native
                .iter()
                .zip(&decoded)
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
                .max()
                .unwrap();
            assert!(
                max <= 2,
                "native/reference difference={max} channels={channels} signal={kind}"
            );
            let mut noise = 0.0;
            let mut energy = 0.0;
            for (original, &reconstructed) in input.iter().zip(&decoded[lookahead * channels..]) {
                energy += f64::from(*original).powi(2);
                noise += (f64::from(*original) - f64::from(reconstructed) / 32768.0).powi(2);
            }
            let snr = 10.0 * (energy / noise).log10();
            if kind == 2 {
                let mut best = (-100.0, 0);
                for shift in 0..240 {
                    let error = input
                        .iter()
                        .zip(&decoded[shift * channels..])
                        .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                        .sum::<f64>();
                    let score = 10.0 * (energy / error).log10();
                    if score > best.0 {
                        best = (score, shift);
                    }
                }
                eprintln!("chirp best alignment {best:?}");
            }
            let rms = (decoded
                .iter()
                .map(|&x| (f64::from(x) / 32768.0).powi(2))
                .sum::<f64>()
                / decoded.len() as f64)
                .sqrt();
            eprintln!(
                "channels={channels} signal={kind} bitrate=128000 rms={rms:.6} snr={snr:.3} dB delay={lookahead} reference_max={max}"
            );
            if kind == 4 {
                assert!(rms < 0.0001);
            } else {
                assert!(rms > 0.001, "effectively silent audio");
                // Quality floors are signal-specific: white noise and isolated
                // impulses stress the intentionally simple long-window encoder.
                let floor = match kind {
                    0 => 10.0,
                    1 | 3 => 0.0,
                    2 => 5.0,
                    5 => 10.0,
                    _ => unreachable!(),
                };
                assert!(
                    snr > floor,
                    "signal {kind}: unacceptable reconstruction {snr} dB (minimum {floor})"
                );
            }
        }
    }
}

#[test]
fn native_durations_have_correct_framing_and_sample_counts() {
    for channels in [1, 2] {
        for frame_size in [120, 240, 480, 960] {
            for bitrate in [(15_360_000 / frame_size).max(32000) as u32, 128000, 256000] {
                let mut encoder = Encoder::new(EncoderConfig {
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate,
                    ..EncoderConfig::default()
                })
                .unwrap();
                let mut decoder = Decoder::new(
                    EncoderConfig::default().sample_rate,
                    channel_layout(channels),
                )
                .unwrap();
                let input = signal(2, channels, 0);
                let mut all = Vec::new();
                for frame in input.chunks_exact(frame_size * channels) {
                    let packet = encoder.encode(frame).unwrap_or_else(|e| {
                        panic!("frame={frame_size} channels={channels} bitrate={bitrate}: {e}")
                    });
                    let mut out = vec![0_i16; frame_size * channels];
                    assert_eq!(
                        decoder.decode(Some(&packet), &mut out, false).unwrap(),
                        frame_size
                    );
                    all.extend(out);
                }
                assert!(all.iter().any(|x| x.abs() > 100));
            }
        }
    }
}

#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_rates_and_durations() {
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [1, 2] {
            for divisor in [400, 200, 100, 50] {
                let frame_size = rate / divisor;
                let mut encoder = Encoder::new(EncoderConfig {
                    sample_rate: moosicbox_opus_native::SampleRate::from_hz(rate as u32).unwrap(),
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate: 128000,
                })
                .unwrap();
                let input: Vec<f32> = (0..rate / 10)
                    .flat_map(|i| {
                        (0..channels).map(move |c| {
                            0.25 * (std::f32::consts::TAU * (440.0 + 220.0 * c as f32) * i as f32
                                / rate as f32)
                                .sin()
                        })
                    })
                    .collect();
                let mut packets = Vec::new();
                for frame in input.chunks_exact(frame_size * channels) {
                    packets.push(encoder.encode(frame).unwrap());
                }
                packets.extend(encoder.finish().unwrap());
                let decoded = oracle_decode(&packets, channels, rate);
                let mut native =
                    Decoder::new(encoder.config().sample_rate, channel_layout(channels)).unwrap();
                let mut native_pcm = Vec::new();
                for packet in &packets {
                    let mut out = vec![0; frame_size * channels];
                    assert_eq!(
                        native.decode(Some(packet), &mut out, false).unwrap(),
                        frame_size
                    );
                    native_pcm.extend(out);
                }
                let max = decoded
                    .iter()
                    .zip(&native_pcm)
                    .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                    .max()
                    .unwrap();
                assert!(
                    max <= 2,
                    "rate={rate} frame={frame_size} channels={channels} max={max}"
                );
                let mut energy = 0.0;
                let mut error = 0.0;
                for (&a, &b) in input.iter().zip(&decoded[encoder.lookahead() * channels..]) {
                    energy += f64::from(a).powi(2);
                    error += (f64::from(a) - f64::from(b) / 32768.0).powi(2);
                }
                let snr = 10.0 * (energy / error).log10();
                eprintln!(
                    "rate={rate} frame={frame_size} channels={channels} snr={snr:.3} delay={}",
                    encoder.lookahead()
                );
                assert!(
                    snr > 10.0,
                    "rate={rate} frame={frame_size} channels={channels} snr={snr}"
                );
            }
        }
    }
}

#[cfg(feature = "silk")]
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_silk_audio_quality() {
    for rate in [8000, 12000, 16000] {
        for divisor in [100, 50] {
            for kind in 0..4 {
                let frame_size = rate / divisor;
                let config = EncoderConfig {
                    sample_rate: moosicbox_opus_native::SampleRate::from_hz(rate as u32).unwrap(),
                    channels: Channels::Mono,
                    frame_size,
                    bitrate: 48000,
                };
                let mut encoder = Encoder::new_silk(config).unwrap();
                let mut random = 17_u32;
                let input: Vec<f32> = (0..rate / 5)
                    .map(|i| {
                        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                        let t = i as f32 / rate as f32;
                        match kind {
                            0 => 0.2 * (std::f32::consts::TAU * 440.0 * t).sin(),
                            1 => 0.2 * (std::f32::consts::TAU * (200.0 * t + 1000.0 * t * t)).sin(),
                            2 => 0.1 * (random as i32 as f32) / 2147483648.0,
                            _ => 0.0,
                        }
                    })
                    .collect();
                let mut packets = Vec::new();
                for frame in input.chunks_exact(frame_size) {
                    packets.push(encoder.encode(frame).unwrap());
                }
                packets.extend(encoder.finish().unwrap());
                let decoded = oracle_decode(&packets, 1, rate);
                let mut native = Decoder::new(config.sample_rate, Channels::Mono).unwrap();
                let mut native_pcm = Vec::new();
                for packet in &packets {
                    let mut out = vec![0; frame_size];
                    assert_eq!(
                        native.decode(Some(packet), &mut out, false).unwrap(),
                        frame_size
                    );
                    native_pcm.extend(out);
                }
                let max = decoded
                    .iter()
                    .zip(&native_pcm)
                    .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                    .max()
                    .unwrap();
                assert!(
                    max <= 2,
                    "SILK native/C mismatch rate={rate} frame={frame_size} max={max}"
                );
                let mut energy = 0.0;
                let mut error = 0.0;
                for (&a, &b) in input.iter().zip(&decoded[encoder.lookahead()..]) {
                    energy += f64::from(a).powi(2);
                    error += (f64::from(a) - f64::from(b) / 32768.0).powi(2);
                }
                let snr = 10.0 * (energy / error).log10();
                eprintln!(
                    "SILK rate={rate} frame={frame_size} signal={kind} snr={snr:.3} delay={} reference_max={max}",
                    encoder.lookahead()
                );
                if kind < 3 {
                    assert!(
                        snr > if kind == 2 { 0.0 } else { 10.0 },
                        "SILK quality {snr}"
                    );
                } else {
                    let rms = (decoded.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>()
                        / decoded.len() as f64)
                        .sqrt();
                    assert!(rms < 100.0, "SILK silence RMS {rms}");
                }
            }
        }
    }
}

#[cfg(feature = "silk")]
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_silk_stereo() {
    for rate in [8000, 12000, 16000] {
        for divisor in [100, 50] {
            let config = EncoderConfig {
                sample_rate: moosicbox_opus_native::SampleRate::from_hz(rate as u32).unwrap(),
                channels: Channels::Stereo,
                frame_size: rate / divisor,
                bitrate: 96000,
            };
            let mut encoder = Encoder::new_silk(config).unwrap();
            let input: Vec<f32> = (0..rate / 5)
                .flat_map(|i| {
                    (0..2).map(move |c| {
                        0.2 * (std::f32::consts::TAU * (440.0 + 220.0 * c as f32) * i as f32
                            / rate as f32)
                            .sin()
                    })
                })
                .collect();
            let mut packets = Vec::new();
            for frame in input.chunks_exact(config.frame_size * 2) {
                packets.push(encoder.encode(frame).unwrap());
            }
            packets.extend(encoder.finish().unwrap());
            let decoded = oracle_decode(&packets, 2, rate);
            let mut native = Decoder::new(config.sample_rate, Channels::Stereo).unwrap();
            let mut native_pcm = Vec::new();
            for packet in &packets {
                let mut out = vec![0; config.frame_size * 2];
                assert_eq!(
                    native.decode(Some(packet), &mut out, false).unwrap(),
                    config.frame_size
                );
                native_pcm.extend(out);
            }
            let max = decoded
                .iter()
                .zip(&native_pcm)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(
                max <= 2,
                "stereo SILK native/C mismatch rate={rate} max={max}"
            );
            let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
            let error = input
                .iter()
                .zip(&decoded[encoder.lookahead() * 2..])
                .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                .sum::<f64>();
            let snr = 10.0 * (energy / error).log10();
            eprintln!(
                "SILK stereo rate={rate} frame={} snr={snr:.3} delay={} reference_max={max}",
                config.frame_size,
                encoder.lookahead()
            );
            assert!(snr > 10.0, "SILK stereo quality {snr}");
        }
    }
}

#[cfg(feature = "hybrid")]
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_hybrid_quality() {
    for channels in [1, 2] {
        for frame_size in [480, 960] {
            for kind in 0..4 {
                let config = EncoderConfig {
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate: 128000,
                    ..EncoderConfig::default()
                };
                let mut encoder = Encoder::new_hybrid(config).unwrap();
                let mut random = 17_u32;
                let input: Vec<f32> = (0..9600)
                    .flat_map(|i| {
                        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                        let noise = (random as i32 as f32) / 2147483648.0;
                        (0..channels).map(move |c| {
                            let t = i as f32 / 48000.0;
                            let tau = std::f32::consts::TAU;
                            match kind {
                                0 => {
                                    0.2 * (tau * (440.0 + 220.0 * c as f32) * t).sin()
                                        + 0.05 * (tau * 10000.0 * t).sin()
                                }
                                1 => 0.12 * noise,
                                2 => 0.2 * (tau * (200.0 * t + 25000.0 * t * t)).sin(),
                                _ => 0.0,
                            }
                        })
                    })
                    .collect();
                let mut packets = Vec::new();
                for frame in input.chunks_exact(frame_size * channels) {
                    packets.push(encoder.encode(frame).unwrap_or_else(|e| panic!("hybrid channels={channels} size={frame_size} signal={kind} packet={} {e}",packets.len())));
                }
                packets.extend(encoder.finish().unwrap());
                let decoded = oracle_decode(&packets, channels, 48000);
                let mut native = Decoder::new(config.sample_rate, config.channels).unwrap();
                let mut native_pcm = Vec::new();
                for packet in &packets {
                    let mut out = vec![0; frame_size * channels];
                    assert_eq!(
                        native.decode(Some(packet), &mut out, false).unwrap(),
                        frame_size
                    );
                    native_pcm.extend(out);
                }
                let max = decoded
                    .iter()
                    .zip(&native_pcm)
                    .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                    .max()
                    .unwrap();
                assert!(max <= 2, "hybrid C/native mismatch {max}");
                let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                let error = input
                    .iter()
                    .zip(&decoded[encoder.lookahead() * channels..])
                    .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                    .sum::<f64>();
                let snr = 10.0 * (energy / error).log10();
                eprintln!(
                    "hybrid channels={channels} frame={frame_size} signal={kind} snr={snr:.3} delay={} reference_max={max}",
                    encoder.lookahead()
                );
                if kind < 3 {
                    assert!(
                        snr > if kind == 1 { 0.0 } else { 10.0 },
                        "hybrid quality {snr}"
                    );
                } else {
                    let rms = (decoded.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>()
                        / decoded.len() as f64)
                        .sqrt();
                    assert!(rms < 100.0, "hybrid silence RMS {rms}");
                }
            }
        }
    }
}

#[cfg(feature = "silk")]
#[test]
fn silk_encoding_is_deterministic_and_preserves_input_contract() {
    for rate in [8000, 12000, 16000] {
        for channels in [1, 2] {
            for divisor in [100, 50] {
                let config = EncoderConfig {
                    sample_rate: moosicbox_opus_native::SampleRate::from_hz(rate as u32).unwrap(),
                    channels: channel_layout(channels),
                    frame_size: rate / divisor,
                    bitrate: 48000 * channels as u32,
                };
                let mut first = Encoder::new_silk(config).unwrap();
                let mut second = Encoder::new_silk(config).unwrap();
                assert!(first.encode(&[f32::NAN]).is_err());
                let input: Vec<f32> = (0..config.frame_size * 4)
                    .flat_map(|i| {
                        (0..channels).map(move |c| {
                            0.2 * (std::f32::consts::TAU * (440.0 + 220.0 * c as f32) * i as f32
                                / rate as f32)
                                .sin()
                        })
                    })
                    .collect();
                let mut decoder = Decoder::new(config.sample_rate, config.channels).unwrap();
                for frame in input.chunks_exact(config.frame_size * channels) {
                    let packet = first.encode(frame).unwrap();
                    assert_eq!(packet, second.encode(frame).unwrap());
                    let mut output = vec![0; config.frame_size * channels];
                    assert_eq!(
                        decoder.decode(Some(&packet), &mut output, false).unwrap(),
                        config.frame_size
                    );
                    assert!(output.iter().any(|x| x.abs() > 100));
                }
                assert_eq!(first.finish().unwrap(), second.finish().unwrap());
                assert!(first.finish().unwrap().is_empty());
                assert!(
                    first
                        .encode(&vec![0.0; config.frame_size * channels])
                        .is_err()
                );
            }
        }
    }
}

#[cfg(feature = "hybrid")]
#[test]
fn hybrid_is_deterministic_and_drains_queried_delay() {
    for channels in [1, 2] {
        for frame_size in [480, 960] {
            for bitrate in [128_000, 256_000] {
                let config = EncoderConfig {
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate,
                    ..EncoderConfig::default()
                };
                let mut first = Encoder::new_hybrid(config).unwrap();
                let mut second = Encoder::new_hybrid(config).unwrap();
                assert_eq!(first.lookahead(), 384);
                let input = signal(2, channels, 0);
                let mut decoder = Decoder::new(config.sample_rate, config.channels).unwrap();
                for frame in input.chunks_exact(frame_size * channels) {
                    let packet = first.encode(frame).unwrap();
                    assert_eq!(packet, second.encode(frame).unwrap());
                    let mut output = vec![0; frame_size * channels];
                    assert_eq!(
                        decoder.decode(Some(&packet), &mut output, false).unwrap(),
                        frame_size
                    );
                    assert!(output.iter().any(|x| x.abs() > 100));
                }
                let tail = first.finish().unwrap();
                assert_eq!(tail, second.finish().unwrap());
                assert_eq!(tail.len(), first.lookahead().div_ceil(frame_size));
                assert!(first.finish().unwrap().is_empty());
                assert!(first.encode(&vec![0.0; frame_size * channels]).is_err());
            }
        }
    }
}

#[cfg(feature = "silk")]
#[test]
fn silk_low_bitrate_stereo_regression() {
    let config = EncoderConfig {
        sample_rate: moosicbox_opus_native::SampleRate::Hz8000,
        channels: Channels::Stereo,
        frame_size: 80,
        bitrate: 32000,
    };
    assert!(
        Encoder::new_silk(config).is_err(),
        "unsupported low-rate stereo must reject before encoding"
    );
}

#[cfg(feature = "silk")]
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_silk_bitrate_boundaries() {
    for rate in [8000, 12000, 16000] {
        for channels in [1, 2] {
            for frame_size in [rate / 100, rate / 50] {
                for bitrate in [if channels == 1 { 48000 } else { 96000 }, 128000] {
                    let config = EncoderConfig {
                        sample_rate: moosicbox_opus_native::SampleRate::from_hz(rate as u32)
                            .unwrap(),
                        channels: channel_layout(channels),
                        frame_size,
                        bitrate,
                    };
                    let mut encoder = Encoder::new_silk(config).unwrap();
                    let input: Vec<f32> = (0..rate / 10)
                        .flat_map(|i| {
                            (0..channels).map(move |c| {
                                0.2 * (std::f32::consts::TAU
                                    * (330.0 + 220.0 * c as f32)
                                    * i as f32
                                    / rate as f32)
                                    .sin()
                            })
                        })
                        .collect();
                    let mut packets = Vec::new();
                    for frame in input.chunks_exact(frame_size * channels) {
                        packets.push(encoder.encode(frame).unwrap());
                    }
                    packets.extend(encoder.finish().unwrap());
                    assert!(
                        packets
                            .iter()
                            .all(|p| p.len() <= bitrate as usize * frame_size / (rate * 8))
                    );
                    let decoded = oracle_decode(&packets, channels, rate);
                    let mut decoder = Decoder::new(config.sample_rate, config.channels).unwrap();
                    for (i, packet) in packets.iter().enumerate() {
                        let mut out = vec![0; frame_size * channels];
                        decoder.decode(Some(packet), &mut out, false).unwrap();
                        assert_eq!(
                            out,
                            &decoded[i * frame_size * channels..(i + 1) * frame_size * channels]
                        );
                    }
                    let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                    let error = input
                        .iter()
                        .zip(&decoded[encoder.lookahead() * channels..])
                        .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                        .sum::<f64>();
                    let snr = 10.0 * (energy / error).log10();
                    assert!(
                        snr > 10.0,
                        "rate={rate} channels={channels} size={frame_size} bitrate={bitrate} SNR={snr}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_celt_bitrate_boundaries() {
    for channels in [1, 2] {
        for frame_size in [120, 240, 480, 960] {
            for bitrate in [(15_360_000 / frame_size).max(32000) as u32, 256000] {
                let config = EncoderConfig {
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate,
                    ..EncoderConfig::default()
                };
                let mut encoder = Encoder::new(config).unwrap();
                let input = signal(4, channels, 0);
                let mut packets = Vec::new();
                for frame in input.chunks_exact(frame_size * channels) {
                    packets.push(encoder.encode(frame).unwrap());
                }
                packets.extend(encoder.finish().unwrap());
                let decoded = oracle_decode(&packets, channels, 48000);
                let mut native = Decoder::new(config.sample_rate, config.channels).unwrap();
                for (i, packet) in packets.iter().enumerate() {
                    assert_eq!(packet.len(), bitrate as usize * frame_size / 384000);
                    let mut out = vec![0; frame_size * channels];
                    native.decode(Some(packet), &mut out, false).unwrap();
                    assert_eq!(
                        out,
                        &decoded[i * frame_size * channels..(i + 1) * frame_size * channels]
                    );
                }
                let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                let error = input
                    .iter()
                    .zip(&decoded[encoder.lookahead() * channels..])
                    .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                    .sum::<f64>();
                let snr = 10.0 * (energy / error).log10();
                assert!(
                    snr > 0.0,
                    "CELT boundary channels={channels} frame={frame_size} bitrate={bitrate} SNR={snr}"
                );
            }
        }
    }
}

#[cfg(feature = "hybrid")]
#[test]
#[ignore = "set MOOSICBOX_OPUS_REFERENCE_ORACLE; run with --include-ignored"]
fn public_libopus_hybrid_bitrate_boundaries() {
    for channels in [1, 2] {
        for frame_size in [480, 960] {
            for bitrate in [128000, 256000] {
                let config = EncoderConfig {
                    channels: channel_layout(channels),
                    frame_size,
                    bitrate,
                    ..EncoderConfig::default()
                };
                let mut encoder = Encoder::new_hybrid(config).unwrap();
                let input = signal(4, channels, 0);
                let mut packets = Vec::new();
                for frame in input.chunks_exact(frame_size * channels) {
                    packets.push(encoder.encode(frame).unwrap());
                }
                packets.extend(encoder.finish().unwrap());
                let decoded = oracle_decode(&packets, channels, 48000);
                let mut native = Decoder::new(config.sample_rate, config.channels).unwrap();
                for (i, packet) in packets.iter().enumerate() {
                    let mut out = vec![0; frame_size * channels];
                    native.decode(Some(packet), &mut out, false).unwrap();
                    assert_eq!(
                        out,
                        &decoded[i * frame_size * channels..(i + 1) * frame_size * channels]
                    );
                }
                let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                let error = input
                    .iter()
                    .zip(&decoded[encoder.lookahead() * channels..])
                    .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                    .sum::<f64>();
                let snr = 10.0 * (energy / error).log10();
                assert!(
                    snr > 10.0,
                    "hybrid boundary channels={channels} frame={frame_size} bitrate={bitrate} SNR={snr}"
                );
            }
        }
    }
}

#[test]
fn unsupported_bitrate_ranges_reject_at_construction() {
    for (frame_size, minimum) in [(120, 128_000), (240, 64_000), (480, 32_000), (960, 32_000)] {
        assert!(
            Encoder::new(EncoderConfig {
                frame_size,
                bitrate: minimum - 1,
                ..EncoderConfig::default()
            })
            .is_err()
        );
        assert!(
            Encoder::new(EncoderConfig {
                frame_size,
                bitrate: minimum,
                ..EncoderConfig::default()
            })
            .is_ok()
        );
    }
    #[cfg(feature = "silk")]
    for (channels, minimum) in [(Channels::Mono, 48_000), (Channels::Stereo, 96_000)] {
        let config = EncoderConfig {
            sample_rate: moosicbox_opus_native::SampleRate::Hz8000,
            channels,
            frame_size: 80,
            bitrate: minimum,
        };
        assert!(
            Encoder::new_silk(EncoderConfig {
                bitrate: minimum - 1,
                ..config
            })
            .is_err()
        );
        assert!(Encoder::new_silk(config).is_ok());
        assert!(
            Encoder::new_silk(EncoderConfig {
                bitrate: 128_001,
                ..config
            })
            .is_err()
        );
    }
    #[cfg(feature = "hybrid")]
    {
        assert!(
            Encoder::new_hybrid(EncoderConfig {
                bitrate: 127_999,
                ..EncoderConfig::default()
            })
            .is_err()
        );
        assert!(Encoder::new_hybrid(EncoderConfig::default()).is_ok());
    }
}
