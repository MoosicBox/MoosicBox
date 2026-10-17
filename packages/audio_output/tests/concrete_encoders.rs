#![cfg(all(feature = "aac", feature = "flac", feature = "mp3", feature = "opus"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_audio_decoder::AudioDecode;
use moosicbox_audio_output::{
    AudioOutputError,
    encoder::{
        AudioEncoder, aac::AacEncoder, flac::FlacEncoder, mp3::Mp3Encoder, opus::OpusEncoder,
    },
};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};
use symphonia::core::{
    audio::{Audio, AudioBuffer, AudioMut, AudioSpec, layouts},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    packet::Packet,
    units::Duration,
};

fn input(rate: u32, mono: bool) -> AudioBuffer<f32> {
    let channels = if mono {
        layouts::CHANNEL_LAYOUT_MONO
    } else {
        layouts::CHANNEL_LAYOUT_STEREO
    };
    let mut buffer = AudioBuffer::new(AudioSpec::new(rate, channels), 4096);
    buffer.render_silence(Some(512));
    for (ch, plane) in buffer.iter_planes_mut().enumerate() {
        for (i, sample) in plane.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let phase = i as f32 * 0.1;
            *sample = phase.sin() * if ch == 0 { 0.25 } else { -0.125 };
        }
    }
    buffer
}
fn factory(kind: &str) -> Box<dyn AudioEncoder> {
    match kind {
        "aac" => Box::new(AacEncoder::new()),
        "flac" => Box::new(FlacEncoder::new()),
        "mp3" => Box::new(Mp3Encoder::new()),
        "opus" => Box::new(OpusEncoder::new()),
        _ => unreachable!(),
    }
}
fn decoded_frames(kind: &str, bytes: &[u8], rate: u32) -> usize {
    assert!(!bytes.is_empty(), "{kind} emitted no stream");
    eprintln!(
        "{kind} bytes={} prefix={:?}",
        bytes.len(),
        &bytes[..bytes.len().min(16)]
    );
    let source = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension(if kind == "opus" { "ogg" } else { kind });
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let track = reader
        .default_track(symphonia::core::formats::TrackType::Audio)
        .unwrap();
    let params = track.codec_params.as_ref().unwrap().audio().unwrap();
    assert_eq!(params.sample_rate, Some(rate));
    assert_eq!(params.channels.as_ref().unwrap().count(), 2);
    let mut frames = 0;
    let mut nonzero = false;
    let mut channel_nonzero = [false; 2];
    if kind == "opus" {
        let mut decoder = opus::Decoder::new(48_000, opus::Channels::Stereo).unwrap();
        loop {
            let packet = match reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(symphonia::core::errors::Error::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => panic!("{kind} demux failed: {error}"),
            };
            let mut pcm = vec![0.0; 5760 * 2];
            let count = decoder.decode_float(&packet.data, &mut pcm, false).unwrap();
            nonzero |= pcm[..count * 2].iter().any(|s| s.abs() > 0.001);
            for frame in pcm[..count * 2].as_chunks::<2>().0 {
                for channel in 0..2 {
                    channel_nonzero[channel] |= frame[channel].abs() > 0.001;
                }
            }
            frames += count;
        }
    } else {
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default().gapless(false))
            .unwrap();
        loop {
            let packet = match reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(symphonia::core::errors::Error::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => panic!("{kind} demux failed: {error}"),
            };
            let pcm = decoder.decode(&packet).unwrap();
            let mut buffer = AudioBuffer::<f32>::new(pcm.spec().clone(), pcm.frames());
            buffer.resize_uninit(pcm.frames());
            pcm.copy_to(&mut buffer);
            assert_eq!(buffer.spec().rate(), rate);
            assert_eq!(buffer.num_planes(), 2);
            nonzero |= buffer.iter_planes().flatten().any(|s| s.abs() > 0.001);
            for (channel, plane) in buffer.iter_planes().enumerate() {
                channel_nonzero[channel] |= plane.iter().any(|s| s.abs() > 0.001);
            }
            frames += buffer.frames();
        }
    }
    assert!(nonzero, "{kind} decoded only silence");
    assert_eq!(channel_nonzero, [true; 2], "{kind} lost a channel");
    frames
}

