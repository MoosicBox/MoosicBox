#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![cfg(feature = "native")]

use moosicbox_ogg_opus::OpusHead;
use moosicbox_opus::{OpusBackend, OpusDecoder};
use symphonia::core::{
    audio::layouts::CHANNEL_LAYOUT_MONO,
    codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_OPUS},
};

fn mono_params(header: Vec<u8>) -> AudioCodecParameters {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48_000)
        .with_channels(CHANNEL_LAYOUT_MONO)
        .with_extra_data(header.into_boxed_slice());
    params
}

#[test]
fn identification_validation_uses_authoritative_parser() {
    let valid = OpusHead::new(1, 312, 48_000).unwrap().encode().unwrap();
    assert!(
        OpusDecoder::try_new_with_backend(&mono_params(valid.clone()), OpusBackend::Native).is_ok()
    );
    let mut invalid_version = valid.clone();
    invalid_version[8] = 0;
    let mut invalid_channels = valid.clone();
    invalid_channels[9] = 2;
    for invalid in [valid[..18].to_vec(), invalid_version, invalid_channels] {
        assert!(
            OpusDecoder::try_new_with_backend(&mono_params(invalid), OpusBackend::Native).is_err()
        );
    }
}

#[test]
fn undefined_mapping_never_silently_collapses_to_stereo() {
    let mut head = OpusHead::new(1, 0, 48_000).unwrap();
    head.mapping_family = 255;
    head.channel_mapping = vec![0];
    let params = mono_params(head.encode().unwrap());
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err());
}

#[test]
fn surround_requires_matching_speakers_and_explicit_backend() {
    use symphonia::core::audio::layouts::{CHANNEL_LAYOUT_5P1_SIDE, CHANNEL_LAYOUT_OGG_5P1};
    let mut head = OpusHead::new(1, 0, 48_000).unwrap();
    head.channels = 6;
    head.mapping_family = 1;
    head.stream_count = 4;
    head.coupled_count = 2;
    head.channel_mapping = vec![0, 4, 1, 2, 3, 5];
    let mut params = mono_params(head.encode().unwrap());
    params.with_channels(CHANNEL_LAYOUT_OGG_5P1);
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_ok());
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Libopus).is_err());
    params.with_channels(CHANNEL_LAYOUT_5P1_SIDE);
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err());
}

#[test]
fn native_surround_routing_gain_trim_and_reset_match_codec() {
    use moosicbox_opus_native::{
        Channels, SampleRate,
        encoder::EncoderConfig,
        multistream::{MultistreamDecoder, MultistreamEncoder, MultistreamLayout},
    };
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef, layouts::CHANNEL_LAYOUT_OGG_5P1},
        codecs::audio::AudioDecoder,
        packet::Packet,
    };
    let mapping = vec![0, 4, 1, 2, 3, 5];
    let layout = MultistreamLayout::new(4, 2, mapping.clone()).unwrap();
    let configs = (0..4)
        .map(|stream| EncoderConfig {
            sample_rate: SampleRate::Hz48000,
            channels: if stream < 2 {
                Channels::Stereo
            } else {
                Channels::Mono
            },
            bitrate: if stream < 2 { 128_000 } else { 64_000 },
            frame_size: 960,
        })
        .collect();
    let mut encoder = MultistreamEncoder::new(layout.clone(), configs).unwrap();
    let input: Vec<_> = (0..960)
        .flat_map(|frame| {
            (0..6).map(move |ch| {
                if frame % (29 + ch * 10) < 13 {
                    0.05 * (ch + 1) as f32
                } else {
                    -0.05 * (ch + 1) as f32
                }
            })
        })
        .collect();
    let data = encoder.encode(&input).unwrap();
    let mut head = OpusHead::new(1, 0, 48_000).unwrap();
    head.channels = 6;
    head.mapping_family = 1;
    head.stream_count = 4;
    head.coupled_count = 2;
    head.channel_mapping = mapping;
    head.output_gain = -768;
    let mut params = mono_params(head.encode().unwrap());
    params.with_channels(CHANNEL_LAYOUT_OGG_5P1);
    let mut adapter = OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).unwrap();
    let mut direct = MultistreamDecoder::new(SampleRate::Hz48000, layout).unwrap();
    // Public float adapter gain is intentionally after i16 normalization;
    // the codec's separately qualified Q8/i16 API stays at zero here.
    let factor = 10.0_f32.powf(f32::from(head.output_gain) / (256.0 * 20.0));
    let mut expected = vec![0; 960 * 6];
    assert_eq!(
        direct
            .decode(Some(&data), &mut expected, 960, false)
            .unwrap(),
        960
    );
    let mut packet = Packet::new(0, 0.into(), 960_u32.into(), data);
    packet.trim_start = 17_u32.into();
    packet.trim_end = 23_u32.into();
    for _ in 0..2 {
        let GenericAudioBufferRef::F32(pcm) = adapter.decode_ref(&packet.as_packet_ref()).unwrap()
        else {
            panic!("expected float PCM")
        };
        assert_eq!(pcm.frames(), 920);
        for (plane, source) in [0, 2, 1, 5, 3, 4].into_iter().enumerate() {
            for (i, &sample) in pcm.plane(plane).unwrap().iter().enumerate() {
                assert_eq!(
                    sample,
                    (f32::from(expected[(i + 17) * 6 + source]) / 32768.0) * factor
                );
            }
        }
        adapter.reset();
    }
    assert!(
        adapter
            .decode_ref(&Packet::new(0, 0.into(), 0_u32.into(), Vec::<u8>::new()).as_packet_ref())
            .is_err()
    );
    assert_eq!(adapter.last_decoded().frames(), 0);
}
