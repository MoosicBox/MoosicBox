//! CELT transient and practical low-rate stereo completion contracts.
#![cfg(feature = "celt")]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig};
use std::io::Write;
use std::process::{Command, Stdio};

fn music(channels: usize, transient: bool) -> Vec<f32> {
    (0..960 * 12)
        .flat_map(|i| {
            (0..channels).map(move |channel| {
                let t = i as f32 / 48000.0;
                let phase = std::f32::consts::TAU * t;
                let base = 0.16 * (phase * 440.0).sin()
                    + 0.07 * (phase * (if channel == 0 { 1760.0 } else { 1320.0 })).sin();
                if transient && i % 960 >= 300 && i % 960 < 420 {
                    base + 0.25 * (phase * 6000.0).sin()
                } else {
                    base
                }
            })
        })
        .collect()
}

fn encode(channels: usize, bitrate: u32, transient: bool) -> (Vec<f32>, Vec<Vec<u8>>, usize) {
    let mut encoder = Encoder::new(EncoderConfig {
        channels: if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        },
        bitrate,
        ..EncoderConfig::default()
    })
    .unwrap();
    let input = music(channels, transient);
    let mut packets = Vec::new();
    for frame in input.chunks_exact(960 * channels) {
        packets.push(encoder.encode(frame).unwrap());
    }
    packets.extend(encoder.finish().unwrap());
    let budget = (bitrate as usize * 960 / 48000 / 8).min(1276);
    assert!(packets.iter().all(|packet| packet.len() <= budget));
    (input, packets, encoder.lookahead())
}

fn native(packets: &[Vec<u8>], channels: usize) -> Vec<i16> {
    let mut decoder = Decoder::new(
        EncoderConfig::default().sample_rate,
        if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        },
    )
    .unwrap();
    let mut result = Vec::new();
    for packet in packets {
        let mut pcm = vec![0; 960 * channels];
        assert_eq!(decoder.decode(Some(packet), &mut pcm, false).unwrap(), 960);
        result.extend(pcm);
    }
    result
}

#[test]
fn short_duration_low_budget_music_is_not_starved_by_energy_headers() {
    short_duration_music(false);
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn short_duration_low_budget_music_matches_portable_reference() {
    short_duration_music(true);
}

fn short_duration_music(reference: bool) {
    for (channels, frame_size, bitrate) in [
        (1, 120, 64_000),
        (1, 240, 32_000),
        (2, 120, 128_000),
        (2, 240, 64_000),
    ] {
        let mut encoder = Encoder::new(EncoderConfig {
            channels: if channels == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            },
            frame_size,
            bitrate,
            ..EncoderConfig::default()
        })
        .unwrap();
        let input = music(channels, false);
        let mut packets = Vec::new();
        for frame in input.chunks_exact(frame_size * channels) {
            let packet = encoder.encode(frame).unwrap();
            assert!(packet.len() <= bitrate as usize * frame_size / 384_000);
            packets.push(packet);
        }
        packets.extend(encoder.finish().unwrap());
        let mut decoder = Decoder::new(
            EncoderConfig::default().sample_rate,
            if channels == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            },
        )
        .unwrap();
        let mut decoded = Vec::new();
        for packet in &packets {
            let mut pcm = vec![0; frame_size * channels];
            assert_eq!(
                decoder.decode(Some(packet), &mut pcm, false).unwrap(),
                frame_size
            );
            decoded.extend(pcm);
        }
        if reference {
            let expected = oracle(&packets, channels);
            assert_eq!(expected.len(), decoded.len());
            let difference = expected
                .iter()
                .zip(&decoded)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(difference <= 2, "short CELT native/C mismatch={difference}");
            decoded = expected;
        }
        let signal = &input[..input.len() / (frame_size * channels) * frame_size * channels];
        let error: f64 = signal
            .iter()
            .zip(&decoded[encoder.lookahead() * channels..])
            .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
            .sum();
        let energy: f64 = signal.iter().map(|&x| f64::from(x).powi(2)).sum();
        let snr = 10.0 * (energy / error).log10();
        assert!(
            snr > 3.0,
            "channels={channels} frame_size={frame_size} bitrate={bitrate} snr={snr}"
        );
    }
}