#[test_log::test]
fn concrete_pending_inputs_and_finish_are_decodable_once() {
    for kind in ["aac", "flac", "mp3", "opus"] {
        let mut encoder = factory(kind);
        let rate = encoder.spec().rate();
        let source_rate = if rate == 48_000 { 44_100 } else { 48_000 };
        assert!(matches!(
            encoder.encode(input(source_rate, true)),
            Err(AudioOutputError::UnsupportedChannels(1))
        ));
        for _ in 0..2 {
            assert!(
                encoder
                    .encode(input(source_rate, false))
                    .unwrap()
                    .is_empty(),
                "{kind} should buffer sub-block input"
            );
        }
        let bytes = encoder.finish().unwrap();
        assert!(
            encoder.finish().unwrap().is_empty(),
            "{kind} finalized twice"
        );
        let frames = decoded_frames(kind, &bytes, rate);
        eprintln!("{kind} frames={frames}");
        assert_eq!(
            frames,
            match kind {
                "aac" => 4096,
                "flac" => 1911,
                "mp3" => 4608,
                "opus" => 2880,
                _ => unreachable!(),
            },
            "{kind} changed tail framing"
        );
    }
}

#[derive(Clone, Default)]
struct Writer(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test_log::test]
fn concrete_audio_decode_callbacks_accept_pending_and_flush_once() {
    for kind in ["aac", "flac", "mp3", "opus"] {
        let writer = Writer::default();
        let sink = writer.0.clone();
        let mut encoder: Box<dyn AudioDecode> = match kind {
            "aac" => Box::new(AacEncoder::with_writer(writer)),
            "flac" => Box::new(FlacEncoder::with_writer(writer)),
            "mp3" => Box::new(Mp3Encoder::with_writer(writer)),
            "opus" => Box::new(OpusEncoder::with_writer(writer)),
            _ => unreachable!(),
        };
        let rate = if kind == "opus" { 48_000 } else { 44_100 };
        let source_rate = if rate == 48_000 { 44_100 } else { 48_000 };
        let packet = Packet::new(0, 0.into(), Duration::new(512), Vec::new());
        let track = symphonia::core::formats::Track::new(0);
        for _ in 0..2 {
            encoder
                .decoded(input(source_rate, false), &packet, &track)
                .unwrap();
        }
        assert!(sink.lock().unwrap().is_empty());
        encoder.flush().unwrap();
        let first = sink.lock().unwrap().clone();
        encoder.flush().unwrap();
        assert_eq!(*sink.lock().unwrap(), first);
        decoded_frames(kind, &first, rate);
    }
}

#[test_log::test]
fn flac_pending_tail_roundtrip() {
    let mut encoder = FlacEncoder::new();
    for _ in 0..2 {
        assert!(encoder.encode(input(48_000, false)).unwrap().is_empty());
    }
    let bytes = encoder.finish().unwrap();
    assert!(encoder.finish().unwrap().is_empty());
    assert_eq!(decoded_frames("flac", &bytes, 44_100), 1911);
}

#[test_log::test]
fn mp3_pending_tail_roundtrip() {
    let mut encoder = Mp3Encoder::new();
    for _ in 0..2 {
        assert!(encoder.encode(input(48_000, false)).unwrap().is_empty());
    }
    let bytes = encoder.finish().unwrap();
    assert!(encoder.finish().unwrap().is_empty());
    assert_eq!(decoded_frames("mp3", &bytes, 44_100), 4608);
}

#[test_log::test]
fn concrete_flac_mp3_native_rate_subblock_tail() {
    for kind in ["flac", "mp3"] {
        let mut encoder = factory(kind);
        let mut pcm = input(44_100, false);
        pcm.truncate(37);
        let mut bytes = encoder.encode(pcm).unwrap().to_vec();
        bytes.extend_from_slice(&encoder.finish().unwrap());
        assert!(encoder.finish().unwrap().is_empty());
        let frames = decoded_frames(kind, &bytes, 44_100);
        eprintln!("tiny {kind} frames={frames}");
        assert_eq!(frames, if kind == "flac" { 37 } else { 3456 });
    }
}

#[test_log::test]
fn mp3_multiple_native_rate_batches_finalize_once() {
    let mut encoder = Mp3Encoder::new();
    let mut bytes = Vec::new();
    for _ in 0..8 {
        bytes.extend_from_slice(&encoder.encode(input(44_100, false)).unwrap());
    }
    bytes.extend_from_slice(&encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    assert!(matches!(
        encoder.encode(input(44_100, false)),
        Err(AudioOutputError::StreamEnd)
    ));
    let frames = decoded_frames("mp3", &bytes, 44_100);
    eprintln!("multi mp3 frames={frames}");
    assert_eq!(frames, 6912);
}
