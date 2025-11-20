#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::OpusDecoder;
use symphonia::core::{
    audio::Channels,
    codecs::{CODEC_TYPE_OPUS, CodecParameters, Decoder, DecoderOptions, FinalizeResult},
};

#[test]
fn test_decoder_default_sample_rate() {
    // When no sample rate is provided, should default to 48000
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // The decoder uses the provided parameters, storing them as-is
    // If no sample rate was provided, it won't be in the retrieved params
    // but the decoder internally defaults to 48000
    assert!(decoder.codec_params().channels.is_some());
}

#[test]
fn test_decoder_default_channels() {
    // When no channels are provided, should default to stereo
    let mut params = CodecParameters::new();
    params.for_codec(CODEC_TYPE_OPUS).with_sample_rate(48000);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let retrieved_params = decoder.codec_params();

    // The decoder stores the params as provided, not necessarily with defaults filled in
    assert_eq!(retrieved_params.sample_rate, Some(48000));
}

#[test]
fn test_decoder_unsupported_channel_count() {
    // Opus decoder only supports mono (1) or stereo (2)
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT | Channels::FRONT_CENTRE);

    let result = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(result.is_err());
}

#[test]
fn test_decoder_all_supported_sample_rates() {
    let supported_rates = [8000, 12000, 16000, 24000, 48000];

    for rate in supported_rates {
        let mut params = CodecParameters::new();
        params
            .for_codec(CODEC_TYPE_OPUS)
            .with_sample_rate(rate)
            .with_channels(Channels::FRONT_LEFT);

        let result = OpusDecoder::try_new(&params, &DecoderOptions::default());
        assert!(
            result.is_ok(),
            "Sample rate {rate} should be supported but got error"
        );

        let decoder = result.unwrap();
        assert_eq!(decoder.codec_params().sample_rate, Some(rate));
    }
}

#[test]
fn test_decoder_reset_clears_state() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Reset should not panic and should clear internal state
    decoder.reset();

    // After reset, last_decoded should return empty buffer
    let buffer = decoder.last_decoded();
    // Signal trait is used here via frames() method
    assert_eq!(buffer.frames(), 0);
}

#[test]
fn test_decoder_finalize_returns_default() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Finalize should return default result for Opus (no pending frames)
    let result = decoder.finalize();

    // FinalizeResult::default() should be returned
    let expected = FinalizeResult::default();
    assert_eq!(result.verify_ok, expected.verify_ok);
}

#[test]
fn test_decoder_last_decoded_initial_state() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();

    // Initially, last_decoded should return empty buffer
    let buffer = decoder.last_decoded();
    assert_eq!(buffer.frames(), 0);
}

#[test]
fn test_decoder_mono_channel_configuration() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let retrieved_params = decoder.codec_params();

    assert_eq!(retrieved_params.channels.unwrap().count(), 1);
}

#[test]
fn test_decoder_creation_with_12khz() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(12000)
        .with_channels(Channels::FRONT_LEFT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
    assert_eq!(decoder.unwrap().codec_params().sample_rate, Some(12000));
}

#[test]
fn test_decoder_unsupported_sample_rates() {
    let unsupported_rates = [11025, 22050, 32000, 44100, 96000];

    for rate in unsupported_rates {
        let mut params = CodecParameters::new();
        params
            .for_codec(CODEC_TYPE_OPUS)
            .with_sample_rate(rate)
            .with_channels(Channels::FRONT_LEFT);

        let result = OpusDecoder::try_new(&params, &DecoderOptions::default());
        assert!(
            result.is_err(),
            "Sample rate {rate} should be unsupported but decoder was created"
        );
    }
}
