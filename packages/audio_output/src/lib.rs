#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::sync::{Arc, LazyLock};

use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError};
use moosicbox_resampler::{to_audio_buffer, Resampler};
pub use symphonia::core::audio::SignalSpec;
use symphonia::core::audio::{AudioBuffer, Signal as _};
use symphonia::core::conv::{FromSample, IntoSample as _};
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;

pub mod encoders;

#[cfg(any(feature = "pulseaudio-standard", feature = "pulseaudio-simple"))]
pub mod pulseaudio;

#[cfg(feature = "cpal")]
pub mod cpal;

pub struct AudioOutput {
    pub name: String,
    pub spec: SignalSpec,
    resampler: Option<Resampler<f32>>,
    writer: Box<dyn AudioWrite>,
}

impl AudioOutput {
    pub fn new(name: String, spec: SignalSpec, writer: Box<dyn AudioWrite>) -> Self {
        Self {
            name,
            spec,
            resampler: None,
            writer,
        }
    }

    fn resample_if_needed(
        &mut self,
        decoded: AudioBuffer<f32>,
    ) -> Result<AudioBuffer<f32>, AudioOutputError> {
        Ok(if let Some(resampler) = &mut self.resampler {
            // Resampling is required. The resampler will return interleaved samples in the
            // correct sample format.
            let samples = match resampler.resample(decoded) {
                Some(resampled) => resampled,
                None => return Err(AudioOutputError::StreamEnd),
            };

            to_audio_buffer(samples, self.spec)
        } else if decoded.spec().rate != self.spec.rate {
            let duration = decoded.capacity();
            log::debug!(
                "audio_output: resample_if_needed: resampling from {} to {} original_duration={} target_duration={}",
                decoded.spec().rate,
                self.spec.rate,
                decoded.capacity(),
                duration,
            );
            self.resampler.replace(Resampler::new(
                *decoded.spec(),
                self.spec.rate as usize,
                duration as u64,
            ));
            self.resample_if_needed(decoded)?
        } else {
            decoded
        })
    }
}

impl AudioWrite for AudioOutput {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        let buf = {
            match self.resample_if_needed(decoded) {
                Ok(buf) => buf,
                Err(e) => match e {
                    AudioOutputError::StreamEnd => return Ok(0),
                    _ => return Err(e),
                },
            }
        };
        self.writer.write(buf)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        self.writer.flush()
    }
}

impl AudioDecode for AudioOutput {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        let buf = {
            match self.resample_if_needed(decoded) {
                Ok(buf) => buf,
                Err(e) => match e {
                    AudioOutputError::StreamEnd => return Ok(()),
                    _ => return Err(AudioDecodeError::Other(Box::new(e))),
                },
            }
        };
        self.writer
            .write(buf)
            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;
        Ok(())
    }
}

type InnerType = Box<dyn AudioWrite>;
pub type GetWriter = Box<dyn Fn() -> Result<InnerType, AudioOutputError> + Send>;

#[derive(Clone)]
pub struct AudioOutputFactory {
    pub name: String,
    pub spec: SignalSpec,
    get_writer: Arc<std::sync::Mutex<GetWriter>>,
}

impl AudioOutputFactory {
    pub fn new(
        name: String,
        spec: SignalSpec,
        writer: impl (Fn() -> Result<InnerType, AudioOutputError>) + Send + 'static,
    ) -> Self {
        Self {
            name,
            spec,
            get_writer: Arc::new(std::sync::Mutex::new(Box::new(writer))),
        }
    }

    pub fn new_box(name: String, spec: SignalSpec, writer: GetWriter) -> Self {
        Self {
            name,
            spec,
            get_writer: Arc::new(std::sync::Mutex::new(writer)),
        }
    }

    pub fn try_into_output(&self) -> Result<AudioOutput, AudioOutputError> {
        self.try_into()
    }
}

impl TryFrom<AudioOutputFactory> for AudioOutput {
    type Error = AudioOutputError;

    fn try_from(value: AudioOutputFactory) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name,
            spec: value.spec,
            resampler: None,
            writer: (value.get_writer.lock().unwrap())()?,
        })
    }
}

impl TryFrom<&AudioOutputFactory> for AudioOutput {
    type Error = AudioOutputError;

    fn try_from(value: &AudioOutputFactory) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name.to_owned(),
            spec: value.spec.to_owned(),
            resampler: None,
            writer: (value.get_writer.lock().unwrap())()?,
        })
    }
}

pub trait AudioWrite {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError>;
    fn flush(&mut self) -> Result<(), AudioOutputError>;
}

