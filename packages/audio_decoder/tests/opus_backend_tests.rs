#![cfg_attr(
    not(any(feature = "opus-native", feature = "opus-libopus")),
    allow(dead_code)
)]
#![cfg(feature = "opus-base")]
use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, DecodeOptions, OpusDecodeBackend,
};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};
use symphonia::core::{
    audio::{Audio, AudioBuffer},
    formats::{FormatOptions, FormatReader, Track, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
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
    stream_packets(gain, pre_skip, &PACKETS)
}
const PACKETS: [&[u8]; 4] = [
    include_bytes!("fixtures/wb_sequence_0.packet"),
    include_bytes!("fixtures/wb_sequence_1.packet"),
    include_bytes!("fixtures/wb_sequence_2.packet"),
    include_bytes!("fixtures/wb_sequence_3.packet"),
];
fn stream_packets(gain: i16, pre_skip: u16, packets: &[&[u8]]) -> Vec<u8> {
    let mut head = b"OpusHead\x01\x01".to_vec();
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&48000_u32.to_le_bytes());
    head.extend_from_slice(&gain.to_le_bytes());
    head.push(0);
    let mut bytes = page(0, 2, 0, &head);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&[0; 8]);
    bytes.extend(page(1, 0, 0, &tags));
    for (i, p) in packets.iter().enumerate() {
        bytes.extend(page(
            u32::try_from(i + 2).unwrap(),
            if i + 1 == packets.len() { 4 } else { 0 },
            u64::try_from((i + 1) * 960).unwrap(),
            p,
        ));
    }
    bytes
}
fn reader(bytes: Vec<u8>) -> Box<dyn FormatReader> {
    symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap()
}
#[derive(Default)]
struct Capture {
    samples: Vec<f32>,
    frames: Vec<usize>,
    flushes: usize,
}
struct Sink(Arc<Mutex<Capture>>);
impl AudioDecode for Sink {
    fn decoded(
        &mut self,
        b: AudioBuffer<f32>,
        _: &Packet,
        _: &Track,
    ) -> Result<(), AudioDecodeError> {
        assert_eq!(b.spec().rate(), 48000);
        assert_eq!(b.spec().channels().count(), 1);
        let mut c = self.0.lock().unwrap();
        c.frames.push(b.frames());
        c.samples.extend_from_slice(b.plane(0).unwrap());
        Ok(())
    }
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}
fn options(backend: OpusDecodeBackend, gapless: bool) -> DecodeOptions {
    let mut o = DecodeOptions {
        opus_backend: backend,
        ..DecodeOptions::default()
    };
    o.decoder.gapless = gapless;
    o.decoder.verify = true;
    o
}
fn expected(gain: i16) -> Vec<f32> {
    let scale = 10_f32.powf(f32::from(gain) / (256.0 * 20.0));
    include_bytes!("fixtures/wb_sequence.api48.pcm")
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0 * scale)
        .collect()
}
fn exercise(backend: OpusDecodeBackend) {
    for gain in [0, 1536] {
        let capture = Arc::new(Mutex::new(Capture::default()));
        let c = Arc::clone(&capture);
        let mut handler = AudioDecodeHandler::new().with_output(Box::new(move |spec, _| {
            assert_eq!(spec.rate(), 48000);
            Ok(Box::new(Sink(Arc::clone(&c))))
        }));
        // Public media-source API, actual Ogg demuxing and stateful decoding, no hardware.
        let source =
            MediaSourceStream::new(Box::new(Cursor::new(stream(gain, 0))), Default::default());
        assert_eq!(
            moosicbox_audio_decoder::decode_media_source_with_options(
                source,
                &Hint::new(),
                &mut handler,
                options(backend, false),
                None,
                None
            )
            .unwrap(),
            0
        );
        let c = capture.lock().unwrap();
        assert_eq!(c.frames, [960; 4]);
        assert_eq!(c.flushes, 1);
        assert_eq!(c.samples, expected(gain));
        assert!(c.samples.iter().any(|x| *x != 0.0));
        drop(c);
        let buffers: Vec<_> = moosicbox_audio_decoder::unsync::decode_with_options(
            reader(stream(gain, 0)),
            None,
            None,
            options(backend, false),
        )
        .unwrap()
        .iter()
        .collect();
        assert_eq!(
            buffers.iter().map(Audio::frames).collect::<Vec<_>>(),
            [960; 4]
        );
        assert_eq!(
            buffers
                .iter()
                .flat_map(|b| b.plane(0).unwrap().iter().copied())
                .collect::<Vec<_>>(),
            expected(gain)
        );
    }
    let gapless: Vec<_> = moosicbox_audio_decoder::unsync::decode_with_options(
        reader(stream(0, 0)),
        None,
        None,
        options(backend, true),
    )
    .unwrap()
    .iter()
    .flat_map(|b| b.plane(0).unwrap().to_vec())
    .collect();
    assert_eq!(gapless, expected(0));
    // Gapless=false must retain raw samples even with a declared container delay.
    let samples: Vec<_> = moosicbox_audio_decoder::unsync::decode_with_options(
        reader(stream(0, 312)),
        None,
        None,
        options(backend, false),
    )
    .unwrap()
    .iter()
    .flat_map(|b| b.plane(0).unwrap().to_vec())
    .collect();
    assert_eq!(samples, expected(0));
}
#[cfg(feature = "opus-native")]
#[test]
fn native_public_ogg_pcm_is_nonzero_stateful_exact_and_flushes() {
    exercise(OpusDecodeBackend::Native);
}
#[cfg(feature = "opus-libopus")]
#[test]
fn libopus_public_ogg_pcm_is_exact_and_flushes() {
    exercise(OpusDecodeBackend::Libopus);
}
#[test]
fn unavailable_backend_is_an_error_not_a_fallback() {
    for (backend, available) in [
        (OpusDecodeBackend::Native, cfg!(feature = "opus-native")),
        (OpusDecodeBackend::Libopus, cfg!(feature = "opus-libopus")),
    ] {
        if !available {
            assert!(
                moosicbox_audio_decoder::unsync::decode_with_options(
                    reader(stream(0, 0)),
                    None,
                    None,
                    options(backend, false)
                )
                .is_err()
            );
            let mut h = AudioDecodeHandler::new();
            assert!(
                moosicbox_audio_decoder::decode_with_options(
                    reader(stream(0, 0)),
                    &mut h,
                    None,
                    None,
                    options(backend, false)
                )
                .is_err()
            );
        }
    }
}

