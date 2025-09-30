use log::{debug, warn};
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Signal, SignalSpec},
    codecs::{
        CODEC_TYPE_OPUS, CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult,
    },
    errors::{Error, Result},
    formats::Packet,
    support_codec,
};

use crate::packet::OpusPacket;

/// Opus decoder implementation.
pub struct OpusDecoder {
    params: CodecParameters,
    output_buf: AudioBuffer<f32>,
}

impl Decoder for OpusDecoder {
    /// Create a new Opus decoder.
    ///
    /// # Errors
    ///
    /// Returns an error if the codec parameters are invalid or missing required fields.
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self> {
        debug!("Initializing Opus decoder");

        let sample_rate = params.sample_rate.unwrap_or(48000);
        let channels = params.channels.unwrap_or(
            symphonia::core::audio::Channels::FRONT_LEFT
                | symphonia::core::audio::Channels::FRONT_RIGHT,
        );

        let spec = SignalSpec::new(sample_rate, channels);
        let output_buf = AudioBuffer::new(960, spec);

        Ok(Self {
            params: params.clone(),
            output_buf,
        })
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(
            CODEC_TYPE_OPUS,
            "opus",
            "Opus Interactive Audio Codec"
        )]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        let opus_packet = OpusPacket::parse(&packet.data)
            .map_err(|_| Error::DecodeError("invalid opus packet"))?;

        debug!("Decoded packet with {} frames", opus_packet.frames.len());

        self.output_buf.clear();

        warn!("Opus decoding not yet implemented, returning silence");

        Ok(self.output_buf.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.output_buf.as_audio_buffer_ref()
    }

    fn reset(&mut self) {
        debug!("Resetting Opus decoder");
        self.output_buf.clear();
    }
}
