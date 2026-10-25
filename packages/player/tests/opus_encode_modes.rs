#![cfg(feature = "encoder-opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Selected native packet modes through the real signal chain and gapless player.

use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, DecodeOptions, OpusDecodeBackend,
};
use moosicbox_audio_output::encoder::{
    AudioEncoder,
    opus::{OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode, SelectedOpusEncoder},
};
use moosicbox_ogg_opus::{OggOpusWriter, OpusHead, OpusTags};
use std::{
    io::{Cursor, Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use symphonia::core::{
    audio::{
        Audio, AudioBuffer, AudioMut, AudioSpec,
        layouts::{CHANNEL_LAYOUT_MONO, CHANNEL_LAYOUT_STEREO},
    },
    formats::probe::Hint,
    io::MediaSourceStream,
};

#[derive(Default)]
struct Capture {
    planes: Vec<Vec<f32>>,
    flushes: usize,
}
struct Sink(Arc<Mutex<Capture>>);
impl AudioDecode for Sink {
    fn decoded(
        &mut self,
        audio: AudioBuffer<f32>,
        _: &symphonia::core::packet::Packet,
        _: &symphonia::core::formats::Track,
    ) -> Result<(), AudioDecodeError> {
        let mut capture = self.0.lock().unwrap();
        capture
            .planes
            .resize_with(audio.spec().channels().count(), Vec::new);
        for (channel, plane) in capture.planes.iter_mut().enumerate() {
            plane.extend_from_slice(audio.plane(channel).unwrap());
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}
fn options() -> DecodeOptions {
    DecodeOptions {
        opus_backend: OpusDecodeBackend::Native,
        ..Default::default()
    }
}
fn input(rate: u32, channels: u8, frames: usize) -> AudioBuffer<f32> {
    let spec = AudioSpec::new(
        rate,
        if channels == 1 {
            CHANNEL_LAYOUT_MONO
        } else {
            CHANNEL_LAYOUT_STEREO
        },
    );
    let mut audio = AudioBuffer::new(spec, frames);
    audio.render_silence(Some(frames));
    for (channel, plane) in audio.iter_planes_mut().enumerate() {
        for (index, sample) in plane.iter_mut().enumerate() {
            *sample = (std::f32::consts::TAU * (330.0 + channel as f32 * 220.0) * index as f32
                / rate as f32)
                .sin()
                * 0.2;
        }
    }
    audio
}
fn play(bytes: Vec<u8>, seek: Option<f64>) -> Vec<Vec<f32>> {
    let capture = Arc::new(Mutex::new(Capture::default()));
    let sink = capture.clone();
    let mut handler = AudioDecodeHandler::new()
        .with_output(Box::new(move |_, _| Ok(Box::new(Sink(sink.clone())))));
    moosicbox_player::symphonia::play_media_source_with_options(
        MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default()),
        &Hint::new(),
        &mut handler,
        options(),
        None,
        seek,
    )
    .unwrap();
    let captured = capture.lock().unwrap();
    assert_eq!(captured.flushes, 1);
    captured.planes.clone()
}
fn ffmpeg(bytes: &[u8]) -> Vec<f32> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i", "pipe:0", "-f", "f32le", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

fn with_gain(mut bytes: Vec<u8>, gain: i16) -> Vec<u8> {
    let segments = usize::from(bytes[26]);
    let body = 27 + segments;
    let end = body
        + bytes[27..body]
            .iter()
            .map(|&x| usize::from(x))
            .sum::<usize>();
    bytes[body + 16..body + 18].copy_from_slice(&gain.to_le_bytes());
    bytes[22..26].fill(0);
    let mut crc = 0_u32;
    for &byte in &bytes[..end] {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
        }
    }
    bytes[22..26].copy_from_slice(&crc.to_le_bytes());
    bytes
}

fn pinned_reference(bytes: &[u8], config: OpusEncodeConfig) -> Vec<f32> {
    let mut format = moosicbox_audio_decoder::create_format_probe()
        .probe(
            &Hint::new(),
            MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default()),
            Default::default(),
            Default::default(),
        )
        .unwrap();
    let mut framed = Vec::new();
    let mut trims = Vec::new();
    while let Some(packet) = format.next_packet().unwrap() {
        framed.extend(u32::try_from(packet.data.len()).unwrap().to_le_bytes());
        framed.extend(&packet.data);
        // Packet::dur counts only valid frames, excluding pre-skip and EOS
        // padding. Oracle decode returns the complete coded packet duration.
        let duration =
            usize::try_from(moosicbox_ogg_opus::packet_duration(&packet.data).unwrap()).unwrap();
        let start = usize::try_from(packet.trim_start.get()).unwrap();
        let end = usize::try_from(packet.trim_end.get()).unwrap();
        assert_eq!(
            u64::try_from(duration - start - end).unwrap(),
            packet.dur.get(),
            "valid/coded duration mismatch: {config:?}"
        );
        trims.push((duration, start, end));
    }
    let channels = format.tracks()[0]
        .codec_params
        .as_ref()
        .unwrap()
        .audio()
        .unwrap()
        .channels
        .as_ref()
        .unwrap()
        .count();
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect(
        "live reference test requires MOOSICBOX_OPUS_REFERENCE_ORACLE=/absolute/path/to/qualified/reference_oracle; build the Cargo example as documented in opus_native/libopus/README.md",
    );
    let mut child = Command::new("python3")
        .args([
            "-B",
            "-c",
            r#"
import importlib.util
from pathlib import Path
import struct
import sys
client = Path(sys.argv[1]).resolve(strict=True)
spec = importlib.util.spec_from_file_location('moosicbox_reference_client', client)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
channels = int(sys.argv[3])
with module.Oracle(sys.argv[2]) as oracle:
    assert oracle.request('decoder', rate=48000, channels=channels) == {'ok': True}
    source = sys.stdin.buffer
    while True:
        header = source.read(4)
        if not header:
            break
        assert len(header) == 4, 'truncated packet length'
        length, = struct.unpack('<I', header)
        packet = source.read(length)
        assert len(packet) == length, 'truncated packet'
        response = oracle.request('decode', frame_size=5760, packet=list(packet))
        assert len(response['pcm']) == response['samples'] * channels
        sys.stdout.buffer.write(struct.pack('<' + 'h' * len(response['pcm']), *response['pcm']))
"#,
        ])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../opus_native/libopus/src/oracle.py"
        ))
        .arg(executable)
        .arg(channels.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&framed).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw: Vec<_> = output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
        .collect();
    let mut pcm = Vec::new();
    let mut offset = 0;
    for (duration, start, end) in trims {
        pcm.extend_from_slice(
            &raw[offset + start * channels..offset + (duration - end) * channels],
        );
        offset += duration * channels;
    }
    assert_eq!(
        offset,
        raw.len(),
        "oracle complete-packet length: {config:?}"
    );
    pcm
}

