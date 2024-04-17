use std::sync::RwLock;

use crate::output::{AudioEncoder, AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use lazy_static::lazy_static;
use moosicbox_converter::aac::encoder_aac;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::*;
use symphonia::core::conv::IntoSample;
use symphonia::core::units::Duration;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct AacEncoder {
    resampler: Option<RwLock<Resampler<i16>>>,
    rate: Option<u32>,
    duration: Option<Duration>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: fdk_aac::enc::Encoder,
}

impl AacEncoder {
    pub fn new() -> Self {
        Self {
            resampler: None,
            rate: None,
            duration: None,
            writer: None,
            encoder: encoder_aac().unwrap(),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: None,
            rate: None,
            duration: None,
            writer: Some(Box::new(writer)),
            encoder: encoder_aac().unwrap(),
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
            self.resampler.replace(RwLock::new(Resampler::new(
                spec.clone(),
                44100_usize,
                duration,
            )));
        }
        self
    }

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
            match moosicbox_converter::aac::encode_aac(&self.encoder, &buf[read..end], &mut output)
            {
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
                "Resampling rate={:?} duration={:?}",
                self.rate,
                self.duration
            );

            Ok(resampler
                .write()
                .unwrap()
                .resample(decoded)
                .ok_or(AudioOutputError::StreamEnd)?
                .to_vec())
        } else {
            Ok(to_samples(decoded))
        }
    }
}

fn to_samples(decoded: AudioBuffer<f32>) -> Vec<i16> {
    let n_channels = decoded.spec().channels.count();
    let n_samples = decoded.frames() * n_channels;
    let mut buf = vec![0_i16; n_samples];

    // Interleave the source buffer channels into the sample buffer.
    for ch in 0..n_channels {
        let ch_slice = decoded.chan(ch);

        for (dst, decoded) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
            *dst = (*decoded).into_sample();
        }
    }

    buf
}

impl AudioEncoder for AacEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("OpusEncoder encode {} frames", decoded.frames());

        let decoded = self.resample_if_needed(decoded)?;

        Ok(self.encode_output(&decoded))
    }
}

impl AudioOutput for AacEncoder {
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

pub fn encode_aac_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_aac_spawn(path, writer);

    stream
}

pub fn encode_aac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_aac(path, writer) })
}

pub fn encode_aac<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                AacEncoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to aac: {err:?}");
    }
}
