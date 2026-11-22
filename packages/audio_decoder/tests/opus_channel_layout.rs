#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![cfg(feature = "opus-native")]

use moosicbox_opus::{OpusBackend, OpusDecoder};
use symphonia::core::{
    audio::{Channels, Position, layouts::CHANNEL_LAYOUT_MONO},
    codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_OPUS},
};

fn params(count: u8, family: u8, channels: Channels) -> AudioCodecParameters {
    let mut head = b"OpusHead\x01".to_vec();
    head.push(count);
    head.extend_from_slice(&0_u16.to_le_bytes());
    head.extend_from_slice(&48_000_u32.to_le_bytes());
    head.extend_from_slice(&0_i16.to_le_bytes());
    head.push(family);
    if family != 0 {
        head.extend_from_slice(&[count, 0]);
        head.extend(0..count);
    }
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48_000)
        .with_channels(channels)
        .with_extra_data(head.into_boxed_slice());
    params
}

#[test]
fn mono_stock_and_canonical_positions_are_accepted_for_both_families() {
    for family in [0, 1] {
        for channels in [CHANNEL_LAYOUT_MONO, Position::FRONT_LEFT.into()] {
            let params = params(1, family, channels);
            assert!(
                OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_ok(),
                "family {family}, channels {:?}",
                params.channels
            );
        }
    }
}

#[test]
fn mono_alias_does_not_accept_other_speakers_or_wrong_multichannel_positions() {
    for family in [0, 1] {
        for position in [Position::FRONT_RIGHT, Position::REAR_CENTER, Position::LFE1] {
            let params = params(1, family, position.into());
            assert!(
                OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err(),
                "family {family}, position {position:?}"
            );
        }
        let params = params(
            2,
            family,
            (Position::FRONT_LEFT | Position::FRONT_CENTER).into(),
        );
        assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err());
    }
    // A family-one surround layout with six speakers, but SIDE instead of REAR,
    // must still fail even though its channel count matches the header.
    let wrong_surround = Position::FRONT_LEFT
        | Position::FRONT_RIGHT
        | Position::FRONT_CENTER
        | Position::LFE1
        | Position::SIDE_LEFT
        | Position::SIDE_RIGHT;
    let params = params(6, 1, wrong_surround.into());
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err());
}

#[test]
fn mono_alias_does_not_bypass_unsupported_mapping_family() {
    let params = params(1, 255, Position::FRONT_LEFT.into());
    assert!(OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).is_err());
}
