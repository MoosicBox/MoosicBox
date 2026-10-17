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

fn pinned_reference(bytes: &[u8]) -> Vec<f32> {
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
        trims.push((
            packet.dur.get() as usize,
            packet.trim_start.get() as usize,
            packet.trim_end.get() as usize,
        ));
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
    assert_eq!(offset, raw.len());
    pcm
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
                    OpusEncodeMode::Default => unreachable!(),
                }
                let external = ffmpeg(&bytes);
                assert_eq!(
                    external.len(),
                    expected * usize::from(channels),
                    "{config:?}"
                );
                let reference = live_reference.then(|| pinned_reference(&bytes));
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
                processor.read_to_end(&mut reencoded).unwrap();
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
