#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::{Arc, LazyLock};

use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError};
use moosicbox_resampler::{to_audio_buffer, Resampler};
use symphonia::core::audio::{AudioBuffer, Signal as _};
pub use symphonia::core::audio::{Channels, SignalSpec};
use symphonia::core::conv::{FromSample, IntoSample as _};
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;

pub mod encoder;

#[cfg(feature = "api")]
pub mod api;

#[cfg(not(target_os = "windows"))]
#[cfg(any(feature = "pulseaudio-standard", feature = "pulseaudio-simple"))]
pub mod pulseaudio;

#[cfg(feature = "cpal")]
pub mod cpal;

pub struct AudioOutput {
    pub id: String,
    pub name: String,
    pub spec: SignalSpec,
    resampler: Option<Resampler<f32>>,
    writer: Box<dyn AudioWrite>,
}

impl AudioOutput {
    #[must_use]
    pub fn new(id: String, name: String, spec: SignalSpec, writer: Box<dyn AudioWrite>) -> Self {
        Self {
            id,
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
            let Some(samples) = resampler.resample(&decoded) else {
                return Err(AudioOutputError::StreamEnd);
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
    pub id: String,
    pub name: String,
    pub spec: SignalSpec,
    get_writer: Arc<std::sync::Mutex<GetWriter>>,
}

impl std::fmt::Debug for AudioOutputFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutputFactory")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .field("get_writer", &"{{get_writer}}")
            .finish()
    }
}

impl AudioOutputFactory {
    pub fn new(
        id: String,
        name: String,
        spec: SignalSpec,
        writer: impl (Fn() -> Result<InnerType, AudioOutputError>) + Send + 'static,
    ) -> Self {
        Self {
            id,
            name,
            spec,
            get_writer: Arc::new(std::sync::Mutex::new(Box::new(writer))),
        }
    }

    #[must_use]
    pub fn new_box(id: String, name: String, spec: SignalSpec, writer: GetWriter) -> Self {
        Self {
            id,
            name,
            spec,
            get_writer: Arc::new(std::sync::Mutex::new(writer)),
        }
    }

    /// # Errors
    ///
    /// * If fails to instantiate the `AudioOutput`
    pub fn try_into_output(&self) -> Result<AudioOutput, AudioOutputError> {
        self.try_into()
    }
}

impl TryFrom<AudioOutputFactory> for AudioOutput {
    type Error = AudioOutputError;

    fn try_from(value: AudioOutputFactory) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
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
            id: value.id.clone(),
            name: value.name.clone(),
            spec: value.spec,
            resampler: None,
            writer: (value.get_writer.lock().unwrap())()?,
        })
    }
}

pub trait AudioWrite {
    /// # Errors
    ///
    /// * If fails to write the `AudioBuffer`
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError>;

    /// # Errors
    ///
    /// * If fails to flush the `AudioWrite`
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
    #[error("Unsupported output configuration")]
    UnsupportedOutputConfiguration,
    #[error("Unsupported channels: {0}")]
    UnsupportedChannels(usize),
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
    #[cfg(feature = "cpal")]
    #[error(transparent)]
    SupportedStreamConfigs(#[from] ::cpal::SupportedStreamConfigsError),
}

#[allow(unused)]
fn to_samples<S: FromSample<f32> + Default + Clone>(decoded: &AudioBuffer<f32>) -> Vec<S> {
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

/// # Errors
///
/// * If the `scan` fails
pub async fn scan_outputs() -> Result<(), AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.scan().await
}

pub async fn output_factories() -> Vec<AudioOutputFactory> {
    AUDIO_OUTPUT_SCANNER.lock().await.outputs.clone()
}

pub async fn default_output_factory() -> Option<AudioOutputFactory> {
    AUDIO_OUTPUT_SCANNER
        .lock()
        .await
        .default_output_factory()
        .cloned()
}

/// # Errors
///
/// * If there is no default output
pub async fn default_output() -> Result<AudioOutput, AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.default_output()
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
    #[must_use]
    pub const fn new() -> Self {
        Self {
            outputs: vec![],
            default_output: None,
        }
    }

