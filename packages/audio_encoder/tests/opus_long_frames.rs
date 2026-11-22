#![cfg(feature = "opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Qualified native SILK long-frame stream clocks and exact tail finalization.
use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode, OpusStreamEncoder,
};
use moosicbox_ogg_opus::{OggOpusReader, packet_duration};
use std::io::Cursor;

#[test]
fn native_silk_long_frames_preserve_packet_and_audible_clocks() {
    for rate in [8000u32, 12000, 16000, 24000, 48000] {
        for channels in [1u8, 2] {
            for millis in [40usize, 60] {
                let frame = rate as usize * millis / 1000;
                for frames in [frame - 1, frame, frame + 1, 2457] {
                    let config = OpusEncodeConfig {
                        backend: OpusEncodeBackend::Native,
                        mode: OpusEncodeMode::Silk,
                        sample_rate: rate,
                        channels,
                        bitrate: 128000,
                        frame_size: frame,
                    };
                    let mut encoder = OpusStreamEncoder::new(Vec::new(), config).unwrap();
                    let pcm: Vec<_> = (0..frames * usize::from(channels))
                        .map(|index| (index as f32 * 0.017).sin() * 0.1)
                        .collect();
                    for chunk in pcm.chunks(13 * usize::from(channels)) {
                        encoder.write(chunk).unwrap();
                    }
                    let bytes = encoder.finish().unwrap().unwrap();
                    assert!(encoder.finish().unwrap().is_none());
                    assert!(encoder.write(&[]).is_err());
                    let eos = bytes
                        .windows(4)
                        .rposition(|window| window == b"OggS")
                        .unwrap();
                    let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
                    let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
                    let expected = frames as u64 * u64::from(48000 / rate);
                    assert_eq!(
                        granule - u64::from(pre_skip),
                        expected,
                        "rate={rate} channels={channels} ms={millis} frames={frames}"
                    );
                    let mut reader = OggOpusReader::new(Cursor::new(bytes)).unwrap();
                    assert_eq!(reader.total_samples().unwrap(), expected);
                    let mut audible = 0u64;
                    while let Some(packet) = reader.next_packet().unwrap() {
                        assert_eq!(packet_duration(&packet.data).unwrap(), (millis * 48) as u32);
                        assert_eq!(packet.duration, (millis * 48) as u32);
                        audible += u64::from(packet.audible_samples());
                    }
                    assert_eq!(audible, expected);
                }
            }
        }
    }
}

#[test]
fn native_stereo_fullband_hybrid_low_bitrates_preserve_lifecycle_and_clocks() {
    for bitrate in [32_000, 64_000] {
        for frame_size in [480, 960] {
            for frames in [frame_size - 1, frame_size, frame_size + 1, 2457] {
                let config = OpusEncodeConfig {
                    backend: OpusEncodeBackend::Native,
                    mode: OpusEncodeMode::Hybrid,
                    sample_rate: 48_000,
                    channels: 2,
                    bitrate,
                    frame_size,
                };
                let pcm: Vec<_> = (0..frames)
                    .flat_map(|frame| {
                        [0.17_f32, 0.23].map(|frequency| (frame as f32 * frequency).sin() * 0.1)
                    })
                    .collect();
                let encode = |chunk_frames| {
                    let mut encoder = OpusStreamEncoder::new(Vec::new(), config)
                        .unwrap_or_else(|error| panic!("{config:?}: {error}"));
                    encoder.write(&[]).unwrap();
                    for chunk in pcm.chunks(chunk_frames * 2) {
                        encoder
                            .write(chunk)
                            .unwrap_or_else(|error| panic!("{config:?}: {error}"));
                    }
                    let bytes = encoder.finish().unwrap().unwrap();
                    assert!(encoder.finish().unwrap().is_none(), "{config:?}");
                    assert!(encoder.write(&[]).is_err(), "{config:?}");
                    bytes
                };
                let bytes = encode(13);
                assert_eq!(bytes, encode(frames), "chunk independence: {config:?}");
                let eos = bytes
                    .windows(4)
                    .rposition(|window| window == b"OggS")
                    .unwrap();
                let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
                let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
                assert_eq!(granule - u64::from(pre_skip), frames as u64, "{config:?}");
                let mut reader = OggOpusReader::new(Cursor::new(bytes)).unwrap();
                assert_eq!(reader.total_samples().unwrap(), frames as u64, "{config:?}");
                #[cfg(feature = "opus")]
                let mut reference = opus::Decoder::new(48_000, opus::Channels::Stereo).unwrap();
                #[cfg(feature = "opus")]
                let mut reference_audible = Vec::new();
                let mut audible = 0_u64;
                while let Some(packet) = reader.next_packet().unwrap() {
                    // Configurations 14/15 are fullband Hybrid 10/20 ms.
                    let toc = packet.data[0] >> 3;
                    assert_eq!(toc, if frame_size == 480 { 14 } else { 15 }, "{config:?}");
                    assert_eq!(
                        packet_duration(&packet.data).unwrap(),
                        frame_size as u32,
                        "{config:?}"
                    );
                    audible += u64::from(packet.audible_samples());
                    #[cfg(feature = "opus")]
                    {
                        let mut decoded = vec![0_i16; 5760 * 2];
                        let count = reference.decode(&packet.data, &mut decoded, false).unwrap();
                        assert_eq!(count, frame_size, "{config:?}");
                        let start = packet.trim_start as usize;
                        let end = count - packet.trim_end as usize;
                        reference_audible.extend_from_slice(&decoded[start * 2..end * 2]);
                    }
                }
                assert_eq!(audible, frames as u64, "{config:?}");
                #[cfg(feature = "opus")]
                {
                    assert_eq!(reference_audible.len(), frames * 2, "{config:?}");
                    assert!(
                        reference_audible.iter().any(|sample| *sample != 0),
                        "{config:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn long_frames_remain_invalid_outside_explicit_native_silk() {
    for rate in [8000u32, 12000, 16000, 24000, 48000] {
        for millis in [40usize, 60] {
            for backend in [OpusEncodeBackend::Native, OpusEncodeBackend::Libopus] {
                for mode in [
                    OpusEncodeMode::Default,
                    OpusEncodeMode::Auto,
                    OpusEncodeMode::Celt,
                    OpusEncodeMode::Hybrid,
                ] {
                    let mut sink = Vec::new();
                    let result = OpusStreamEncoder::new(
                        &mut sink,
                        OpusEncodeConfig {
                            backend,
                            mode,
                            sample_rate: rate,
                            frame_size: rate as usize * millis / 1000,
                            ..Default::default()
                        },
                    );
                    assert!(
                        result.is_err(),
                        "rate={rate} ms={millis} backend={backend:?} mode={mode:?}"
                    );
                    drop(result);
                    assert!(sink.is_empty(), "invalid configuration emitted headers");
                }
            }
        }
    }
}
