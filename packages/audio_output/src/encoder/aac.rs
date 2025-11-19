//! AAC audio encoder implementation.

#![allow(clippy::module_name_repetitions)]

use std::sync::RwLock;

use bytes::Bytes;
use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
};
use moosicbox_audio_encoder::aac::encoder_aac;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use switchy_async::task::JoinHandle;
use symphonia::core::{
    audio::{AudioBuffer, Channels, Signal, SignalSpec},
    formats::{Packet, Track},
    units::Duration,
};

use crate::{AudioOutputError, AudioWrite, to_samples};
use moosicbox_resampler::Resampler;

use super::AudioEncoder;

/// AAC audio encoder that converts decoded audio to AAC format.
///
/// This encoder uses the FDK-AAC library to encode audio samples
/// and supports automatic resampling to a target sample rate.
pub struct AacEncoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    input_rate: Option<u32>,
    resample_rate: Option<u32>,
    output_rate: usize,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: fdk_aac::enc::Encoder,
}

impl AacEncoder {
    /// Creates a new AAC encoder with default settings.
    ///
    /// Default output sample rate is 44100 Hz.
    ///
    /// # Panics
    ///
    /// * If fails to get the aac encoder
    #[must_use]
    pub fn new() -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: None,
            encoder: encoder_aac().unwrap(),
        }
    }

    /// Creates a new AAC encoder with a custom writer.
    ///
    /// # Arguments
    /// * `writer` - Output writer for encoded AAC data
    ///
    /// # Panics
    ///
    /// * If fails to get the aac encoder
    #[must_use]
    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_aac().unwrap(),
        }
    }

    /// Initializes the resampler if needed based on input audio spec.
    ///
    /// # Arguments
    /// * `spec` - Input audio signal specification
    /// * `duration` - Audio duration in samples
    pub fn init_resampler(&mut self, spec: &SignalSpec, duration: Duration) -> &Self {
        self.input_rate.replace(spec.rate);
        self.duration.replace(duration);

        if self.resample_rate.is_none_or(|r| r != spec.rate)
            && self.output_rate != spec.rate as usize
        {
            log::debug!(
                "Initializing resampler with rate={} duration={}",
                spec.rate,
                duration,
            );
            self.resample_rate.replace(spec.rate);
            self.resampler.replace(RwLock::new(Resampler::new(
                *spec,
                self.output_rate,
                duration,
            )));
        }
        self
    }

    /// Opens the encoder with the specified audio specification.
    ///
    /// This initializes the resampler and prepares the encoder for encoding.
    ///
    /// # Arguments
    /// * `spec` - Audio signal specification
    /// * `duration` - Audio duration in samples
    #[must_use]
    pub fn open(mut self, spec: SignalSpec, duration: Duration) -> Self {
        self.init_resampler(&spec, duration);
        self
    }

    fn encode_output(&self, buf: &[i16]) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            let mut output = [0u8; 2048];
            match moosicbox_audio_encoder::aac::encode_aac(
                &self.encoder,
                &buf[read..end],
                &mut output,
            ) {
                Ok(info) => {
                    written.extend_from_slice(&output[..info.output_size]);
                    read += info.input_consumed;

                    if read >= buf.len() {
                        break;
                    }
                }
                Err(err) => {
                    panic!("Failed to convert: {err:?}");
                }
            }
        }
        written.into()
    }

    fn resample_if_needed(
        &mut self,
        decoded: &AudioBuffer<f32>,
    ) -> Result<Vec<i16>, AudioOutputError> {
        let spec = decoded.spec();
        let duration = decoded.capacity() as u64;

        self.init_resampler(spec, duration);

        if let Some(resampler) = &self.resampler {
            log::debug!(
                "Resampling input_rate={:?} output_rate={} duration={:?}",
                self.input_rate,
                self.output_rate,
                self.duration
            );

            let mut resampler = resampler.write().unwrap();

            Ok(resampler
                .resample(decoded)
                .ok_or(AudioOutputError::StreamEnd)?
                .to_vec())
        } else {
            log::debug!(
                "Passing through audio frames={} duration={duration} rate={} channels={} channels_count={}",
                decoded.frames(),
                spec.rate,
                spec.channels,
                spec.channels.count(),
            );
            Ok(to_samples(decoded))
        }
    }
}

impl Default for AacEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEncoder for AacEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("AacEncoder encode {} frames", decoded.frames());

        let decoded = self.resample_if_needed(&decoded)?;

        Ok(self.encode_output(&decoded))
    }

    fn spec(&self) -> SignalSpec {
        SignalSpec {
            rate: u32::try_from(self.output_rate).unwrap(),
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        }
    }
}

impl AudioDecode for AacEncoder {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        if self.writer.is_none() {
            return Ok(());
        }

        let bytes = self.encode(decoded).map_err(|e| {
            AudioDecodeError::IO(std::io::Error::other(format!("Failed to encode: {e:?}")))
        })?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += match writer.write(&bytes[count..]) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        log::error!("Failed to write: {e:?}");
                        return Err(AudioDecodeError::StreamClosed);
                    }
                };
                if count >= bytes.len() {
                    break;
                }
            }
        }

        Ok(())
    }
}

impl AudioWrite for AacEncoder {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        if self.writer.is_none() {
            return Ok(0);
        }

        let bytes = self.encode(decoded)?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += match writer.write(&bytes[count..]) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        log::error!("Failed to write: {e:?}");
                        return Err(AudioOutputError::StreamClosed);
                    }
                };
                if count >= bytes.len() {
                    break;
                }
            }
        }

        Ok(bytes.len())
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        Ok(())
    }

    fn handle(&self) -> crate::AudioHandle {
        unimplemented!("AacEncoder does not support command handling")
    }
}

/// Encodes an audio file to AAC format and returns a byte stream.
///
/// This function spawns a background task to encode the audio file
/// and returns a stream that can be read as the encoding progresses.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
#[must_use]
pub fn encode_aac_stream(path: &str) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_aac_spawn(path, writer);

    stream
}

/// Spawns a background task to encode an audio file to AAC format.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded AAC data
pub fn encode_aac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: &str,
    writer: T,
) -> JoinHandle<()> {
    let path = path.to_string();
    switchy_async::runtime::Handle::current().spawn_blocking_with_name(
        "audio_decoder: encode_aac",
        move || {
            encode_aac(&path, writer);
        },
    )
}

/// Encodes an audio file to AAC format.
///
/// This function blocks until encoding is complete.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded AAC data
pub fn encode_aac<T: std::io::Write + Send + Sync + Clone + 'static>(path: &str, writer: T) {
    let mut audio_decode_handler =
        AudioDecodeHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                AacEncoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = decode_file_path_str(path, &mut audio_decode_handler, true, true, None, None)
    {
        log::error!("Failed to encode to aac: {err:?}");
    }
}