impl AudioDecode for Box<dyn AudioWrite> {
    fn decoded(
        &mut self,
        decoded: symphonia::core::audio::AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        self.write(decoded)
            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;
        Ok(())
    }
}

impl AudioDecode for &mut dyn AudioWrite {
    fn decoded(
        &mut self,
        decoded: symphonia::core::audio::AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        self.write(decoded)
            .map_err(|e| AudioDecodeError::Other(Box::new(e)))?;
        Ok(())
    }
}

impl From<Box<dyn AudioWrite>> for Box<dyn AudioDecode> {
    fn from(value: Box<dyn AudioWrite>) -> Self {
        Box::new(value)
    }
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioOutputError {
    #[error("No audio outputs")]
    NoOutputs,
    #[error("OpenStreamError")]
    OpenStream,
    #[error("PlayStreamError")]
    PlayStream,
    #[error("StreamClosedError")]
    StreamClosed,
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[allow(unused)]
fn to_samples<S: FromSample<f32> + Default + Clone>(decoded: AudioBuffer<f32>) -> Vec<S> {
    let n_channels = decoded.spec().channels.count();
    let n_samples = decoded.frames() * n_channels;
    let mut buf: Vec<S> = vec![S::default(); n_samples];

    // Interleave the source buffer channels into the sample buffer.
    for ch in 0..n_channels {
        let ch_slice = decoded.chan(ch);

        for (dst, decoded) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
            *dst = (*decoded).into_sample();
        }
    }

    buf
}

static AUDIO_OUTPUT_SCANNER: LazyLock<Arc<Mutex<AudioOutputScanner>>> =
    LazyLock::new(|| Arc::new(Mutex::new(AudioOutputScanner::new())));

pub async fn scan_outputs() -> Result<(), AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.scan().await
}

pub async fn default_output_factory() -> Option<AudioOutputFactory> {
    AUDIO_OUTPUT_SCANNER
        .lock()
        .await
        .default_output_factory()
        .await
        .cloned()
}

pub async fn default_output() -> Result<AudioOutput, AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.default_output().await
}

pub struct AudioOutputScanner {
    pub outputs: Vec<AudioOutputFactory>,
    pub default_output: Option<AudioOutputFactory>,
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioOutputScannerError {
    #[error("No outputs available")]
    NoOutputs,
    #[error(transparent)]
    AudioOutput(#[from] AudioOutputError),
    #[error(transparent)]
    Join(#[from] JoinError),
}

impl AudioOutputScanner {
    pub fn new() -> Self {
        Self {
            outputs: vec![],
            default_output: None,
        }
    }

    pub async fn scan(&mut self) -> Result<(), AudioOutputScannerError> {
        #[cfg(feature = "cpal")]
        {
            self.default_output = moosicbox_task::spawn(
                "server: scan cpal default output",
                moosicbox_task::spawn_blocking(
                    "server: scan cpal default output (blocking)",
                    || {
                        let start = std::time::SystemTime::now();
                        let output = crate::cpal::player::scan_default_output();

                        if let Some(output) = &output {
                            log::debug!("cpal output: {}", output.name);
                        }

                        let end = std::time::SystemTime::now();
                        log::debug!(
                            "took {}ms to scan default output",
                            end.duration_since(start).unwrap().as_millis()
                        );
                        output
                    },
                ),
            )
            .await??;

            self.outputs = moosicbox_task::spawn(
                "server: scan cpal outputs",
                moosicbox_task::spawn_blocking("server: scan cpal outputs (blocking)", || {
                    let start = std::time::SystemTime::now();
                    let outputs = crate::cpal::player::scan_available_outputs().collect::<Vec<_>>();

                    for output in &outputs {
                        log::debug!("cpal output: {}", output.name);
                    }

                    let end = std::time::SystemTime::now();
                    log::debug!(
                        "took {}ms to scan outputs",
                        end.duration_since(start).unwrap().as_millis()
                    );
                    outputs
                }),
            )
            .await??;
        }

        Ok(())
    }

    pub async fn default_output_factory(&self) -> Option<&AudioOutputFactory> {
        self.default_output.as_ref()
    }

    pub async fn default_output(&self) -> Result<AudioOutput, AudioOutputScannerError> {
        self.default_output_factory()
            .await
            .map(|x| x.try_into())
            .transpose()?
            .ok_or(AudioOutputScannerError::NoOutputs)
    }
}

impl Default for AudioOutputScanner {
    fn default() -> Self {
        Self::new()
    }
}
