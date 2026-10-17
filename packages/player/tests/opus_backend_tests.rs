#![cfg(feature = "decoder-opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError, AudioDecodeHandler};
#[cfg(feature = "encoder-opus-native")]
use moosicbox_audio_output::encoder::opus::{OpusEncodeBackend, OpusEncodeConfig};
use moosicbox_player::{DecodeOptions, OpusDecodeBackend};
use std::{
    io::{Cursor, Read as _},
    sync::{Arc, Mutex},
};
use symphonia::core::{
    audio::{Audio, AudioBuffer},
    formats::{Track, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    packet::Packet,
};

fn page(sequence: u32, flags: u8, granule: u64, packet: &[u8]) -> Vec<u8> {
    let mut page = b"OggS\0".to_vec();
    page.push(flags);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&42_u32.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    let mut remaining = packet.len();
    let mut lacing = Vec::new();
    while remaining >= 255 {
        lacing.push(255);
        remaining -= 255;
    }
    lacing.push(u8::try_from(remaining).unwrap());
    page.push(u8::try_from(lacing.len()).unwrap());
    page.extend_from_slice(&lacing);
    page.extend_from_slice(packet);
    let mut crc = 0_u32;
    for byte in &page {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

fn stream(gain: i16, pre_skip: u16) -> Vec<u8> {
    let mut head = b"OpusHead\x01\x01".to_vec();
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&48000_u32.to_le_bytes());
    head.extend_from_slice(&gain.to_le_bytes());
    head.push(0);
    let mut bytes = page(0, 2, 0, &head);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&[0; 8]);
    bytes.extend(page(1, 0, 0, &tags));
    let packets: [&[u8]; 4] = [
        include_bytes!("../../audio_decoder/tests/fixtures/wb_sequence_0.packet"),
        include_bytes!("../../audio_decoder/tests/fixtures/wb_sequence_1.packet"),
        include_bytes!("../../audio_decoder/tests/fixtures/wb_sequence_2.packet"),
        include_bytes!("../../audio_decoder/tests/fixtures/wb_sequence_3.packet"),
    ];
    for (i, p) in packets.iter().enumerate() {
        bytes.extend(page(
            u32::try_from(i + 2).unwrap(),
            if i == 3 { 4 } else { 0 },
            u64::try_from((i + 1) * 960).unwrap(),
            p,
        ));
    }
    bytes
}
#[derive(Default)]
struct Capture {
    pcm: Vec<f32>,
    frames: Vec<usize>,
    flushes: usize,
}
struct Sink(Arc<Mutex<Capture>>);
impl AudioDecode for Sink {
    fn decoded(
        &mut self,
        audio: AudioBuffer<f32>,
        _: &Packet,
        _: &Track,
    ) -> Result<(), AudioDecodeError> {
        assert_eq!(audio.spec().rate(), 48_000);
        assert_eq!(audio.spec().channels().count(), 1);
        let mut capture = self.0.lock().unwrap();
        capture.frames.push(audio.frames());
        capture.pcm.extend_from_slice(audio.plane(0).unwrap());
        drop(capture);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}
fn options(backend: OpusDecodeBackend) -> DecodeOptions {
    DecodeOptions {
        decoder: symphonia::core::codecs::audio::AudioDecoderOptions::default().gapless(false),
        opus_backend: backend,
    }
}
fn source(gain: i16) -> MediaSourceStream<'static> {
    MediaSourceStream::new(
        Box::new(Cursor::new(stream(gain, 0))),
        MediaSourceStreamOptions::default(),
    )
}
fn expected(gain: i16) -> Vec<f32> {
    let factor = 10_f32.powf(f32::from(gain) / 5120.0);
    include_bytes!("../../audio_decoder/tests/fixtures/wb_sequence.api48.pcm")
        .chunks_exact(2)
        .map(|s| f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0 * factor)
        .collect()
}
fn exercise(backend: OpusDecodeBackend) {
    for gain in [0, 1536] {
        let capture = Arc::new(Mutex::new(Capture::default()));
        let sink = capture.clone();
        let mut handler = AudioDecodeHandler::new().with_output(Box::new(move |spec, _| {
            assert_eq!(spec.rate(), 48_000);
            Ok(Box::new(Sink(sink.clone())))
        }));
        assert_eq!(
            moosicbox_player::symphonia::play_media_source_with_options(
                source(gain),
                &Hint::new(),
                &mut handler,
                options(backend),
                None,
                None
            )
            .unwrap(),
            0
        );
        {
            let capture = capture.lock().unwrap();
            assert_eq!(capture.frames, [960; 4]);
            assert_eq!(capture.flushes, 1);
            let reference = expected(gain);
            assert!(reference.iter().any(|s| s.abs() > 0.001));
            assert_eq!(capture.pcm, reference);
            drop(capture);
        }
        let rx = moosicbox_player::symphonia_unsync::play_media_source_with_options(
            source(gain),
            &Hint::new(),
            options(backend),
            None,
            None,
        )
        .unwrap();
        let mut pcm = Vec::new();
        for _ in 0..4 {
            let audio = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            assert_eq!(audio.spec().rate(), 48_000);
            assert_eq!(audio.spec().channels().count(), 1);
            assert_eq!(audio.frames(), 960);
            pcm.extend_from_slice(audio.plane(0).unwrap());
        }
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(flume::RecvTimeoutError::Disconnected)
        ));
        assert_eq!(pcm, expected(gain));
    }
}
#[test_log::test]
fn native_player_normal_gapless_preskip_tail_gain_and_seek() {
    for gain in [0, 1536] {
        for seek in [None, Some(0.025)] {
            let mut bytes = stream(gain, 312);
            // Last page contains the fourth packet; truncate its audible tail by 111.
            let last = bytes.windows(4).rposition(|w| w == b"OggS").unwrap();
            bytes[last + 6..last + 14].copy_from_slice(&3729_u64.to_le_bytes());
            bytes[last + 22..last + 26].fill(0);
            let mut crc = 0_u32;
            for byte in &bytes[last..] {
                crc ^= u32::from(*byte) << 24;
                for _ in 0..8 {
                    crc = if crc & 0x8000_0000 != 0 {
                        (crc << 1) ^ 0x04c1_1db7
                    } else {
                        crc << 1
                    };
                }
            }
            bytes[last + 22..last + 26].copy_from_slice(&crc.to_le_bytes());
            let capture = Arc::new(Mutex::new(Capture::default()));
            let sink = capture.clone();
            let mut handler = AudioDecodeHandler::new()
                .with_output(Box::new(move |_, _| Ok(Box::new(Sink(sink.clone())))));
            let options = DecodeOptions {
                opus_backend: OpusDecodeBackend::Native,
                ..DecodeOptions::default()
            };
            moosicbox_player::symphonia::play_media_source_with_options(
                MediaSourceStream::new(
                    Box::new(Cursor::new(bytes.clone())),
                    MediaSourceStreamOptions::default(),
                ),
                &Hint::new(),
                &mut handler,
                options,
                None,
                seek,
            )
            .unwrap();
            let reference = expected(gain);
            let start = 312 + if seek.is_some() { 1200 } else { 0 };
            let reference = &reference[start..3729];
            let captured = capture.lock().unwrap();
            assert_eq!(captured.pcm, reference);
            assert_eq!(captured.flushes, 1);
            drop(captured);
            let rx = moosicbox_player::symphonia_unsync::play_media_source_with_options_result(
                MediaSourceStream::new(
                    Box::new(Cursor::new(bytes)),
                    MediaSourceStreamOptions::default(),
                ),
                &Hint::new(),
                options,
                None,
                seek,
            )
            .unwrap();
            let mut actual = Vec::new();
            while let Ok(result) = rx.recv_timeout(std::time::Duration::from_secs(2)) {
                actual.extend_from_slice(result.unwrap().plane(0).unwrap());
            }
            assert_eq!(actual, reference);
        }
    }
}

