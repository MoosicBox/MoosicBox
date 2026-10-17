#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![cfg(any(feature = "libopus", feature = "native"))]

use moosicbox_opus::{OpusBackend, OpusDecoder};
use symphonia::core::{audio::Position, codecs::audio::AudioCodecParameters};
#[cfg(feature = "native")]
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoder,
};

fn params(gain: i16) -> AudioCodecParameters {
    let mut header = b"OpusHead\x01\x01\0\0".to_vec();
    header.extend_from_slice(&48000_u32.to_le_bytes());
    header.extend_from_slice(&gain.to_le_bytes());
    header.push(0);
    let mut params = AudioCodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Position::FRONT_LEFT.into())
        .with_extra_data(header.into_boxed_slice());
    params
}

#[test]
fn identification_header_validation() {
    let backend = if cfg!(feature = "libopus") {
        OpusBackend::Libopus
    } else {
        OpusBackend::Native
    };
    for (index, value) in [(8, 16), (9, 2), (18, 1)] {
        let mut params = params(0);
        params.extra_data.as_mut().unwrap()[index] = value;
        assert!(OpusDecoder::try_new_with_backend(&params, backend).is_err());
    }
    let mut truncated = params(0);
    truncated.extra_data = Some(Box::from(&b"OpusHead"[..]));
    assert!(OpusDecoder::try_new_with_backend(&truncated, backend).is_err());
}

#[cfg(feature = "native")]
#[test]
fn native_gain_is_applied_after_integer_decode_and_survives_reset() {
    use moosicbox_opus_native::{Channels as NativeChannels, Decoder as NativeDecoder, SampleRate};
    // Valid CELT silence packet: useful for checking shared conversion/gain and
    // reset plumbing, but not a substitute for non-silent native PCM parity.
    let bytes = &[0xf8, 0xff, 0xfe];
    let mut direct = NativeDecoder::new(SampleRate::Hz48000, NativeChannels::Mono).unwrap();
    let mut pcm = vec![0; 5760];
    let count = direct.decode(Some(bytes), &mut pcm, false).unwrap();
    assert_eq!(count, 960);
    for gain in [-1536, 0, 1536] {
        let mut adapter =
            OpusDecoder::try_new_with_backend(&params(gain), OpusBackend::Native).unwrap();
        let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
        for _ in 0..2 {
            let packet = make_packet(0, 0, 960, bytes);
            let GenericAudioBufferRef::F32(output) = adapter.decode(&packet).unwrap() else {
                panic!("expected f32")
            };
            assert_eq!(output.frames(), count);
            assert!(
                output
                    .plane(0)
                    .unwrap()
                    .iter()
                    .zip(&pcm[..count])
                    .all(|(actual, expected)| *actual == (f32::from(*expected) / 32768.0) * factor)
            );
            adapter.reset();
        }
    }
}

#[cfg(feature = "native")]
fn make_packet(track: u32, ts: u64, dur: u64, data: &[u8]) -> symphonia::core::packet::Packet {
    symphonia::core::packet::Packet::new(
        track,
        symphonia::core::units::Timestamp::from(ts as i64),
        dur.into(),
        data,
    )
}
