//! Audio output management for `MoosicBox`.
//!
//! This crate provides a flexible audio output system that supports writing decoded audio samples
//! to various output destinations (hardware devices, encoders, etc.). It handles automatic
//! resampling when the decoded sample rate doesn't match the output specification.
//!
//! # Features
//!
//! * Audio output abstraction through the [`AudioWrite`] trait
//! * Automatic resampling support via [`AudioOutput`]
//! * Audio device scanning and management with [`AudioOutputScanner`]
//! * Progress tracking for playback position with [`ProgressTracker`]
//! * Async command-based control through [`AudioHandle`]
//! * Support for CPAL audio backend (with `cpal` feature)
//! * Multiple audio encoder formats: AAC, FLAC, MP3, Opus (with corresponding features)
//!
//! # Example
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_audio_output::{scan_outputs, default_output};
//!
//! // Scan for available audio outputs
//! scan_outputs().await?;
//!
//! // Get the default audio output
//! let mut output = default_output().await?;
//!
//! // Use the output with an audio decoder...
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Resource daemon pattern for managing !Send resources in dedicated threads.
pub mod resource_daemon;

#[cfg(feature = "cpal")]
/// CPAL stream daemon for managing !Send CPAL streams in dedicated threads.
pub mod cpal_daemon;

use std::sync::{Arc, LazyLock};

use moosicbox_audio_decoder::{AudioDecode, AudioDecodeError};
use moosicbox_resampler::{Resampler, to_audio_buffer};
use switchy_async::task::JoinError;
use symphonia::core::audio::{AudioBuffer, Signal as _};
use symphonia::core::conv::FromSample;
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;
use tokio::sync::Mutex;

// Reexport commonly used Symphonia types for centralized imports
pub use symphonia::core::audio::{Channels, SignalSpec};
pub use symphonia::core::conv::{ConvertibleSample, IntoSample};
pub use symphonia::core::units::Duration;

// Export ProgressTracker for use by AudioOutput implementations
pub use progress_tracker::ProgressTracker;

// Export command types for use by AudioOutput implementations
pub use command::{AudioCommand, AudioError, AudioHandle, AudioResponse, CommandMessage};

/// Command-based control interface for audio outputs.
pub mod command;
/// Audio encoders for compressing decoded audio into various formats.
pub mod encoder;

#[cfg(feature = "api")]
/// HTTP API endpoints for managing audio outputs.
pub mod api;

#[cfg(feature = "cpal")]
/// CPAL (Cross-Platform Audio Library) audio output implementation.
pub mod cpal;

/// Progress tracking for audio playback.
pub mod progress_tracker;

/// An audio output that writes decoded audio samples to an underlying audio device or stream.
///
/// This struct handles audio resampling when the decoded sample rate doesn't match the output
/// specification, and delegates the actual writing to an `AudioWrite` implementation.
pub struct AudioOutput {
    /// Unique identifier for this audio output
    pub id: String,
    /// Human-readable name for this audio output
    pub name: String,
    /// Audio signal specification (sample rate, channels, etc.)
    pub spec: SignalSpec,
    resampler: Option<Resampler<f32>>,
    writer: Box<dyn AudioWrite>,
}

impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl AudioOutput {
    /// Creates a new `AudioOutput` with the specified configuration.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this audio output
    /// * `name` - Human-readable name for this audio output
    /// * `spec` - Audio signal specification (sample rate, channels, etc.)
    /// * `writer` - The underlying audio writer implementation
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
        AudioWrite::flush(&mut *self.writer)
    }

    fn get_playback_position(&self) -> Option<f64> {
        self.writer.get_playback_position()
    }

    fn set_consumed_samples(&mut self, consumed_samples: Arc<std::sync::atomic::AtomicUsize>) {
        self.writer.set_consumed_samples(consumed_samples);
    }

    fn set_volume(&mut self, volume: f64) {
        self.writer.set_volume(volume);
    }

    fn set_shared_volume(&mut self, shared_volume: std::sync::Arc<atomic_float::AtomicF64>) {
        self.writer.set_shared_volume(shared_volume);
    }

    fn get_output_spec(&self) -> Option<symphonia::core::audio::SignalSpec> {
        self.writer.get_output_spec()
    }

    fn set_progress_callback(
        &mut self,
        callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
        self.writer.set_progress_callback(callback);
    }

    fn handle(&self) -> AudioHandle {
        self.writer.handle()
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

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        AudioWrite::flush(self).map_err(|e| AudioDecodeError::Other(Box::new(e)))
    }
}

