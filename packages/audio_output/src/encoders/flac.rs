use std::sync::RwLock;

use bytes::Bytes;
use moosicbox_audio_decoder::{
    decode_file_path_str, AudioDecode, AudioDecodeError, AudioDecodeHandler,
};
use moosicbox_audio_encoder::flac::{encoder_flac, Encoder};
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::formats::Track;
use symphonia::core::units::Duration;
use symphonia::core::{audio::*, formats::Packet};

use crate::{to_samples, AudioOutput, AudioOutputError};
use moosicbox_resampler::Resampler;

use super::AudioEncoder;

pub struct FlacEncoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    input_rate: Option<u32>,
    resample_rate: Option<u32>,
    output_rate: usize,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: Encoder,
}

impl FlacEncoder {
    pub fn new() -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: None,
            encoder: encoder_flac().expect("Failed to create Flac encoder"),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: None,
            input_rate: None,
            resample_rate: None,
            output_rate: 44100,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_flac().expect("Failed to create Flac encoder"),
        }
    }

    pub fn init_resampler(&mut self, spec: &SignalSpec, duration: Duration) -> &Self {
        self.input_rate.replace(spec.rate);
        self.duration.replace(duration);

        if !self.resample_rate.is_some_and(|r| r == spec.rate)
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

    pub fn open(mut self, spec: SignalSpec, duration: Duration) -> Self {
        self.init_resampler(&spec, duration);
        self
    }

    fn encode_output(&mut self, buf: &[i16]) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            let mut output = [0u8; 4096];
            match moosicbox_audio_encoder::flac::encode_flac(
                &mut self.encoder,
                &buf[read..end].iter().map(|x| *x as i32).collect::<Vec<_>>(),
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
        decoded: AudioBuffer<f32>,
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

impl Default for FlacEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEncoder for FlacEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("FlacEncoder encode {} frames", decoded.frames());

        let decoded = self.resample_if_needed(decoded)?;

        Ok(self.encode_output(&decoded))
    }

    fn spec(&self) -> SignalSpec {
        SignalSpec {
            rate: self.output_rate as u32,
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        }
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
            AudioDecodeError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to encode: {e:?}"),
            ))
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

impl AudioOutput for FlacEncoder {
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
}

pub fn encode_flac_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_flac_spawn(path, writer);

    stream
}

pub fn encode_flac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    moosicbox_task::spawn_blocking("audio_decoder: encode_flac", move || {
        encode_flac(path, writer)
    })
}

pub fn encode_flac<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_decode_handler =
        AudioDecodeHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                FlacEncoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = decode_file_path_str(&path, &mut audio_decode_handler, true, true, None, None)
    {
        log::error!("Failed to encode to flac: {err:?}");
    }
}
