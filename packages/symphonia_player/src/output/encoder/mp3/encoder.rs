use std::sync::{Arc, RwLock};

use crate::output::{to_samples, AudioEncoder, AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use lazy_static::lazy_static;
use moosicbox_converter::mp3::encoder_mp3;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::*;
use symphonia::core::units::Duration;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct Mp3Encoder {
    resampler: Arc<RwLock<Option<Resampler<i16>>>>,
    rate: Option<u32>,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: mp3lame_encoder::Encoder,
}

impl Mp3Encoder {
    pub fn new() -> Self {
        Self {
            resampler: Arc::new(RwLock::new(None)),
            rate: None,
            duration: None,
            writer: None,
            encoder: encoder_mp3().unwrap(),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: Arc::new(RwLock::new(None)),
            rate: None,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_mp3().unwrap(),
        }
    }

    pub fn init_resampler(&mut self, spec: &SignalSpec, duration: Duration) -> &Self {
        if !self.rate.is_some_and(|r| r == spec.rate)
            || !self.duration.is_some_and(|d| d == duration)
        {
            log::debug!(
                "Initializing resampler with rate={} duration={}",
                spec.rate,
                duration
            );
            self.rate.replace(spec.rate);
            self.duration.replace(duration);
            self.resampler
                .write()
                .unwrap()
                .replace(Resampler::<i16>::new(*spec, 44100_usize, duration));
        }
        self
    }

    pub fn open(mut self, spec: SignalSpec, duration: Duration) -> Self {
        self.init_resampler(&spec, duration);
        self
    }

    fn encode_output(&mut self, buf: &[i16]) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            log::trace!("Encoding {} bytes {read}..{end}", end - read);
            match moosicbox_converter::mp3::encode_mp3(&mut self.encoder, &buf[read..end]) {
                Ok((output, info)) => {
                    log::trace!(
                        "input_consumed={} output_size={} (output len={})",
                        info.input_consumed,
                        info.output_size,
                        output.len()
                    );
                    written.extend_from_slice(&output);
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
        decoded: AudioBuffer<f32>,
    ) -> Result<Vec<i16>, AudioOutputError> {
        let spec = decoded.spec();
        let duration = decoded.capacity() as u64;

        self.init_resampler(spec, duration);

        let mut binding = { self.resampler.write().unwrap() };

        if let Some(resampler) = binding.as_mut() {
            log::debug!(
                "Resampling rate={:?} duration={:?}",
                self.rate,
                self.duration
            );

            Ok(resampler
                .resample(decoded)
                .ok_or(AudioOutputError::StreamEnd)?
                .to_vec())
        } else {
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

        let decoded = self.resample_if_needed(decoded)?;

        Ok(self.encode_output(&decoded))
    }
}

impl AudioOutput for Mp3Encoder {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        if self.writer.is_none() {
            return Ok(0);
        }

        let bytes = self.encode(decoded)?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += writer.write(&bytes[count..]).unwrap();
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
}

pub fn encode_mp3_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_mp3_spawn(path, writer);

    stream
}

pub fn encode_mp3_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_mp3(path, writer) })
}

pub fn encode_mp3<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                Mp3Encoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to mp3: {err:?}");
    }
}