#[cfg(all(feature = "opus-native", not(feature = "opus-libopus")))]
#[test]
fn compatibility_wrapper_does_not_change_backend_in_native_only_build() {
    let mut handler = AudioDecodeHandler::new();
    assert!(
        moosicbox_audio_decoder::decode(
            reader(stream(0, 0)),
            &mut handler,
            None,
            None,
            Default::default()
        )
        .is_err()
    );
    assert!(
        moosicbox_audio_decoder::unsync::decode(
            reader(stream(0, 0)),
            None,
            None,
            Default::default()
        )
        .is_err()
    );
}

fn exercise_hybrid(backend: OpusDecodeBackend) {
    let packets: [&[u8]; 4] = [
        include_bytes!("fixtures/hybrid_sequence_0.packet"),
        include_bytes!("fixtures/hybrid_sequence_1.packet"),
        include_bytes!("fixtures/hybrid_sequence_2.packet"),
        include_bytes!("fixtures/hybrid_sequence_3.packet"),
    ];
    let expected: Vec<f32> = (match backend {
        OpusDecodeBackend::Native => include_bytes!("fixtures/hybrid_sequence.pcm").as_slice(),
        OpusDecodeBackend::Libopus => {
            include_bytes!("fixtures/hybrid_sequence.libopus.pcm").as_slice()
        }
    })
    .as_chunks::<2>()
    .0
    .iter()
    .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
    .collect();
    assert!(expected.iter().any(|x| *x != 0.0));
    let capture = Arc::new(Mutex::new(Capture::default()));
    let sink = Arc::clone(&capture);
    let mut handler = AudioDecodeHandler::new()
        .with_output(Box::new(move |_, _| Ok(Box::new(Sink(Arc::clone(&sink))))));
    let source = MediaSourceStream::new(
        Box::new(Cursor::new(stream_packets(0, 0, &packets))),
        Default::default(),
    );
    assert_eq!(
        moosicbox_audio_decoder::decode_media_source_with_options(
            source,
            &Hint::new(),
            &mut handler,
            options(backend, false),
            None,
            None
        )
        .unwrap(),
        0
    );
    let c = capture.lock().unwrap();
    assert_eq!(c.frames, [960; 4]);
    assert_eq!(c.flushes, 1);
    assert_eq!(c.samples, expected);
    drop(c);
    let buffers: Vec<_> = moosicbox_audio_decoder::unsync::decode_with_options_result(
        reader(stream_packets(0, 0, &packets)),
        None,
        None,
        options(backend, false),
    )
    .unwrap()
    .iter()
    .map(Result::unwrap)
    .collect();
    assert_eq!(
        buffers.iter().map(Audio::frames).collect::<Vec<_>>(),
        [960; 4]
    );
    assert_eq!(
        buffers
            .iter()
            .flat_map(|b| b.plane(0).unwrap().iter().copied())
            .collect::<Vec<_>>(),
        expected
    );
}
#[cfg(feature = "opus-native")]
#[test]
fn native_public_hybrid_ogg_is_stateful_exact_and_flushes() {
    exercise_hybrid(OpusDecodeBackend::Native);
}
#[cfg(feature = "opus-libopus")]
#[test]
fn libopus_public_hybrid_ogg_is_stateful_exact_and_flushes() {
    exercise_hybrid(OpusDecodeBackend::Libopus);
}