    /// # Panics
    ///
    /// * If time went backwards
    ///
    /// # Errors
    ///
    /// * If the tokio spawned tasks fail to join
    #[allow(clippy::too_many_lines, clippy::unused_async)]
    pub async fn scan(&mut self) -> Result<(), AudioOutputScannerError> {
        self.default_output = None;
        self.outputs = vec![];

        #[cfg(feature = "cpal")]
        {
            self.outputs.extend(
                moosicbox_task::spawn(
                    "server: scan cpal outputs",
                    moosicbox_task::spawn_blocking("server: scan cpal outputs (blocking)", || {
                        let start = std::time::SystemTime::now();
                        let outputs = crate::cpal::scan_available_outputs().collect::<Vec<_>>();

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
                .await??,
            );

            if self.default_output.is_none() {
                self.default_output = moosicbox_task::spawn(
                    "server: scan cpal default output",
                    moosicbox_task::spawn_blocking(
                        "server: scan cpal default output (blocking)",
                        || {
                            let start = std::time::SystemTime::now();
                            let output = crate::cpal::scan_default_output();

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

                if let Some(output) = &self.default_output {
                    if !self.outputs.iter().any(|x| x.id == output.id) {
                        if self.outputs.is_empty() {
                            self.outputs.push(output.clone());
                        } else {
                            self.outputs.insert(0, output.clone());
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        #[cfg(feature = "pulseaudio-standard")]
        {
            self.outputs.extend(
                moosicbox_task::spawn(
                    "server: scan pulseaudio-standard outputs",
                    moosicbox_task::spawn_blocking(
                        "server: scan pulseaudio-standard outputs (blocking)",
                        || {
                            let start = std::time::SystemTime::now();
                            let outputs = crate::pulseaudio::standard::scan_available_outputs()
                                .collect::<Vec<_>>();

                            for output in &outputs {
                                log::debug!("pulseaudio-standard output: {}", output.name);
                            }

                            let end = std::time::SystemTime::now();
                            log::debug!(
                                "took {}ms to scan outputs",
                                end.duration_since(start).unwrap().as_millis()
                            );
                            outputs
                        },
                    ),
                )
                .await??,
            );

            if self.default_output.is_none() {
                self.default_output = moosicbox_task::spawn(
                    "server: scan pulseaudio-standard default output",
                    moosicbox_task::spawn_blocking(
                        "server: scan pulseaudio-standard default output (blocking)",
                        || {
                            let start = std::time::SystemTime::now();
                            let output = crate::pulseaudio::standard::scan_default_output();

                            if let Some(output) = &output {
                                log::debug!("pulseaudio-standard output: {}", output.name);
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

                if let Some(output) = &self.default_output {
                    if !self.outputs.iter().any(|x| x.id == output.id) {
                        if self.outputs.is_empty() {
                            self.outputs.push(output.clone());
                        } else {
                            self.outputs.insert(0, output.clone());
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        #[cfg(feature = "pulseaudio-simple")]
        {
            self.outputs.extend(
                moosicbox_task::spawn(
                    "server: scan pulseaudio-simple outputs",
                    moosicbox_task::spawn_blocking(
                        "server: scan pulseaudio-simple outputs (blocking)",
                        || {
                            let start = std::time::SystemTime::now();
                            let outputs = crate::pulseaudio::simple::scan_available_outputs()
                                .collect::<Vec<_>>();

                            for output in &outputs {
                                log::debug!("pulseaudio-simple output: {}", output.name);
                            }

                            let end = std::time::SystemTime::now();
                            log::debug!(
                                "took {}ms to scan outputs",
                                end.duration_since(start).unwrap().as_millis()
                            );
                            outputs
                        },
                    ),
                )
                .await??,
            );

            if self.default_output.is_none() {
                self.default_output = moosicbox_task::spawn(
                    "server: scan pulseaudio-simple default output",
                    moosicbox_task::spawn_blocking(
                        "server: scan pulseaudio-simple default output (blocking)",
                        || {
                            let start = std::time::SystemTime::now();
                            let output = crate::pulseaudio::simple::scan_default_output();

                            if let Some(output) = &output {
                                log::debug!("pulseaudio-simple output: {}", output.name);
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

                if let Some(output) = &self.default_output {
                    if !self.outputs.iter().any(|x| x.id == output.id) {
                        if self.outputs.is_empty() {
                            self.outputs.push(output.clone());
                        } else {
                            self.outputs.insert(0, output.clone());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub const fn default_output_factory(&self) -> Option<&AudioOutputFactory> {
        self.default_output.as_ref()
    }

    /// # Errors
    ///
    /// * If there is no default output
    pub fn default_output(&self) -> Result<AudioOutput, AudioOutputScannerError> {
        self.default_output_factory()
            .map(TryInto::try_into)
            .transpose()?
            .ok_or(AudioOutputScannerError::NoOutputs)
    }
}

impl Default for AudioOutputScanner {
    fn default() -> Self {
        Self::new()
    }
}
