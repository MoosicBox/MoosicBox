//! Explicit decode implementation selection, shared by synchronized and channel APIs.
use crate::DecodeError;
use symphonia::core::{
    codecs::{
        CodecParameters,
        audio::{AudioDecoder, AudioDecoderOptions},
        registry::CodecRegistry,
    },
    formats::Track,
};

/// The requested Opus implementation. Selection never falls back to another backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpusDecodeBackend {
    /// Independent C libopus implementation (the compatibility default).
    #[default]
    Libopus,
    /// Native Rust integer decoder.
    Native,
}

/// Codec options and explicit Opus implementation selection.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeOptions {
    /// Symphonia gapless and verification behavior.
    pub decoder: AudioDecoderOptions,
    /// Requested Opus backend; unavailable implementations return an error.
    pub opus_backend: OpusDecodeBackend,
}

pub fn make_decoder(
    track: &Track,
    options: DecodeOptions,
) -> Result<Box<dyn AudioDecoder>, DecodeError> {
    let params = track
        .codec_params
        .as_ref()
        .and_then(CodecParameters::audio)
        .ok_or(DecodeError::InvalidSource)?;
    #[cfg(feature = "opus-base")]
    if params.codec == symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS {
        return Ok(Box::new(make_opus_decoder(params, options)?));
    }
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    Ok(registry.make_audio_decoder(params, &options.decoder)?)
}

#[cfg(feature = "opus-base")]
fn make_opus_decoder(
    params: &symphonia::core::codecs::audio::AudioCodecParameters,
    options: DecodeOptions,
) -> Result<moosicbox_opus::OpusDecoder, DecodeError> {
    let backend = match options.opus_backend {
        OpusDecodeBackend::Libopus => moosicbox_opus::OpusBackend::Libopus,
        OpusDecodeBackend::Native => moosicbox_opus::OpusBackend::Native,
    };
    Ok(moosicbox_opus::OpusDecoder::try_new_with_backend_options(
        params,
        backend,
        &options.decoder,
    )?)
}

#[cfg(all(test, feature = "opus-native", feature = "opus-libopus"))]
mod tests {
    use super::*;
    #[test]
    fn product_factory_retains_explicit_backend_identity() {
        let mut params = symphonia::core::codecs::audio::AudioCodecParameters::new();
        params.codec = symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
        params.sample_rate = Some(48000);
        params.channels = Some(symphonia::core::audio::Position::FRONT_LEFT.into());
        for (requested, expected) in [
            (
                OpusDecodeBackend::Native,
                moosicbox_opus::OpusBackend::Native,
            ),
            (
                OpusDecodeBackend::Libopus,
                moosicbox_opus::OpusBackend::Libopus,
            ),
        ] {
            let options = DecodeOptions {
                opus_backend: requested,
                ..DecodeOptions::default()
            };
            assert_eq!(
                make_opus_decoder(&params, options).unwrap().backend(),
                expected
            );
        }
    }
}