// These are acceptance tests for the intended public envelope, including cells
// still blocked by native codec guards. Unsupported is deliberately not success.

// Do not replace the process-global panic hook: tests may run concurrently.
// Libtest captures hook output; the final assertion lists every failed cell.
fn public_case_failure(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload")
}

fn intended_public_low_rate_playback(mode: OpusEncodeMode, bitrates: &[u32]) {
    let mut failures = Vec::new();
    let mut attempted = 0;
    for &bitrate in bitrates {
        for channels in [1, 2] {
            for frame_size in [480, 960] {
                for (input_rate, input_frames) in [(48_000, 2457), (44_100, 2258)] {
                    let config = OpusEncodeConfig {
                        backend: OpusEncodeBackend::Native,
                        mode,
                        sample_rate: 48_000,
                        channels,
                        frame_size,
                        bitrate,
                    };
                    attempted += 1;
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut encoder =
                            SelectedOpusEncoder::new(config).unwrap_or_else(|error| {
                                panic!("public constructor {config:?}: {error}")
                            });
                        let mut bytes = encoder
                            .encode(input(input_rate, channels, input_frames))
                            .unwrap_or_else(|error| {
                                panic!("public encode {config:?}, input_rate={input_rate}: {error}")
                            })
                            .to_vec();
                        bytes.extend_from_slice(
                            &encoder.finish().unwrap_or_else(|error| {
                                panic!("public finish {config:?}: {error}")
                            }),
                        );
                        assert!(encoder.finish().unwrap().is_empty(), "{config:?}");
                        let full = play(bytes.clone(), None);
                        assert_eq!(full.len(), usize::from(channels), "{config:?}");
                        for plane in &full {
                            assert_eq!(plane.len(), 2457, "{config:?}, input_rate={input_rate}");
                            assert!(plane.iter().all(|sample| sample.is_finite()), "{config:?}");
                            assert!(
                                plane.iter().any(|sample| sample.abs() > 0.001),
                                "{config:?}"
                            );
                        }
                        let sought = play(bytes.clone(), Some(0.05));
                        assert_eq!(sought.len(), full.len());
                        for (actual, expected) in sought.iter().zip(&full) {
                            assert_eq!(
                                actual,
                                &expected[2400..],
                                "public low-rate seek: {config:?}"
                            );
                        }
                        let gained = play(with_gain(bytes, -1536), None);
                        let factor = 10.0_f32.powf(-1536.0 / (256.0 * 20.0));
                        assert_eq!(gained.len(), full.len());
                        for (actual, expected) in gained.iter().zip(&full) {
                            assert_eq!(actual.len(), 2457);
                            for (actual, expected) in actual.iter().zip(expected) {
                                assert_eq!(
                                    *actual,
                                    *expected * factor,
                                    "gain exactly once: {config:?}"
                                );
                            }
                        }
                    }));
                    if let Err(payload) = outcome {
                        failures.push(format!(
                            "{config:?}, input_rate={input_rate}, input_frames={input_frames}: {}",
                            public_case_failure(payload.as_ref())
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {attempted} public player cells failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn intended_public_silk_low_rates_reach_player_and_seek() {
    intended_public_low_rate_playback(OpusEncodeMode::Silk, &[16_000, 24_000, 32_000, 64_000]);
}

#[test]
fn intended_public_hybrid_low_rates_reach_player_and_seek() {
    intended_public_low_rate_playback(OpusEncodeMode::Hybrid, &[32_000, 64_000]);
}

#[test]
fn intended_public_auto_low_budget_mode_controls_reach_player() {
    let mut failures = Vec::new();
    let mut attempted = 0;
    use moosicbox_audio_output::encoder::opus::EncoderMode;

    for bitrate in [32_000, 64_000] {
        for channels in [1, 2] {
            for frame_size in [480, 960] {
                let config = OpusEncodeConfig {
                    backend: OpusEncodeBackend::Native,
                    mode: OpusEncodeMode::Auto,
                    sample_rate: 48_000,
                    channels,
                    frame_size,
                    bitrate,
                };
                attempted += 1;
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut encoder = SelectedOpusEncoder::new(config)
                        .unwrap_or_else(|error| panic!("public Auto {config:?}: {error}"));
                    let mut control = encoder.control().expect("native requested controls");
                    let mut bytes = encoder
                        .encode(input(48_000, channels, frame_size))
                        .unwrap()
                        .to_vec();
                    control.mode = EncoderMode::Hybrid;
                    encoder.set_control(control).unwrap_or_else(|error| {
                        panic!("intended low-budget Hybrid entry {config:?}: {error}")
                    });
                    bytes.extend_from_slice(
                        &encoder
                            .encode(input(48_000, channels, frame_size))
                            .unwrap_or_else(|error| {
                                panic!("public Hybrid boundary {config:?}: {error}")
                            }),
                    );
                    assert_eq!(
                        encoder.active_mode(),
                        Some(EncoderMode::Hybrid),
                        "{config:?}"
                    );
                    control.mode = EncoderMode::Celt;
                    encoder.set_control(control).unwrap_or_else(|error| {
                        panic!("intended low-budget CELT departure {config:?}: {error}")
                    });
                    bytes.extend_from_slice(
                        &encoder
                            .encode(input(48_000, channels, 2457 - 2 * frame_size))
                            .unwrap_or_else(|error| panic!("public departure {config:?}: {error}")),
                    );
                    bytes.extend_from_slice(&encoder.finish().unwrap());
                    assert!(encoder.finish().unwrap().is_empty());
                    let full = play(bytes.clone(), None);
                    assert_eq!(full.len(), usize::from(channels));
                    for plane in &full {
                        assert_eq!(plane.len(), 2457, "{config:?}");
                        assert!(plane.iter().any(|sample| sample.abs() > 0.001));
                    }
                    let sought = play(bytes, Some(0.05));
                    assert_eq!(sought.len(), full.len());
                    for (actual, expected) in sought.iter().zip(full) {
                        assert_eq!(actual, &expected[2400..], "controlled seek: {config:?}");
                    }
                }));
                if let Err(payload) = outcome {
                    failures.push(format!(
                        "{config:?}: {}",
                        public_case_failure(payload.as_ref())
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {attempted} public player cells failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn intended_public_auto_low_rates_reach_player_and_seek() {
    intended_public_low_rate_playback(OpusEncodeMode::Auto, &[32_000, 64_000]);
}

#[test]
fn selected_modes_reenter_native_player_with_exact_tail_and_seek() {
    selected_modes(false);
}

#[test]
#[ignore = "requires explicit qualified reference oracle"]
fn selected_modes_match_qualified_public_c_pcm() {
    // Validate opt-in even before doing any native/FFmpeg work.
    assert!(
        std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").is_some(),
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to the qualified Cargo reference_oracle example executable; see opus_native/libopus/README.md"
    );
    selected_modes(true);
}

fn selected_modes(live_reference: bool) {
    for (mode, rate) in [
        (OpusEncodeMode::Celt, 48000),
        (OpusEncodeMode::Silk, 8000),
        (OpusEncodeMode::Silk, 12000),
        (OpusEncodeMode::Silk, 16000),
        (OpusEncodeMode::Hybrid, 48000),
    ] {
        for channels in [1, 2] {
            for divisor in [100, 50] {
                let frame_size = rate as usize / divisor;
                let frames = frame_size * 10 + frame_size / 3;
                let config = OpusEncodeConfig {
                    backend: OpusEncodeBackend::Native,
                    mode,
                    sample_rate: rate,
                    channels,
                    frame_size,
                    bitrate: 128000,
                };
                let mut encoder = SelectedOpusEncoder::new(config).unwrap();
                assert_eq!(encoder.mode(), mode);
                let mut bytes = encoder
                    .encode(input(rate, channels, frames))
                    .unwrap_or_else(|error| panic!("{config:?}: {error}"))
                    .to_vec();
                bytes.extend_from_slice(&encoder.finish().unwrap());
                assert!(encoder.finish().unwrap().is_empty());
                let expected = frames * (48000 / rate) as usize;
                let mut probe = moosicbox_audio_decoder::create_format_probe()
                    .probe(
                        &Hint::new(),
                        MediaSourceStream::new(
                            Box::new(Cursor::new(bytes.clone())),
                            Default::default(),
                        ),
                        Default::default(),
                        Default::default(),
                    )
                    .unwrap();
                let packet = probe.next_packet().unwrap().unwrap();
                let toc = packet.data[0] >> 3;
                match mode {
                    OpusEncodeMode::Celt => assert!(toc >= 16, "CELT TOC {toc}"),
                    OpusEncodeMode::Silk => assert!(toc < 12, "SILK TOC {toc}"),
                    OpusEncodeMode::Hybrid => assert!((12..16).contains(&toc), "hybrid TOC {toc}"),
                    OpusEncodeMode::Default | OpusEncodeMode::Auto => unreachable!(),
                }
                let external = ffmpeg(&bytes);
                assert_eq!(
                    external.len(),
                    expected * usize::from(channels),
                    "{config:?}"
                );
                let reference = live_reference.then(|| pinned_reference(&bytes, config));
                let decoded = play(bytes.clone(), None);
                assert_eq!(decoded.len(), usize::from(channels));
                for (channel, plane) in decoded.iter().enumerate() {
                    assert_eq!(plane.len(), expected, "{config:?}");
                    assert!(plane.iter().any(|x| x.abs() > 0.001), "{config:?}");
                    if let Some(reference) = &reference {
                        let max_difference = plane
                            .iter()
                            .zip(
                                reference
                                    .iter()
                                    .skip(channel)
                                    .step_by(usize::from(channels)),
                            )
                            .map(|(native, external)| (native - external).abs())
                            .fold(0.0_f32, f32::max);
                        assert_eq!(
                            max_difference, 0.0,
                            "pinned public C PCM mismatch: {config:?}"
                        );
                    }
                }
                for gain in [-1536_i16, 1536] {
                    let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
                    let gained = play(with_gain(bytes.clone(), gain), None);
                    for (actual, full) in gained.iter().zip(&decoded) {
                        assert_eq!(actual.len(), full.len());
                        for (sample, reference) in actual.iter().zip(full) {
                            assert_eq!(
                                *sample,
                                *reference * factor,
                                "gain applied exactly once: {config:?}"
                            );
                        }
                    }
                }
                // A 50-ms seek retains stream-origin preroll and must be exact.
                let sought = play(bytes.clone(), Some(0.05));
                for (actual, full) in sought.iter().zip(&decoded) {
                    assert_eq!(actual, &full[2400..], "{config:?}");
                }
                let late = play(bytes.clone(), Some(0.09));
                for plane in &late {
                    assert_eq!(
                        plane.len(),
                        expected - 4320,
                        "late seek sample cutoff: {config:?}"
                    );
                    assert!(plane.iter().all(|sample| sample.is_finite()));
                }
                // The public signal-chain API forwards this very mode selection.
                let mut processor = moosicbox_player::signal_chain::SignalChainStep::new()
                    .with_decode_options(options())
                    .with_opus_encoder(config)
                    .unwrap()
                    .process(Box::new(Cursor::new(bytes)))
                    .unwrap();
                let mut reencoded = Vec::new();
                processor.read_to_end(&mut reencoded).unwrap_or_else(|error| {
                    let maximum = decoded.iter().flatten().fold(0.0_f32, |maximum, sample| maximum.max(sample.abs()));
                    panic!("signal-chain {config:?}, decoded_frames={}, decoded_peak={maximum}: {error}", decoded[0].len());
                });
                let replay = play(reencoded, None);
                assert_eq!(replay[0].len(), expected, "signal-chain {config:?}");
            }
        }
    }
}

#[test]
#[ignore = "requires explicit qualified reference oracle"]
fn changing_mode_ogg_file_matches_public_c_and_player_seek() {
    assert!(
        std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").is_some(),
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to the qualified Cargo reference_oracle executable"
    );
    for channels in [1_u8, 2] {
        let output = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/changing_mode_oracle.py"
            ))
            .arg(channels.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut records = output.stdout.as_slice();
        let mut packets = Vec::new();
        let mut reference = vec![Vec::new(); usize::from(channels)];
        let mut modes = String::new();
        while !records.is_empty() {
            assert!(records.len() >= 8);
            let size = u32::from_le_bytes(records[..4].try_into().unwrap()) as usize;
            let samples = u32::from_le_bytes(records[4..8].try_into().unwrap()) as usize;
            assert_eq!(samples, 960 * usize::from(channels));
            records = &records[8..];
            assert!(records.len() >= size + samples * 2);
            let packet = records[..size].to_vec();
            let toc = packet[0] >> 3;
            modes.push(if toc < 12 {
                'S'
            } else if toc < 16 {
                'H'
            } else {
                'C'
            });
            records = &records[size..];
            for (index, sample) in records[..samples * 2].as_chunks::<2>().0.iter().enumerate() {
                reference[index % usize::from(channels)]
                    .push(f32::from(i16::from_le_bytes(*sample)) / 32768.0);
            }
            records = &records[samples * 2..];
            packets.push(packet);
        }
        assert_eq!(packets.len(), 42);
        assert_eq!(
            modes,
            [
                "SSSSSS", "HHHHHH", "HCCCCC", "HHHHHH", "SSSSSS", "HCCCCC", "SSSSSS"
            ]
            .concat()
        );
        // Exercise a valid full-delay timeline and an explicit nonzero trim.
        // Neither value claims to be the C encoder's queried lookahead.
        for pre_skip in [0_u16, 137] {
            let audible = 42 * 960 - 117 - usize::from(pre_skip);
            let reference: Vec<Vec<f32>> = reference
                .iter()
                .map(|plane| plane[usize::from(pre_skip)..42 * 960 - 117].to_vec())
                .collect();
            let head = OpusHead::new(channels, pre_skip, 48000).unwrap();
            let mut writer =
                OggOpusWriter::new(Vec::new(), head, &OpusTags::default(), 71).unwrap();
            for packet in &packets {
                writer.write_packet(packet).unwrap();
            }
            let bytes = writer.finish(audible as u64).unwrap();
            let full = play(bytes.clone(), None);
            assert_eq!(
                full, reference,
                "changing-mode persistent PCM channels={channels}"
            );
            let external = ffmpeg(&bytes);
            assert_eq!(external.len(), audible * usize::from(channels));
            assert!(external.iter().any(|sample| sample.abs() > 0.001));
            for gain in [-1536_i16, 1536] {
                let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
                let gained = play(with_gain(bytes.clone(), gain), None);
                for (actual, expected) in gained.iter().zip(&reference) {
                    assert_eq!(actual.len(), audible);
                    for (sample, reference) in actual.iter().zip(expected) {
                        assert_eq!(*sample, *reference * factor);
                    }
                }
            }
            // Origin-preroll seek must reproduce linear PCM exactly.
            let early = play(bytes.clone(), Some(0.05));
            for (actual, expected) in early.iter().zip(&reference) {
                assert_eq!(actual, &expected[2400..]);
            }
            // Later seeks cross codec boundaries with finite preroll. Assert exact
            // presentation cutoff/count, not infinite-history reconstruction identity.
            for (seconds, cutoff) in [(0.31, 14880), (0.65, 31200)] {
                let sought = play(bytes.clone(), Some(seconds));
                for plane in sought {
                    assert_eq!(plane.len(), audible - cutoff);
                    assert!(plane.iter().all(|sample| sample.is_finite()));
                    assert!(plane.iter().any(|sample| sample.abs() > 0.001));
                }
            }
            // Truncated pages must propagate through normal playback as errors,
            // rather than trigger a backend fallback or silently accept missing EOS.
            let mut truncated = bytes;
            truncated.truncate(truncated.len() - 1);
            let result = moosicbox_player::symphonia::play_media_source_with_options(
                MediaSourceStream::new(Box::new(Cursor::new(truncated)), Default::default()),
                &Hint::new(),
                &mut AudioDecodeHandler::new(),
                options(),
                None,
                None,
            );
            assert!(result.is_err());
        }
    }
}

#[test]
fn public_auto_controlled_ogg_reenters_player_with_exact_gain_and_seek() {
    use moosicbox_audio_output::encoder::opus::EncoderMode;

    let config = OpusEncodeConfig {
        backend: OpusEncodeBackend::Native,
        mode: OpusEncodeMode::Auto,
        sample_rate: 48_000,
        channels: 2,
        frame_size: 960,
        bitrate: 192_000,
    };
    let mut encoder = SelectedOpusEncoder::new(config).unwrap();
    let mut control = encoder.control().unwrap();
    let mut bytes = encoder.encode(input(48_000, 2, 960)).unwrap().to_vec();
    assert_eq!(encoder.active_mode(), Some(EncoderMode::Hybrid));
    for (bitrate, frames) in [(64_000, 960), (32_000, 537)] {
        control.bitrate = bitrate;
        encoder.set_control(control).unwrap();
        bytes.extend_from_slice(&encoder.encode(input(48_000, 2, frames)).unwrap());
        assert_eq!(encoder.active_mode(), Some(EncoderMode::Hybrid));
    }
    let before = encoder.control();
    assert!(encoder.set_control(control).is_err());
    assert_eq!(encoder.control(), before);
    bytes.extend_from_slice(&encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    // Inspect only packets emitted by the public encoder: no synthetic packet
    // injection or test-only native constructor participates in this regression.
    let mut probe = moosicbox_audio_decoder::create_format_probe()
        .probe(
            &Hint::new(),
            MediaSourceStream::new(Box::new(Cursor::new(bytes.clone())), Default::default()),
            Default::default(),
            Default::default(),
        )
        .unwrap();
    let mut audible = 0;
    let mut packets = 0;
    while let Some(packet) = probe.next_packet().unwrap() {
        assert_eq!(
            packet.data[0] >> 3,
            15,
            "public Auto must retain fullband Hybrid"
        );
        audible += packet.dur.get();
        packets += 1;
    }
    assert!(packets >= 3);
    assert_eq!(audible, 2457);
    let decoded = play(bytes.clone(), None);
    assert_eq!(decoded.len(), 2);
    for plane in &decoded {
        assert_eq!(plane.len(), 2457);
        assert!(plane.iter().all(|sample| sample.is_finite()));
        assert!(plane.iter().any(|sample| sample.abs() > 0.001));
    }
    let sought = play(bytes.clone(), Some(0.05));
    assert_eq!(sought.len(), 2);
    for (actual, expected) in sought.iter().zip(&decoded) {
        assert_eq!(actual, &expected[2400..]);
    }
    for gain in [-1536, 1536] {
        let gained = play(with_gain(bytes.clone(), gain), None);
        let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
        for (actual, expected) in gained.iter().zip(&decoded) {
            assert_eq!(actual.len(), 2457);
            for (actual, expected) in actual.iter().zip(expected) {
                assert_eq!(*actual, *expected * factor);
            }
        }
    }
    // The existing convenience API supports steady Auto without a new scheduler.
    let mut processor = moosicbox_player::signal_chain::SignalChainStep::new()
        .with_decode_options(options())
        .with_opus_encoder(config)
        .unwrap()
        .process(Box::new(Cursor::new(bytes)))
        .unwrap();
    let mut encoded = Vec::new();
    processor.read_to_end(&mut encoded).unwrap();
    for plane in play(encoded, None) {
        assert_eq!(plane.len(), 2457);
        assert!(plane.iter().any(|sample| sample.abs() > 0.001));
    }
}

#[test]
fn stereo_fullband_hybrid_low_bitrates_reenter_player_and_signal_chain() {
    for bitrate in [32_000, 64_000] {
        for frame_size in [480, 960] {
            let frames = 2457;
            let config = OpusEncodeConfig {
                backend: OpusEncodeBackend::Native,
                mode: OpusEncodeMode::Hybrid,
                sample_rate: 48_000,
                channels: 2,
                frame_size,
                bitrate,
            };
            let mut encoder = SelectedOpusEncoder::new(config)
                .unwrap_or_else(|error| panic!("{config:?}: {error}"));
            assert_eq!(encoder.mode(), OpusEncodeMode::Hybrid);
            let mut bytes = encoder.encode(input(48_000, 2, frames)).unwrap().to_vec();
            bytes.extend_from_slice(&encoder.finish().unwrap());
            assert!(encoder.finish().unwrap().is_empty(), "{config:?}");
            let mut probe = moosicbox_audio_decoder::create_format_probe()
                .probe(
                    &Hint::new(),
                    MediaSourceStream::new(
                        Box::new(Cursor::new(bytes.clone())),
                        Default::default(),
                    ),
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
            while let Some(packet) = probe.next_packet().unwrap() {
                assert_eq!(
                    packet.data[0] >> 3,
                    if frame_size == 480 { 14 } else { 15 },
                    "fullband Hybrid, not a fallback: {config:?}"
                );
            }
            let decoded = play(bytes.clone(), None);
            assert_eq!(decoded.len(), 2, "{config:?}");
            for plane in &decoded {
                assert_eq!(plane.len(), frames, "{config:?}");
                assert!(plane.iter().all(|sample| sample.is_finite()), "{config:?}");
                assert!(
                    plane.iter().any(|sample| sample.abs() > 0.001),
                    "{config:?}"
                );
            }
            for gain in [-1536, 1536] {
                let gained = play(with_gain(bytes.clone(), gain), None);
                let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
                for (actual, expected) in gained.iter().zip(&decoded) {
                    assert_eq!(actual.len(), frames, "{config:?}");
                    for (actual, expected) in actual.iter().zip(expected) {
                        assert_eq!(*actual, *expected * factor, "gain exactly once: {config:?}");
                    }
                }
            }
            let sought = play(bytes.clone(), Some(0.05));
            assert_eq!(sought.len(), 2, "{config:?}");
            for (actual, expected) in sought.iter().zip(&decoded) {
                assert_eq!(actual, &expected[2400..], "seek exact tail: {config:?}");
            }
            let mut processor = moosicbox_player::signal_chain::SignalChainStep::new()
                .with_decode_options(options())
                .with_opus_encoder(config)
                .unwrap()
                .process(Box::new(Cursor::new(bytes)))
                .unwrap();
            let mut reencoded = Vec::new();
            processor
                .read_to_end(&mut reencoded)
                .unwrap_or_else(|error| panic!("signal-chain {config:?}: {error}"));
            let replay = play(reencoded, None);
            assert_eq!(replay.len(), 2, "{config:?}");
            for plane in replay {
                assert_eq!(plane.len(), frames, "signal-chain clock: {config:?}");
                assert!(plane.iter().all(|sample| sample.is_finite()), "{config:?}");
                assert!(
                    plane.iter().any(|sample| sample.abs() > 0.001),
                    "{config:?}"
                );
            }
        }
    }
}

#[test]
fn stereo_silk_low_bitrate_product_regression() {
    for (rate, bitrate) in [(8000_u32, 32000), (16000, 64000)] {
        let config = OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode: OpusEncodeMode::Silk,
            sample_rate: rate,
            channels: 2,
            bitrate,
            frame_size: rate as usize / 100,
        };
        assert!(
            SelectedOpusEncoder::new(config).is_err(),
            "unsupported bitrate must fail during construction: {config:?}"
        );
    }
}

#[test]
fn resampled_final_fft_chunk_preserves_exact_length_and_chunk_independence() {
    for (input_rate, output_rate, mode) in [
        (48000_u32, 16000_u32, OpusEncodeMode::Silk),
        (44100, 48000, OpusEncodeMode::Celt),
        (48000, 24000, OpusEncodeMode::Celt),
    ] {
        for frames in [1023_usize, 1024, 1025, 2457, 4097] {
            let config = OpusEncodeConfig {
                backend: OpusEncodeBackend::Native,
                mode,
                sample_rate: output_rate,
                channels: 2,
                frame_size: usize::try_from(output_rate / 50).unwrap(),
                ..Default::default()
            };
            let original = input(input_rate, 2, frames);
            let mut results = Vec::new();
            for chunk_size in [frames, 13] {
                let mut encoder = SelectedOpusEncoder::new(config).unwrap();
                let mut bytes = Vec::new();
                for start in (0..frames).step_by(chunk_size) {
                    let count = chunk_size.min(frames - start);
                    let mut chunk = AudioBuffer::<f32>::new(original.spec().clone(), count);
                    chunk.render_silence(Some(count));
                    for channel in 0..2 {
                        chunk.plane_mut(channel).unwrap().copy_from_slice(
                            &original.plane(channel).unwrap()[start..start + count],
                        );
                    }
                    bytes.extend_from_slice(&encoder.encode(chunk).unwrap());
                }
                bytes.extend_from_slice(&encoder.finish().unwrap());
                assert!(encoder.finish().unwrap().is_empty());
                let eos = bytes
                    .windows(4)
                    .rposition(|window| window == b"OggS")
                    .unwrap();
                let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
                let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
                let expected = u64::try_from(frames).unwrap() * u64::from(output_rate)
                    / u64::from(input_rate)
                    * u64::from(48000 / output_rate);
                assert_eq!(
                    granule - u64::from(pre_skip),
                    expected,
                    "{input_rate}->{output_rate} frames={frames} chunk={chunk_size}"
                );
                let decoded = play(bytes.clone(), None);
                assert_eq!(decoded.len(), 2);
                for plane in decoded {
                    assert_eq!(u64::try_from(plane.len()).unwrap(), expected);
                    assert!(plane.iter().any(|sample| sample.abs() > 0.001));
                }
                results.push(bytes);
            }
            assert_eq!(
                results[0], results[1],
                "PCM chunking changed encoded output: {input_rate}->{output_rate} frames={frames}"
            );
        }
    }
}

#[test]
fn product_pcm_saturation_matches_explicit_clamping_and_rejects_nonfinite() {
    for mode in [
        OpusEncodeMode::Celt,
        OpusEncodeMode::Silk,
        OpusEncodeMode::Hybrid,
    ] {
        let config = OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode,
            sample_rate: 48000,
            channels: 2,
            frame_size: 960,
            ..Default::default()
        };
        let mut original = input(48000, 2, 2457);
        for plane in original.iter_planes_mut() {
            for (index, sample) in plane.iter_mut().enumerate() {
                *sample = match index % 5 {
                    0 => 1.0038608,
                    1 => -1.0038608,
                    2 => 1.0,
                    3 => -1.0,
                    _ => *sample,
                };
            }
        }
        let mut clamped = original.clone();
        for plane in clamped.iter_planes_mut() {
            for sample in plane {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
        let mut expected_encoder = SelectedOpusEncoder::new(config).unwrap();
        let mut expected = expected_encoder.encode(clamped).unwrap().to_vec();
        expected.extend_from_slice(&expected_encoder.finish().unwrap());
        let mut encoder = SelectedOpusEncoder::new(config).unwrap();
        // Rejected buffers must not establish input rate, consume samples, or
        // change codec state. Different invalid rates expose accidental pinning.
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut invalid = input(44100, 2, 13);
            invalid.plane_mut(0).unwrap()[0] = value;
            assert!(
                encoder.encode(invalid).is_err(),
                "{mode:?} accepted {value}"
            );
        }
        let mut actual = encoder.encode(original).unwrap().to_vec();
        actual.extend_from_slice(&encoder.finish().unwrap());
        assert!(encoder.finish().unwrap().is_empty());
        assert_eq!(actual, expected, "product saturation policy {mode:?}");
    }
}

#[test]
fn unsupported_mode_combinations_are_errors_not_fallbacks() {
    for config in [
        OpusEncodeConfig {
            backend: OpusEncodeBackend::Libopus,
            mode: OpusEncodeMode::Silk,
            ..Default::default()
        },
        OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode: OpusEncodeMode::Silk,
            frame_size: 120,
            ..Default::default()
        },
        OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode: OpusEncodeMode::Hybrid,
            sample_rate: 16000,
            frame_size: 320,
            ..Default::default()
        },
        OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode: OpusEncodeMode::Hybrid,
            frame_size: 120,
            ..Default::default()
        },
    ] {
        assert!(SelectedOpusEncoder::new(config).is_err(), "{config:?}");
    }
}
