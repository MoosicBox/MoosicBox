#![cfg(feature = "opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusStreamEncoder,
};
use moosicbox_audio_output::encoder::{AudioEncoder, opus::SelectedOpusEncoder};
use std::{
    io::{Cursor, Write},
    process::{Command, Stdio},
};
use symphonia::core::audio::{
    Audio, AudioBuffer, AudioMut, AudioSpec,
    layouts::{CHANNEL_LAYOUT_MONO, CHANNEL_LAYOUT_STEREO},
};
use symphonia::core::codecs::audio::AudioDecoder;

fn config(channels: u8) -> OpusEncodeConfig {
    OpusEncodeConfig {
        backend: OpusEncodeBackend::Native,
        channels,
        ..OpusEncodeConfig::default()
    }
}
fn encode(channels: u8, rate: u32, frames: usize) -> Vec<u8> {
    let mut encoder = SelectedOpusEncoder::new(config(channels)).unwrap();
    assert_eq!(encoder.backend(), OpusEncodeBackend::Native);
    let spec = AudioSpec::new(
        rate,
        if channels == 1 {
            CHANNEL_LAYOUT_MONO
        } else {
            CHANNEL_LAYOUT_STEREO
        },
    );
    let mut audio = AudioBuffer::<f32>::new(spec, frames);
    audio.render_silence(Some(frames));
    for (channel, plane) in audio.iter_planes_mut().enumerate() {
        for (i, sample) in plane.iter_mut().enumerate() {
            *sample = (std::f32::consts::TAU * (440.0 + channel as f32 * 220.0) * i as f32
                / rate as f32)
                .sin()
                * 0.2;
        }
    }
    let mut bytes = encoder.encode(audio).unwrap().to_vec();
    bytes.extend_from_slice(&encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    bytes
}
fn ffmpeg(bytes: &[u8]) -> Vec<u8> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i", "pipe:0", "-f", "f32le", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("independent FFmpeg required");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
#[test]
fn native_partial_tail_crossdecodes_with_independent_ffmpeg() {
    for channels in [1, 2] {
        let bytes = encode(channels, 48_000, 2457);
        let decoded = ffmpeg(&bytes);
        assert_eq!(decoded.len(), 2457 * usize::from(channels) * 4);
        assert!(
            decoded
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| f32::from_le_bytes(*x))
                .any(|x| x.abs() > 0.001)
        );
        let mut reader = moosicbox_audio_decoder::create_format_probe()
            .probe(
                &symphonia::core::formats::probe::Hint::new(),
                symphonia::core::io::MediaSourceStream::new(
                    Box::new(Cursor::new(bytes)),
                    Default::default(),
                ),
                Default::default(),
                Default::default(),
            )
            .unwrap();
        let track = reader.tracks()[0].clone();
        let options = moosicbox_audio_decoder::DecodeOptions {
            opus_backend: moosicbox_audio_decoder::OpusDecodeBackend::Native,
            ..Default::default()
        };
        let mut decoder = moosicbox_opus::OpusDecoder::try_new_with_backend_options(
            track.codec_params.as_ref().unwrap().audio().unwrap(),
            moosicbox_opus::OpusBackend::Native,
            &options.decoder,
        )
        .unwrap();
        let mut frames = 0;
        while let Some(packet) = reader.next_packet().unwrap() {
            frames += decoder.decode(&packet).unwrap().frames();
        }
        assert_eq!(frames, 2457);
    }
}
#[test]
fn native_input_rate_short_frame_clock_matrix() {
    use symphonia::core::{
        codecs::audio::AudioDecoderOptions,
        formats::{FormatOptions, SeekMode, SeekTo},
    };
    for rate in [8000_u32, 12000, 16000, 24000, 48000] {
        for divisor in [400, 200, 100, 50] {
            for channels in [1, 2] {
                let frame_size = rate as usize / divisor;
                let frames = frame_size * 5 + frame_size / 3;
                let config = OpusEncodeConfig {
                    sample_rate: rate,
                    frame_size,
                    channels,
                    backend: OpusEncodeBackend::Native,
                    ..Default::default()
                };
                let pcm: Vec<_> = (0..frames)
                    .flat_map(|i| {
                        (0..channels).map(move |ch| {
                            (std::f32::consts::TAU * (330.0 + f32::from(ch) * 110.0) * i as f32
                                / rate as f32)
                                .sin()
                                * 0.2
                        })
                    })
                    .collect();
                let mut encoder = OpusStreamEncoder::new(Vec::new(), config).unwrap();
                encoder.write(&pcm).unwrap();
                let bytes = encoder.finish().unwrap().unwrap();
                assert!(encoder.finish().unwrap().is_none());
                assert!(encoder.write(&pcm[..usize::from(channels)]).is_err());
                let count = frames * (48000 / rate) as usize;
                let independent = ffmpeg(&bytes);
                assert_eq!(
                    independent.len(),
                    count * usize::from(channels) * 4,
                    "rate={rate} frame={frame_size} channels={channels}"
                );
                assert!(
                    independent
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|x| f32::from_le_bytes(*x))
                        .any(|x| x.abs() > 0.001)
                );
                let mut reader = moosicbox_audio_decoder::create_format_probe()
                    .probe(
                        &symphonia::core::formats::probe::Hint::new(),
                        symphonia::core::io::MediaSourceStream::new(
                            Box::new(Cursor::new(bytes)),
                            Default::default(),
                        ),
                        FormatOptions::default(),
                        Default::default(),
                    )
                    .unwrap();
                let params = reader.tracks()[0]
                    .codec_params
                    .as_ref()
                    .unwrap()
                    .audio()
                    .unwrap()
                    .clone();
                let mut decoder = moosicbox_opus::OpusDecoder::try_new_with_backend_options(
                    &params,
                    moosicbox_opus::OpusBackend::Native,
                    &AudioDecoderOptions::default(),
                )
                .unwrap();
                let mut native = Vec::new();
                while let Some(packet) = reader.next_packet().unwrap() {
                    let symphonia::core::audio::GenericAudioBufferRef::F32(audio) =
                        decoder.decode(&packet).unwrap()
                    else {
                        panic!("f32 required")
                    };
                    native.extend_from_slice(audio.plane(0).unwrap());
                }
                assert_eq!(native.len(), count);
                assert!(native.iter().any(|x| x.abs() > 0.001));
                let target = count / 2;
                reader
                    .seek(
                        SeekMode::Accurate,
                        SeekTo::Timestamp {
                            ts: i64::try_from(target).unwrap().into(),
                            track_id: 0,
                        },
                    )
                    .unwrap();
                decoder.reset();
                let mut sought = Vec::new();
                while let Some(packet) = reader.next_packet().unwrap() {
                    let skip = usize::try_from(
                        (i64::try_from(target).unwrap()
                            - packet.pts.get()
                            - i64::try_from(packet.trim_start.get()).unwrap())
                        .max(0),
                    )
                    .unwrap();
                    let symphonia::core::audio::GenericAudioBufferRef::F32(audio) =
                        decoder.decode(&packet).unwrap()
                    else {
                        panic!("f32 required")
                    };
                    let plane = audio.plane(0).unwrap();
                    sought.extend_from_slice(&plane[skip.min(plane.len())..]);
                }
                assert_eq!(sought, native[target..]);
            }
        }
    }
}