type InnerType = Box<dyn AudioWrite>;

/// Function type for creating audio writer instances on demand.
///
/// This function type is used by [`AudioOutputFactory`] to defer the creation
/// of audio writers until they are actually needed.
pub type GetWriter = Box<dyn Fn() -> Result<InnerType, AudioOutputError> + Send>;

/// A factory for creating `AudioOutput` instances.
///
/// This allows deferring the creation of the underlying `AudioWrite` implementation
/// until the output is actually needed, which is useful for managing audio device resources.
#[derive(Clone)]
pub struct AudioOutputFactory {
    /// Unique identifier for this audio output factory
    pub id: String,
    /// Human-readable name for this audio output
    pub name: String,
    /// Audio signal specification (sample rate, channels, etc.)
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
    /// Creates a new `AudioOutputFactory` with a writer function.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this audio output factory
    /// * `name` - Human-readable name for this audio output
    /// * `spec` - Audio signal specification (sample rate, channels, etc.)
    /// * `writer` - Function that creates the underlying audio writer when called
    #[must_use]
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

    /// Creates a new `AudioOutputFactory` with a boxed writer function.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this audio output factory
    /// * `name` - Human-readable name for this audio output
    /// * `spec` - Audio signal specification (sample rate, channels, etc.)
    /// * `writer` - Boxed function that creates the underlying audio writer when called
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
    #[must_use]
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

/// Trait for writing decoded audio samples to an output destination.
///
/// Implementors of this trait handle the low-level details of writing audio data
/// to hardware devices, encoders, or other output streams.
pub trait AudioWrite {
    /// Writes decoded audio samples to the output.
    ///
    /// Returns the number of samples written.
    ///
    /// # Errors
    ///
    /// * If fails to write the `AudioBuffer`
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError>;

    /// Flushes any buffered audio data to the output.
    ///
    /// # Errors
    ///
    /// * If fails to flush the `AudioWrite`
    fn flush(&mut self) -> Result<(), AudioOutputError>;

    /// Get the actual playback position in seconds based on consumed samples
    /// Returns None if not supported by the audio output implementation
    fn get_playback_position(&self) -> Option<f64> {
        None
    }

    /// Set the consumed samples counter for progress tracking
    /// Default implementation does nothing
    fn set_consumed_samples(&mut self, _consumed_samples: Arc<std::sync::atomic::AtomicUsize>) {}

    /// Set the volume for immediate effect
    /// Default implementation does nothing
    fn set_volume(&mut self, _volume: f64) {}

    /// Set a shared volume atomic for immediate volume changes
    /// Default implementation does nothing
    fn set_shared_volume(&mut self, _shared_volume: std::sync::Arc<atomic_float::AtomicF64>) {}

    /// Get the actual output audio specification (for accurate progress calculation)
    /// Returns None if not supported by the audio output implementation
    fn get_output_spec(&self) -> Option<SignalSpec> {
        None
    }

    /// Set a progress callback that will be called when playback position changes significantly
    /// The callback receives the current position in seconds
    /// Default implementation does nothing
    fn set_progress_callback(
        &mut self,
        _callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
    }

    /// Get a communication handle for sending commands to this audio output.
    ///
    /// The handle can be used to control playback (pause, resume, seek, etc.)
    /// from other threads or async contexts.
    fn handle(&self) -> AudioHandle;
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

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        (**self)
            .flush()
            .map_err(|e| AudioDecodeError::Other(Box::new(e)))
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

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        (*self)
            .flush()
            .map_err(|e| AudioDecodeError::Other(Box::new(e)))
    }
}

impl From<Box<dyn AudioWrite>> for Box<dyn AudioDecode> {
    fn from(value: Box<dyn AudioWrite>) -> Self {
        Box::new(value)
    }
}

