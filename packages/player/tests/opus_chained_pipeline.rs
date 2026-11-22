#![cfg(feature = "encoder-opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Public player integration for independent logical Ogg sections and surround routing.
use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::encoder::{
    AudioEncoder,
    opus::{
        MultistreamLayout, OpusEncodeBackend, OpusEncodeConfig, OpusMultistreamEncodeConfig,
        SelectedOpusEncoder,
    },
};
use moosicbox_player::{DecodeOptions, OpusDecodeBackend};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};
use symphonia::core::{
    audio::{
        Audio, AudioBuffer, AudioMut, AudioSpec,
        layouts::{
            CHANNEL_LAYOUT_3P0, CHANNEL_LAYOUT_5P1, CHANNEL_LAYOUT_7P1, CHANNEL_LAYOUT_MONO,
            CHANNEL_LAYOUT_STEREO,
        },
    },
    formats::{Track, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    packet::Packet,
};

type Captured = Vec<(usize, Vec<f32>)>;
struct Sink(Arc<Mutex<Captured>>);
impl AudioDecode for Sink {
    fn decoded(
        &mut self,
        audio: AudioBuffer<f32>,
        _: &Packet,
        _: &Track,
    ) -> Result<(), AudioDecodeError> {
        let channels = audio.spec().channels().count();
        let mut pcm = Vec::new();
        for frame in 0..audio.frames() {
            for channel in 0..channels {
                pcm.push(audio.plane(channel).unwrap()[frame]);
            }
        }
        self.0.lock().unwrap().push((channels, pcm));
        Ok(())
    }
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        Ok(())
    }
}
fn playback(bytes: Vec<u8>) -> Captured {
    let capture = Arc::new(Mutex::new(Vec::new()));
    let sink = capture.clone();
    let mut handler = AudioDecodeHandler::new()
        .with_output(Box::new(move |_, _| Ok(Box::new(Sink(sink.clone())))));
    moosicbox_player::symphonia::play_media_source_with_options(
        MediaSourceStream::new(
            Box::new(Cursor::new(bytes)),
            MediaSourceStreamOptions::default(),
        ),
        &Hint::new(),
        &mut handler,
        DecodeOptions {
            opus_backend: OpusDecodeBackend::Native,
            ..Default::default()
        },
        None,
        None,
    )
    .unwrap();
    capture.lock().unwrap().clone()
}
fn encode(channels: usize, phase: f32, surround: bool) -> Vec<u8> {
    let spec = AudioSpec::new(
        48000,
        match channels {
            1 => CHANNEL_LAYOUT_MONO,
            2 => CHANNEL_LAYOUT_STEREO,
            3 => CHANNEL_LAYOUT_3P0,
            6 => CHANNEL_LAYOUT_5P1,
            8 => CHANNEL_LAYOUT_7P1,
            _ => unreachable!(),
        },
    );
    let mut encoder = if surround {
        let (streams, coupled, mapping) = match channels {
            3 => (2, 1, vec![0, 2, 1]),
            6 => (4, 2, vec![0, 4, 1, 2, 3, 5]),
            8 => (5, 3, vec![0, 6, 1, 2, 3, 4, 5, 7]),
            _ => unreachable!(),
        };
        SelectedOpusEncoder::new_multistream(OpusMultistreamEncodeConfig {
            sample_rate: 48000,
            frame_size: 960,
            layout: MultistreamLayout::new(streams, coupled, mapping).unwrap(),
            bitrates: vec![64000; streams as usize],
            mapping_family: 1,
        })
        .unwrap()
    } else {
        SelectedOpusEncoder::new(OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            channels: channels as u8,
            ..Default::default()
        })
        .unwrap()
    };
    let mut audio = AudioBuffer::<f32>::new(spec, 2457);
    audio.render_silence(Some(2457));
    for channel in 0..channels {
        for (frame, sample) in audio.plane_mut(channel).unwrap().iter_mut().enumerate() {
            *sample = (frame as f32 * (channel + 1) as f32 * 0.03 + phase).sin() * 0.1;
        }
    }
    let mut bytes = encoder.encode(audio).unwrap().to_vec();
    bytes.extend_from_slice(&encoder.finish().unwrap());
    bytes
}
fn new_serial(bytes: &mut [u8], serial: u32) {
    let mut offset = 0;
    while offset < bytes.len() {
        let segments = usize::from(bytes[offset + 26]);
        let body: usize = bytes[offset + 27..offset + 27 + segments]
            .iter()
            .map(|&value| usize::from(value))
            .sum();
        let end = offset + 27 + segments + body;
        bytes[offset + 14..offset + 18].copy_from_slice(&serial.to_le_bytes());
        bytes[offset + 22..offset + 26].fill(0);
        let mut crc = 0u32;
        for &byte in &bytes[offset..end] {
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ 0x04c1_1db7
                } else {
                    crc << 1
                };
            }
        }
        bytes[offset + 22..offset + 26].copy_from_slice(&crc.to_le_bytes());
        offset = end;
    }
}
#[test]
fn chained_independent_sections_preserve_pcm_same_and_changed_headers() {
    for second_channels in [1, 2] {
        let first = encode(1, 0.0, false);
        let mut second = encode(second_channels, 0.7, false);
        new_serial(&mut second, 42);
        let mut expected = playback(first.clone());
        expected.extend(playback(second.clone()));
        let mut chained = first;
        chained.extend(second);
        assert_eq!(
            playback(chained),
            expected,
            "second channels={second_channels}"
        );
    }
}
#[test]
fn family1_surround_reaches_public_player_with_distinct_channel_routing() {
    for channels in [3, 6, 8] {
        let actual = playback(encode(channels, 0.0, true));
        assert!(actual.iter().all(|(count, _)| *count == channels));
        assert_eq!(
            actual
                .iter()
                .map(|(_, pcm)| pcm.len() / channels)
                .sum::<usize>(),
            2457
        );
        let pcm: Vec<_> = actual.into_iter().flat_map(|(_, pcm)| pcm).collect();
        for channel in 0..channels {
            assert!(
                pcm.chunks_exact(channels)
                    .any(|frame| frame[channel].abs() > 0.001)
            );
        }
        assert!(
            pcm.chunks_exact(channels)
                .any(|frame| frame[0] != frame[1] && frame[1] != frame[2])
        );
    }
}
