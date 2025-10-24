use std::sync::Mutex;

use audiopus::{
    Channels, SampleRate,
    coder::{Decoder as OpusLibDecoder, GenericCtl},
};
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

/// Opus audio decoder for Symphonia.
///
/// Implements the Symphonia [`Decoder`] trait to provide RFC 6716 compliant
/// Opus decoding using libopus. Supports mono and stereo playback at sample
/// rates of 8, 12, 16, 24, and 48 kHz.
///
/// Create instances using [`Decoder::try_new`] with appropriate [`CodecParameters`],
/// or register with a codec registry using [`register_opus_codec`](crate::register_opus_codec).
pub struct OpusDecoder {
    params: CodecParameters,
    decoder: Mutex<OpusLibDecoder>,
    output_buf: AudioBuffer<f32>,
    temp_decode_buf: Vec<i16>,
    channel_count: usize,
    frame_size_samples: usize,
}

impl Decoder for OpusDecoder {
    /// Create a new Opus decoder.
    ///
    /// # Errors
    ///
    /// Returns an error if the codec parameters are invalid or missing required fields.
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self> {
        debug!("Initializing Opus decoder with libopus");

        let sample_rate = params.sample_rate.unwrap_or(48000);
        let channels = params.channels.unwrap_or(
            symphonia::core::audio::Channels::FRONT_LEFT
                | symphonia::core::audio::Channels::FRONT_RIGHT,
        );
        let channel_count = channels.count();

        let sample_rate_enum = match sample_rate {
            8000 => SampleRate::Hz8000,
            12000 => SampleRate::Hz12000,
            16000 => SampleRate::Hz16000,
            24000 => SampleRate::Hz24000,
            48000 => SampleRate::Hz48000,
            _ => return Err(Error::Unsupported("unsupported sample rate")),
        };

        let channels_enum = match channel_count {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => return Err(Error::Unsupported("unsupported channel count")),
        };

        let decoder = OpusLibDecoder::new(sample_rate_enum, channels_enum)
            .map_err(|_| Error::DecodeError("failed to create opus decoder"))?;

        let frame_size_samples = 960;
        let spec = SignalSpec::new(sample_rate, channels);
        let output_buf = AudioBuffer::new(frame_size_samples as u64, spec);
        let temp_decode_buf = vec![0i16; frame_size_samples * channel_count];

        Ok(Self {
            params: params.clone(),
            decoder: Mutex::new(decoder),
            output_buf,
            temp_decode_buf,
            channel_count,
            frame_size_samples,
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
        self.output_buf.clear();

        let opus_packet = OpusPacket::parse(&packet.data)
            .map_err(|_| Error::DecodeError("invalid opus packet"))?;

        debug!("Decoding {} frames", opus_packet.frames.len());

        let mut output_offset = 0;
        for frame in &opus_packet.frames {
            if frame.is_dtx {
                debug!("DTX frame, generating silence");
                continue;
            }

            let required_size = self.frame_size_samples * self.channel_count;
            if self.temp_decode_buf.len() < required_size {
                self.temp_decode_buf.resize(required_size, 0);
            }

            let decoded_samples = self
                .decoder
                .lock()
                .unwrap()
                .decode(
                    Some(&frame.data),
                    &mut self.temp_decode_buf[..required_size],
                    false,
                )
                .map_err(|_| Error::DecodeError("opus decode failed"))?;

            if self.channel_count == 1 {
                let output = self.output_buf.chan_mut(0);
                for i in 0..decoded_samples {
                    output[output_offset + i] = f32::from(self.temp_decode_buf[i]) / 32768.0;
                }
            } else {
                for i in 0..decoded_samples {
                    for ch in 0..self.channel_count {
                        let sample = self.temp_decode_buf[i * self.channel_count + ch];
                        let normalized = f32::from(sample) / 32768.0;
                        self.output_buf.chan_mut(ch)[i + output_offset] = normalized;
                    }
                }
            }
            output_offset += decoded_samples;
        }

        self.output_buf.truncate(output_offset);
        Ok(self.output_buf.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.output_buf.as_audio_buffer_ref()
    }

    fn reset(&mut self) {
        debug!("Resetting Opus decoder state");
        let result = self.decoder.lock().unwrap().reset_state();
        if let Err(e) = result {
            warn!("Failed to reset decoder state: {e}");
        }
        self.output_buf.clear();
    }
}