#[test]
fn practical_celt_budgets_are_deterministic_and_decode() {
    for (channels, bitrate) in [(1, 32000), (1, 48000), (2, 32000), (2, 48000), (2, 64000)] {
        for transient in [false, true] {
            let (_, packets, _) = encode(channels, bitrate, transient);
            let (_, repeated, _) = encode(channels, bitrate, transient);
            assert_eq!(packets, repeated);
            let pcm = native(&packets, channels);
            assert!(pcm.iter().any(|sample| sample.unsigned_abs() > 100));
        }
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn joint_stereo_channel_geometry_matches_reference_with_fixed_packet_budget() {
    for geometry in 0..3 {
        for transient in [false, true] {
            let mut encoder = Encoder::new(EncoderConfig {
                channels: Channels::Stereo,
                bitrate: 32000,
                ..EncoderConfig::default()
            })
            .unwrap();
            let mut input = music(2, transient);
            for frame in input.as_chunks_mut::<2>().0 {
                if geometry == 0 {
                    frame[1] = frame[0];
                } else if geometry == 1 {
                    frame[1] = -frame[0];
                }
            }
            let mut packets = Vec::new();
            for frame in input.as_chunks::<1920>().0 {
                let packet = encoder.encode(frame).unwrap();
                assert!(packet.len() <= 80);
                packets.push(packet);
            }
            packets.extend(encoder.finish().unwrap());
            let expected = oracle(&packets, 2);
            let actual = native(&packets, 2);
            assert_eq!(actual.len(), expected.len());
            let first = actual
                .iter()
                .zip(&expected)
                .position(|(&a, &b)| (i32::from(a) - i32::from(b)).abs() > 2);
            assert!(
                first.is_none(),
                "geometry={geometry} transient={transient} first mismatch={first:?}"
            );
            let signal_energy: f64 = input.iter().map(|&x| f64::from(x).powi(2)).sum();
            let error: f64 = input
                .iter()
                .zip(&expected[encoder.lookahead() * 2..])
                .map(|(&a, &b)| (f64::from(a) - f64::from(b) / 32768.0).powi(2))
                .sum();
            let snr = 10.0 * (signal_energy / error).log10();
            eprintln!("joint geometry={geometry} transient={transient} snr={snr:.3}dB");
            assert!(
                snr > 3.0,
                "joint stereo quality geometry={geometry} transient={transient} snr={snr}"
            );
        }
    }
}

fn oracle(packets: &[Vec<u8>], channels: usize) -> Vec<i16> {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE")
        .expect("explicit portable oracle required; see libopus/README.md");
    let mut stream = Vec::new();
    for packet in packets {
        stream.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
        stream.extend_from_slice(packet);
    }
    let mut child = Command::new("python3")
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/output_oracle.py"
        ))
        .args(["--decode", "48000", &channels.to_string()])
        .env("MOOSICBOX_OPUS_REFERENCE_ORACLE", executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&stream).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.len().is_multiple_of(2));
    output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| i16::from_le_bytes(*sample))
        .collect()
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn practical_celt_music_and_attacks_match_portable_reference() {
    for (channels, bitrate) in [(1, 32000), (1, 48000), (2, 32000), (2, 48000), (2, 64000)] {
        for transient in [false, true] {
            let (input, packets, delay) = encode(channels, bitrate, transient);
            let reference = oracle(&packets, channels);
            let decoded = native(&packets, channels);
            assert_eq!(decoded.len(), reference.len());
            let maximum = decoded
                .iter()
                .zip(&reference)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(
                maximum <= 2,
                "channels={channels} bitrate={bitrate} transient={transient} native/C={maximum}"
            );
            let energy: f64 = input.iter().map(|&x| f64::from(x).powi(2)).sum();
            let error: f64 = input
                .iter()
                .zip(&reference[delay * channels..])
                .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
                .sum();
            let snr = 10.0 * (energy / error).log10();
            eprintln!(
                "channels={channels} bitrate={bitrate} transient={transient} snr={snr:.2}dB native/C={maximum}"
            );
            assert!(
                snr > 3.0,
                "music reconstruction must improve substantially on silence: {snr}dB"
            );
        }
    }
}
