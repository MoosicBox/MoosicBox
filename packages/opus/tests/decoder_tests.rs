#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::OpusDecoder;
use symphonia::core::{
    audio::Channels,
    codecs::{CODEC_TYPE_OPUS, CodecParameters, Decoder, DecoderOptions},
};

#[test_log::test]
fn test_decoder_creation_mono() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_creation_stereo() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_creation_8khz() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(8000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_creation_16khz() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(16000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_creation_24khz() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(24000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test_log::test]
fn test_decoder_creation_unsupported_sample_rate() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(22050)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default());
    assert!(decoder.is_err());
}

#[test_log::test]
fn test_decoder_supported_codecs() {
    let codecs = OpusDecoder::supported_codecs();
    assert_eq!(codecs.len(), 1);
    assert_eq!(codecs[0].codec, CODEC_TYPE_OPUS);
}

#[test_log::test]
fn test_decoder_codec_params() {
    let mut params = CodecParameters::new();
    params
        .for_codec(CODEC_TYPE_OPUS)
        .with_sample_rate(48000)
        .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

    let decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let retrieved_params = decoder.codec_params();

    assert_eq!(retrieved_params.sample_rate, Some(48000));
}
