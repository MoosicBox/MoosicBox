//! MP3 audio encoder implementation.

#![allow(clippy::module_name_repetitions)]

use std::sync::RwLock;

use bytes::Bytes;
use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
};
use moosicbox_audio_encoder::mp3::encoder_mp3;
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

/// MP3 audio encoder that converts decoded audio to MP3 format.
///
/// This encoder uses the LAME library to encode audio samples
/// and supports automatic resampling to a target sample rate.
pub struct Mp3Encoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    input_rate: Option<u32>,
    finished: bool,
    resample_rate: Option<u32>,
    output_rate: usize,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: mp3lame_encoder::Encoder,
}

impl Mp3Encoder {
    /// Creates a new MP3 encoder with default settings.
    ///
    /// Default output sample rate is 44100 Hz.
    ///
    /// # Panics
    ///
    /// * If fails to get the mp3 encoder
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
            encoder: encoder_mp3().unwrap(),
        }
    }

    /// Creates a new MP3 encoder with a custom writer.
    ///
    /// # Arguments
    /// * `writer` - Output writer for encoded MP3 data
    ///
    /// # Panics
    ///
    /// * If fails to get the mp3 encoder
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
            encoder: encoder_mp3().unwrap(),
        }
    }

    /// Initializes the resampler if needed based on input audio spec.
    ///
    /// # Arguments
    /// * `spec` - Input audio signal specification
    /// * `duration` - Audio duration in samples
    pub fn init_resampler(&mut self, spec: &AudioSpec, duration: Duration) -> &Self {
        if self.resample_rate.is_none_or(|r| r != spec.rate())
            && self.output_rate != spec.rate() as usize
        {
            log::debug!(
                "Initializing resampler with rate={} duration={}",
                spec.rate(),
                duration,
            );
            self.input_rate.replace(spec.rate());
            self.duration.replace(duration);
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
        let mut output = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(buf.len()));
        self.encoder
            .encode_to_vec(mp3lame_encoder::InterleavedPcm(buf), &mut output)
            .expect("Failed to encode MP3");
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

            Ok(resampler
                .write()
                .unwrap()
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

impl Default for Mp3Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEncoder for Mp3Encoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("Mp3Encoder encode {} frames", decoded.frames());

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
        let mut output = self.encode_output(&tail).to_vec();
        // Normal EOF must emit the encoder delay and final partial frame. No-gap
        // flushing is only for consecutive tracks in a continuous encoded stream.
        output.reserve(7200);
        self.encoder
            .flush_to_vec::<mp3lame_encoder::FlushGap>(&mut output)
            .map_err(|error| AudioOutputError::IO(std::io::Error::other(format!("{error:?}"))))?;
        Ok(output.into())
    }

    fn spec(&self) -> AudioSpec {
        AudioSpec::new(
            u32::try_from(self.output_rate).unwrap(),
            symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
        )
    }
}

impl AudioDecode for Mp3Encoder {
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

impl AudioWrite for Mp3Encoder {
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
        unimplemented!("Mp3Encoder does not support command handling")
    }
}

/// Encodes an audio file to MP3 format and returns a byte stream.
///
/// This function spawns a background task to encode the audio file
/// and returns a stream that can be read as the encoding progresses.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
#[must_use]
pub fn encode_mp3_stream(path: &str) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_mp3_spawn(path, writer);

    stream
}

/// Spawns a background task to encode an audio file to MP3 format.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded MP3 data
pub fn encode_mp3_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: &str,
    writer: T,
) -> JoinHandle<()> {
    let path = path.to_string();
    switchy_async::runtime::Handle::current().spawn_blocking_with_name(
        "audio_output: encode_mp3",
        move || {
            encode_mp3(&path, writer);
        },
    )
}

/// Encodes an audio file to MP3 format.
///
/// This function blocks until encoding is complete.
///
/// # Arguments
/// * `path` - Path to the audio file to encode
/// * `writer` - Output writer for encoded MP3 data
pub fn encode_mp3<T: std::io::Write + Send + Sync + Clone + 'static>(path: &str, writer: T) {
    let mut audio_decode_handler =
        AudioDecodeHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                Mp3Encoder::with_writer(writer.clone()).open(&spec, duration),
            ))
        }));

    if let Err(err) = decode_file_path_str(path, &mut audio_decode_handler, true, true, None, None)
    {
        log::error!("Failed to encode to mp3: {err:?}");
    }
}