/// Errors that can occur during audio output operations.
#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioOutputError {
    /// No audio output devices are available
    #[error("No audio outputs")]
    NoOutputs,
    /// The requested audio output configuration is not supported
    #[error("Unsupported output configuration")]
    UnsupportedOutputConfiguration,
    /// The requested number of audio channels is not supported
    #[error("Unsupported channels: {0}")]
    UnsupportedChannels(usize),
    /// Failed to open the audio output stream
    #[error("OpenStreamError")]
    OpenStream,
    /// Failed to start playing the audio stream
    #[error("PlayStreamError")]
    PlayStream,
    /// The audio stream was closed unexpectedly
    #[error("StreamClosedError")]
    StreamClosed,
    /// The audio stream reached its end
    #[error("StreamEndError")]
    StreamEnd,
    /// Audio playback was interrupted
    #[error("InterruptError")]
    Interrupt,
    /// An I/O error occurred
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Failed to query supported stream configurations (CPAL-specific)
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
#[must_use]
pub async fn scan_outputs() -> Result<(), AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.scan().await
}

/// Returns all available audio output factories.
///
/// The list is populated by calling [`scan_outputs()`] first.
#[must_use]
pub async fn output_factories() -> Vec<AudioOutputFactory> {
    AUDIO_OUTPUT_SCANNER.lock().await.outputs.clone()
}

/// Returns the default audio output factory, if available.
///
/// The default output is determined by calling [`scan_outputs()`] first.
#[must_use]
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
#[must_use]
pub async fn default_output() -> Result<AudioOutput, AudioOutputScannerError> {
    AUDIO_OUTPUT_SCANNER.lock().await.default_output()
}

/// Scans and manages available audio output devices.
pub struct AudioOutputScanner {
    /// All available audio output factories
    pub outputs: Vec<AudioOutputFactory>,
    /// The default audio output factory, if available
    pub default_output: Option<AudioOutputFactory>,
}

/// Errors that can occur during audio output scanning operations.
#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioOutputScannerError {
    /// No audio output devices were found during scanning
    #[error("No outputs available")]
    NoOutputs,
    /// An error occurred while working with an audio output
    #[error(transparent)]
    AudioOutput(#[from] AudioOutputError),
    /// A spawned task failed to join
    #[error(transparent)]
    Join(#[from] JoinError),
}

impl AudioOutputScanner {
    /// Creates a new `AudioOutputScanner` with no outputs.
    ///
    /// Call [`scan()`](Self::scan) to populate the list of available outputs.
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
                switchy_async::runtime::Handle::current()
                    .spawn_with_name(
                        "server: scan cpal outputs",
                        switchy_async::runtime::Handle::current().spawn_blocking_with_name(
                            "server: scan cpal outputs (blocking)",
                            || {
                                let start = switchy_time::now();
                                let outputs =
                                    crate::cpal::scan_available_outputs().collect::<Vec<_>>();

                                for output in &outputs {
                                    log::debug!("cpal output: {}", output.name);
                                }

                                let end = switchy_time::now();
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
                self.default_output = switchy_async::runtime::Handle::current()
                    .spawn_with_name(
                        "server: scan cpal default output",
                        switchy_async::runtime::Handle::current().spawn_blocking_with_name(
                            "server: scan cpal default output (blocking)",
                            || {
                                let start = switchy_time::now();
                                let output = crate::cpal::scan_default_output();

                                if let Some(output) = &output {
                                    log::debug!("cpal output: {}", output.name);
                                }

                                let end = switchy_time::now();
                                log::debug!(
                                    "took {}ms to scan default output",
                                    end.duration_since(start).unwrap().as_millis()
                                );
                                output
                            },
                        ),
                    )
                    .await??;

                if let Some(output) = &self.default_output
                    && !self.outputs.iter().any(|x| x.id == output.id)
                {
                    if self.outputs.is_empty() {
                        self.outputs.push(output.clone());
                    } else {
                        self.outputs.insert(0, output.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns a reference to the default audio output factory, if available.
    #[must_use]
    pub const fn default_output_factory(&self) -> Option<&AudioOutputFactory> {
        self.default_output.as_ref()
    }

    /// # Errors
    ///
    /// * If there is no default output
    #[must_use]
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
