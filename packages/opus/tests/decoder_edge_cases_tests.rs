#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::OpusDecoder;
use symphonia::core::{
    audio::Channels,
    codecs::{CodecParameters, Decoder, DecoderOptions},
    formats::Packet,
};

#[test_log::test]
fn test_decoder_creation_unsupported_channel_count() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT | Channels::FRONT_CENTRE);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_err());
}

#[test_log::test]
fn test_decoder_creation_default_sample_rate() {
    let mut params = CodecParameters::new();
    params.with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    // No sample rate specified - should default to 48000

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
    // Decoder should be created successfully with default sample rate
}

#[test_log::test]
fn test_decoder_creation_default_channels() {
    let mut params = CodecParameters::new();
    params.with_sample_rate(48000);
    // No channels specified - should default to stereo

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
    // Decoder should be created successfully with default channels
}

#[test_log::test]
fn test_decoder_creation_12khz() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(12000)
        .with_channels(Channels::FRONT_LEFT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_finalize_returns_default() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Finalize should return default/no-op result for Opus
    let result = decoder.finalize();
    assert!(result.verify_ok.is_none());
}

#[test_log::test]
fn test_decoder_last_decoded_empty() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Before any decoding, last_decoded should return empty buffer
    let audio_buf = decoder.last_decoded();
    assert_eq!(audio_buf.frames(), 0);
}

#[test_log::test]
fn test_decoder_reset() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Reset should clear internal state without errors
    decoder.reset();

    // After reset, last_decoded should still work
    let audio_buf = decoder.last_decoded();
    assert_eq!(audio_buf.frames(), 0);
}

#[test_log::test]
fn test_decode_empty_packet_fails() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Empty packet data should fail parsing and return DecodeError
    let packet = Packet::new_from_slice(0, 0, 0, &[]);
    let result = decoder.decode(&packet);
    assert!(result.is_err());
}

#[test_log::test]
fn test_decode_invalid_code1_packet_fails() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Code 1 packet with odd length payload should fail (two equal frames can't split odd bytes)
    // TOC byte 0x01 = code 1, followed by only 1 data byte
    let packet = Packet::new_from_slice(0, 0, 0, &[0x01, 0xAA]);
    let result = decoder.decode(&packet);
    assert!(result.is_err());
}

#[test_log::test]
fn test_decode_invalid_code3_zero_frames_fails() {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Code 3 packet with frame_count = 0 is invalid
    // TOC byte 0x03 = code 3, followed by header byte 0x00 = 0 frames
    let packet = Packet::new_from_slice(0, 0, 0, &[0x03, 0x00]);
    let result = decoder.decode(&packet);
    assert!(result.is_err());
}
