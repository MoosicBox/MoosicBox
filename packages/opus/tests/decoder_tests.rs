#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::OpusDecoder;
use symphonia::core::codecs::registry::RegisterableAudioDecoder;
use symphonia::core::{
    audio::Position,
    codecs::audio::{
        AudioCodecParameters, AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_OPUS,
    },
};

#[test]
fn test_decoder_creation_mono() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48000)
        .with_channels((Position::FRONT_LEFT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_creation_stereo() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48000)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_creation_8khz() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(8000)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_creation_16khz() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(16000)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_creation_24khz() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(24000)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_creation_unsupported_sample_rate() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(22050)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    assert!(decoder.is_err());
}

#[test]
fn test_decoder_supported_codecs() {
    let codecs = OpusDecoder::supported_codecs();
    assert_eq!(codecs.len(), 1);
    assert_eq!(codecs[0].id, CODEC_ID_OPUS);
}

#[test]
fn test_decoder_codec_params() {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48000)
        .with_channels((Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

    let decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default()).unwrap();
    let retrieved_params = decoder.codec_params();

    assert_eq!(retrieved_params.sample_rate, Some(48000));
}