#[test_log::test]
fn native_nonzero_granule_origin_normalizes_player_seek() {
    let mut bytes = stream(0, 312);
    let pages: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == b"OggS").then_some(i))
        .collect();
    for (index, start) in pages.iter().copied().enumerate().skip(2) {
        let end = pages.get(index + 1).copied().unwrap_or(bytes.len());
        let granule = u64::from_le_bytes(bytes[start + 6..start + 14].try_into().unwrap()) + 10_000;
        bytes[start + 6..start + 14].copy_from_slice(&granule.to_le_bytes());
        bytes[start + 22..start + 26].fill(0);
        let mut crc = 0_u32;
        for byte in &bytes[start..end] {
            crc ^= u32::from(*byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ 0x04c1_1db7
                } else {
                    crc << 1
                };
            }
        }
        bytes[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
    }
    let mut reader = moosicbox_audio_decoder::create_format_probe()
        .probe(
            &Hint::new(),
            MediaSourceStream::new(
                Box::new(Cursor::new(bytes.clone())),
                MediaSourceStreamOptions::default(),
            ),
            symphonia::core::formats::FormatOptions::default(),
            symphonia::core::meta::MetadataOptions::default(),
        )
        .unwrap();
    assert_eq!(reader.next_packet().unwrap().unwrap().pts.get(), -312);
    for seek in [None, Some(0.025)] {
        let rx = moosicbox_player::symphonia_unsync::play_media_source_with_options_result(
            MediaSourceStream::new(
                Box::new(Cursor::new(bytes.clone())),
                MediaSourceStreamOptions::default(),
            ),
            &Hint::new(),
            DecodeOptions {
                opus_backend: OpusDecodeBackend::Native,
                ..Default::default()
            },
            None,
            seek,
        )
        .unwrap();
        let mut actual = Vec::new();
        while let Ok(result) = rx.recv_timeout(std::time::Duration::from_secs(2)) {
            actual.extend_from_slice(result.unwrap().plane(0).unwrap());
        }
        assert_eq!(
            actual,
            expected(0)[312 + if seek.is_some() { 1200 } else { 0 }..]
        );
    }
}

