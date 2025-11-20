#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::OpusDecoder;
use symphonia::core::{
    audio::Channels,
    codecs::{CodecParameters, Decoder, DecoderOptions},
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
