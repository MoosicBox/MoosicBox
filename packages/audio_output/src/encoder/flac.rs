//! FLAC audio encoder implementation.

#![allow(clippy::module_name_repetitions)]

use std::sync::RwLock;

use bytes::Bytes;
use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
};
use moosicbox_audio_encoder::flac::{Encoder, encoder_flac};
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use switchy_async::task::JoinHandle;
use symphonia::core::{
    audio::{Audio, AudioBuffer, AudioSpec},
    formats::Track,
    packet::Packet,
    units::Duration,
};

use crate::{AudioOutputError, AudioWrite, to_samples};
use moosicbox_resampler::Resampler;

use super::AudioEncoder;

/// FLAC audio encoder that converts decoded audio to FLAC format.
///
/// This encoder uses the FLAC library to encode audio samples
/// and supports automatic resampling to a target sample rate.
pub struct FlacEncoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    input_rate: Option<u32>,
    finished: bool,
    resample_rate: Option<u32>,
    output_rate: usize,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: Encoder,
}

impl FlacEncoder {
    /// Creates a new FLAC encoder with default settings.
    ///
    /// Default output sample rate is 44100 Hz.
    ///
    /// # Panics
    ///
    /// * If fails to get the flac encoder
    #[must_use]
    pub fn new() -> Self {
        Self {
            resampler: None,
            input_rate: None,
            finished: false,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: None,
            encoder: encoder_flac().expect("Failed to create Flac encoder"),
        }
    }

    /// Creates a new FLAC encoder with a custom writer.
    ///
    /// # Arguments
    /// * `writer` - Output writer for encoded FLAC data
    ///
    /// # Panics
    ///
    /// * If fails to get the flac encoder
    #[must_use]
    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: None,
            input_rate: None,
            finished: false,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_flac().expect("Failed to create Flac encoder"),
        }
    }

    /// Initializes the resampler if needed based on input audio spec.
    ///
    /// # Arguments
    /// * `spec` - Input audio signal specification
    /// * `duration` - Audio duration in samples
    pub fn init_resampler(&mut self, spec: &AudioSpec, duration: Duration) -> &Self {
        self.input_rate.replace(spec.rate());
        self.duration.replace(duration);

        if self.resample_rate.is_none_or(|r| r != spec.rate())
            && self.output_rate != spec.rate() as usize
        {
            log::debug!(
                "Initializing resampler with rate={} duration={}",
                spec.rate(),
                duration,
            );
            self.resample_rate.replace(spec.rate());
            self.resampler.replace(RwLock::new(Resampler::new(
                spec.clone(),
                self.output_rate,
                duration.get(),
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
    pub fn open(mut self, spec: impl std::borrow::Borrow<AudioSpec>, duration: Duration) -> Self {
        self.init_resampler(spec.borrow(), duration);
        self
    }

    fn encode_output(&mut self, buf: &[i16]) -> Bytes {
        if buf.is_empty() {
            return Bytes::new();
        }
        // Encode one complete FLAC stream per input batch, not concatenated
        // independent streams for each arbitrary 1024-sample slice.
        let input: Vec<_> = buf.iter().map(|&sample| i32::from(sample)).collect();
        let mut output = vec![0; buf.len() * 4 + 4096];
        let info =
            moosicbox_audio_encoder::flac::encode_flac(&mut self.encoder, &input, &mut output)
                .expect("Failed to encode FLAC");
        output.truncate(info.output_size);
        // flacenc 0.5.1 includes the short final block in STREAMINFO's minimum
        // block size even though it emits fixed-block frame numbers. FLAC
        // excludes that last block from this minimum. A mismatched min/max
        // advertises variable blocking, so strict readers reject every frame.
        // The audio_encoder helper uses a fixed 512-frame block size.
        assert_eq!(&output[..8], b"fLaC\x80\x00\x00\x22");
        assert_eq!(&output[10..12], &512_u16.to_be_bytes());
        output[8..10].copy_from_slice(&512_u16.to_be_bytes());
        output.into()
    }

    fn resample_if_needed(
        &mut self,
        decoded: &AudioBuffer<f32>,
    ) -> Result<Vec<i16>, AudioOutputError> {
        if self.finished {
            return Err(AudioOutputError::StreamEnd);
        }
        let spec = decoded.spec();
        if spec.channels() != self.spec().channels() {
            return Err(AudioOutputError::UnsupportedChannels(
                spec.channels().count(),
            ));
        }
        let duration = Duration::from(decoded.capacity() as u64);

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
                .map_or_else(Vec::new, <[_]>::to_vec))
        } else {
            log::debug!(
                "Passing through audio frames={} duration={duration} rate={} channels={:?} channels_count={}",
                decoded.frames(),
                spec.rate(),
                spec.channels(),
                spec.channels().count(),
            );
            Ok(to_samples(decoded))
        }
    }
}

impl Default for FlacEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEncoder for FlacEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("FlacEncoder encode {} frames", decoded.frames());

        let decoded = self.resample_if_needed(&decoded)?;

        if decoded.is_empty() {
            return Ok(Bytes::new());
        }
        Ok(self.encode_output(&decoded))
    }

    fn finish(&mut self) -> Result<Bytes, AudioOutputError> {
        if self.finished {
            return Ok(Bytes::new());
        }
        self.finished = true;
        let mut tail = Vec::new();
        if let Some(resampler) = &self.resampler {
            let mut resampler = resampler.write().map_err(|_| AudioOutputError::Interrupt)?;
            while let Some(samples) = resampler.flush() {
                tail.extend_from_slice(samples);
            }
        }
        if tail.is_empty() {
            return Ok(Bytes::new());
        }
        Ok(self.encode_output(&tail))
    }

    fn spec(&self) -> AudioSpec {
        AudioSpec::new(
            u32::try_from(self.output_rate).unwrap(),
            symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
        )
    }
}

impl AudioDecode for FlacEncoder {
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
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        AudioWrite::flush(self).map_err(|error| AudioDecodeError::Other(Box::new(error)))
    }
}

impl AudioWrite for FlacEncoder {
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
        let bytes = self.finish()?;
        if let Some(writer) = &mut self.writer {
            writer.write_all(&bytes)?;
            writer.flush()?;
        }
        Ok(())
    }

    fn handle(&self) -> crate::AudioHandle {
        unimplemented!("FlacEncoder does not support command handling")
    }
}

/// Encodes an audio file to FLAC format and returns a byte stream.
///
/// This function spawns a background task to encode the audio file
/// and returns a stream that can be read as the encoding progresses.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
#[must_use]
pub fn encode_flac_stream(path: &str) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_flac_spawn(path, writer);

    stream
}

/// Spawns a background task to encode an audio file to FLAC format.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded FLAC data
pub fn encode_flac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: &str,
    writer: T,
) -> JoinHandle<()> {
    let path = path.to_string();
    switchy_async::runtime::Handle::current().spawn_blocking_with_name(
        "audio_decoder: encode_flac",
        move || {
            encode_flac(&path, writer);
        },
    )
}

/// Encodes an audio file to FLAC format.
///
/// This function blocks until encoding is complete.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded FLAC data
pub fn encode_flac<T: std::io::Write + Send + Sync + Clone + 'static>(path: &str, writer: T) {
    let mut audio_decode_handler =
        AudioDecodeHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                FlacEncoder::with_writer(writer.clone()).open(&spec, duration),
            ))
        }));

    if let Err(err) = decode_file_path_str(path, &mut audio_decode_handler, true, true, None, None)
    {
        log::error!("Failed to encode to flac: {err:?}");
    }
}