#[cfg(feature = "encoder-opus-native")]
#[test_log::test]
fn real_native_transcode_reenters_gapless_player() {
    let options = DecodeOptions {
        opus_backend: OpusDecodeBackend::Native,
        ..Default::default()
    };
    let mut processor = moosicbox_player::signal_chain::SignalChainStep::new()
        .with_decode_options(options)
        .with_opus_encoder(OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            channels: 1,
            ..Default::default()
        })
        .unwrap()
        .process(Box::new(Cursor::new(stream(0, 312))))
        .unwrap();
    let mut ogg = Vec::new();
    processor.read_to_end(&mut ogg).unwrap();
    assert_eq!(processor.read(&mut [0; 1]).unwrap(), 0);
    let capture = Arc::new(Mutex::new(Capture::default()));
    let sink = capture.clone();
    let mut handler = AudioDecodeHandler::new()
        .with_output(Box::new(move |_, _| Ok(Box::new(Sink(sink.clone())))));
    moosicbox_player::symphonia::play_media_source_with_options(
        MediaSourceStream::new(
            Box::new(Cursor::new(ogg)),
            MediaSourceStreamOptions::default(),
        ),
        &Hint::new(),
        &mut handler,
        options,
        None,
        None,
    )
    .unwrap();
    let captured = capture.lock().unwrap();
    assert_eq!(captured.pcm.len(), 3840 - 312);
    assert_eq!(captured.flushes, 1);
    assert!(captured.pcm.iter().any(|x| x.abs() > 0.001));
    drop(captured);
}

#[test_log::test]
fn native_player_pcm_matches_frozen_stateful_reference() {
    exercise(OpusDecodeBackend::Native);
}
#[cfg(feature = "decoder-opus")]
#[test_log::test]
fn explicit_libopus_player_pcm_matches_reference_when_both_compiled() {
    exercise(OpusDecodeBackend::Libopus);
}
#[cfg(not(feature = "decoder-opus"))]
#[test_log::test]
fn unavailable_libopus_does_not_fall_back_to_native() {
    assert!(
        moosicbox_player::symphonia::play_media_source_with_options(
            source(0),
            &Hint::new(),
            &mut AudioDecodeHandler::new(),
            options(OpusDecodeBackend::Libopus),
            None,
            None
        )
        .is_err()
    );
    assert!(
        moosicbox_player::symphonia_unsync::play_media_source_with_options(
            source(0),
            &Hint::new(),
            options(OpusDecodeBackend::Libopus),
            None,
            None
        )
        .is_err()
    );
}

struct PcmEncoder;
impl moosicbox_audio_output::encoder::AudioEncoder for PcmEncoder {
    fn encode(
        &mut self,
        audio: AudioBuffer<f32>,
    ) -> Result<bytes::Bytes, moosicbox_audio_output::AudioOutputError> {
        assert_eq!(audio.spec().rate(), 48_000);
        assert_eq!(audio.spec().channels().count(), 1);
        Ok(audio
            .plane(0)
            .unwrap()
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<_>>()
            .into())
    }
    fn spec(&self) -> symphonia::core::audio::AudioSpec {
        symphonia::core::audio::AudioSpec::new(
            48_000,
            symphonia::core::audio::layouts::CHANNEL_LAYOUT_MONO,
        )
    }
}
#[test_log::test]
fn signal_chain_native_selection_reaches_pcm_encoder_and_eof() {
    let mut processor = moosicbox_player::signal_chain::SignalChainStep::new()
        .with_decode_options(options(OpusDecodeBackend::Native))
        .with_encoder(|| Box::new(PcmEncoder))
        .process(Box::new(Cursor::new(stream(1536, 0))))
        .unwrap();
    let mut actual = Vec::new();
    processor.read_to_end(&mut actual).unwrap();
    let expected: Vec<_> = expected(1536)
        .iter()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(processor.read(&mut [0; 1]).unwrap(), 0);
}

#[test_log::test(switchy_async::test(real_time))]
async fn async_player_native_selection_reaches_pcm_sink() {
    let capture = Arc::new(Mutex::new(Capture::default()));
    let output = capture.clone();
    let status = moosicbox_player::symphonia::play_media_source_async_with_options(
        source(1536),
        &Hint::new(),
        move || {
            Ok(AudioDecodeHandler::new()
                .with_output(Box::new(move |_, _| Ok(Box::new(Sink(output.clone()))))))
        },
        options(OpusDecodeBackend::Native),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status, 0);
    let capture = capture.lock().unwrap();
    assert_eq!(capture.frames, [960; 4]);
    assert_eq!(capture.flushes, 1);
    assert_eq!(capture.pcm, expected(1536));
    drop(capture);
}