#[cfg(feature = "opus")]
#[test]
fn libopus_short_frame_drain_uses_48k_clock() {
    for rate in [8000_u32, 48000] {
        for divisor in [400, 50] {
            let frame_size = rate as usize / divisor;
            let frames = frame_size * 3 + 1;
            let mut encoder = OpusStreamEncoder::new(
                Vec::new(),
                OpusEncodeConfig {
                    backend: OpusEncodeBackend::Libopus,
                    channels: 1,
                    sample_rate: rate,
                    frame_size,
                    ..Default::default()
                },
            )
            .unwrap();
            encoder.write(&vec![0.2; frames]).unwrap();
            let bytes = encoder.finish().unwrap().unwrap();
            assert!(encoder.finish().unwrap().is_none());
            assert_eq!(ffmpeg(&bytes).len(), frames * (48000 / rate) as usize * 4);
        }
    }
}

#[test]
fn native_resampling_finishes_once() {
    let bytes = encode(2, 24_000, 1200);
    assert_eq!(ffmpeg(&bytes).len(), 2400 * 2 * 4);
}
#[test]
fn unavailable_backend_and_bad_pcm_are_errors() {
    #[cfg(not(feature = "opus"))]
    assert!(OpusStreamEncoder::new(Vec::new(), OpusEncodeConfig::default()).is_err());
    let mut encoder = OpusStreamEncoder::new(Vec::new(), config(2)).unwrap();
    assert!(encoder.write(&[0.1]).is_err());
    assert!(encoder.write(&[f32::NAN, 0.0]).is_err());
}
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("test sink failure"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn output_error_is_not_swallowed() {
    assert!(OpusStreamEncoder::new(Broken, config(1)).is_err());
}
