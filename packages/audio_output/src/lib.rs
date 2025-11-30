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
use switchy_async::sync::Mutex;
use switchy_async::task::JoinError;
use symphonia::core::audio::{AudioBuffer, Signal as _};
use symphonia::core::conv::FromSample;
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;

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

/// Converts an audio buffer to interleaved samples of the specified type.
///
/// This helper function takes an audio buffer with samples organized by channel
/// and converts them into an interleaved format where samples from different
/// channels alternate (e.g., L, R, L, R for stereo).
///
/// # Type Parameters
/// * `S` - The target sample type (must implement `FromSample<f32>`)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test_log::test]
    fn test_audio_output_error_debug() {
        let err = AudioOutputError::NoOutputs;
        assert_eq!(format!("{err:?}"), "NoOutputs");

        let err = AudioOutputError::UnsupportedOutputConfiguration;
        assert_eq!(format!("{err:?}"), "UnsupportedOutputConfiguration");

        let err = AudioOutputError::UnsupportedChannels(5);
        assert_eq!(format!("{err:?}"), "UnsupportedChannels(5)");

        let err = AudioOutputError::OpenStream;
        assert_eq!(format!("{err:?}"), "OpenStream");

        let err = AudioOutputError::PlayStream;
        assert_eq!(format!("{err:?}"), "PlayStream");

        let err = AudioOutputError::StreamClosed;
        assert_eq!(format!("{err:?}"), "StreamClosed");

        let err = AudioOutputError::StreamEnd;
        assert_eq!(format!("{err:?}"), "StreamEnd");

        let err = AudioOutputError::Interrupt;
        assert_eq!(format!("{err:?}"), "Interrupt");
    }

    #[test_log::test]
    fn test_audio_output_error_display() {
        let err = AudioOutputError::NoOutputs;
        assert_eq!(format!("{err}"), "No audio outputs");

        let err = AudioOutputError::UnsupportedOutputConfiguration;
        assert_eq!(format!("{err}"), "Unsupported output configuration");

        let err = AudioOutputError::UnsupportedChannels(5);
        assert_eq!(format!("{err}"), "Unsupported channels: 5");

        let err = AudioOutputError::OpenStream;
        assert_eq!(format!("{err}"), "OpenStreamError");

        let err = AudioOutputError::PlayStream;
        assert_eq!(format!("{err}"), "PlayStreamError");

        let err = AudioOutputError::StreamClosed;
        assert_eq!(format!("{err}"), "StreamClosedError");

        let err = AudioOutputError::StreamEnd;
        assert_eq!(format!("{err}"), "StreamEndError");

        let err = AudioOutputError::Interrupt;
        assert_eq!(format!("{err}"), "InterruptError");
    }

    #[test_log::test]
    fn test_audio_output_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: AudioOutputError = io_err.into();
        assert!(matches!(err, AudioOutputError::IO(_)));
    }

    #[test_log::test]
    fn test_audio_output_scanner_error_debug() {
        let err = AudioOutputScannerError::NoOutputs;
        assert_eq!(format!("{err:?}"), "NoOutputs");
    }

    #[test_log::test]
    fn test_audio_output_scanner_error_display() {
        let err = AudioOutputScannerError::NoOutputs;
        assert_eq!(format!("{err}"), "No outputs available");
    }

    #[test_log::test]
    fn test_audio_output_scanner_error_from_audio_output() {
        let err = AudioOutputError::NoOutputs;
        let scanner_err: AudioOutputScannerError = err.into();
        assert!(matches!(
            scanner_err,
            AudioOutputScannerError::AudioOutput(_)
        ));
    }

    #[test_log::test]
    fn test_audio_output_scanner_new() {
        let scanner = AudioOutputScanner::new();
        assert_eq!(scanner.outputs.len(), 0);
        assert!(scanner.default_output.is_none());
    }

    #[test_log::test]
    fn test_audio_output_scanner_default() {
        let scanner = AudioOutputScanner::default();
        assert_eq!(scanner.outputs.len(), 0);
        assert!(scanner.default_output.is_none());
    }

    #[test_log::test]
    fn test_audio_output_scanner_default_output_factory() {
        let scanner = AudioOutputScanner::new();
        assert!(scanner.default_output_factory().is_none());
    }

    #[test_log::test]
    fn test_audio_output_scanner_default_output_error() {
        let scanner = AudioOutputScanner::new();
        let result = scanner.default_output();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AudioOutputScannerError::NoOutputs
        ));
    }

    #[test_log::test]
    fn test_audio_output_factory_debug() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let debug_str = format!("{factory:?}");
        assert!(debug_str.contains("test-id"));
        assert!(debug_str.contains("Test Output"));
        assert!(debug_str.contains("get_writer"));
    }

    #[test_log::test]
    fn test_audio_output_factory_clone() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let cloned = factory.clone();
        assert_eq!(cloned.id, factory.id);
        assert_eq!(cloned.name, factory.name);
    }

    #[test_log::test]
    fn test_audio_output_factory_new_box() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let writer: GetWriter = Box::new(|| Err(AudioOutputError::NoOutputs));
        let factory = AudioOutputFactory::new_box(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            writer,
        );

        assert_eq!(factory.id, "test-id");
        assert_eq!(factory.name, "Test Output");
    }

    #[test_log::test]
    fn test_audio_output_factory_try_into_output_error() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let result = factory.try_into_output();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioOutputError::NoOutputs));
    }

    #[test_log::test]
    fn test_audio_output_factory_try_from_error() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let result: Result<AudioOutput, _> = factory.try_into();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioOutputError::NoOutputs));
    }

    #[test_log::test]
    fn test_audio_output_factory_try_from_ref_error() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let factory = AudioOutputFactory::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            || Err(AudioOutputError::NoOutputs),
        );

        let result: Result<AudioOutput, _> = (&factory).try_into();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioOutputError::NoOutputs));
    }

    // Mock AudioWrite for testing
    struct MockAudioWrite {
        handle: AudioHandle,
    }

    impl MockAudioWrite {
        fn new() -> Self {
            let (tx, _rx) = flume::bounded(1);
            Self {
                handle: AudioHandle::new(tx),
            }
        }
    }

    impl AudioWrite for MockAudioWrite {
        fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
            Ok(decoded.frames())
        }

        fn flush(&mut self) -> Result<(), AudioOutputError> {
            Ok(())
        }

        fn handle(&self) -> AudioHandle {
            self.handle.clone()
        }
    }

    #[test_log::test]
    fn test_audio_output_new() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(MockAudioWrite::new()),
        );

        assert_eq!(output.id, "test-id");
        assert_eq!(output.name, "Test Output");
        assert_eq!(output.spec.rate, 44100);
    }

    #[test_log::test]
    fn test_audio_output_debug() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(MockAudioWrite::new()),
        );

        let debug_str = format!("{output:?}");
        assert!(debug_str.contains("test-id"));
        assert!(debug_str.contains("Test Output"));
        assert!(debug_str.contains("AudioOutput"));
    }

    #[test_log::test]
    fn test_audio_output_handle() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(MockAudioWrite::new()),
        );

        let _handle = output.handle();
    }

    #[test_log::test]
    fn test_to_samples_stereo_interleaving() {
        use symphonia::core::audio::Signal;

        // Create a stereo audio buffer with 4 frames
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(4, spec);
        // Reserve 4 frames to make the buffer usable
        buffer.render_reserved(Some(4));

        // Fill left channel with [1.0, 2.0, 3.0, 4.0]
        // Fill right channel with [5.0, 6.0, 7.0, 8.0]
        {
            let left = buffer.chan_mut(0);
            left[0] = 1.0;
            left[1] = 2.0;
            left[2] = 3.0;
            left[3] = 4.0;

            let right = buffer.chan_mut(1);
            right[0] = 5.0;
            right[1] = 6.0;
            right[2] = 7.0;
            right[3] = 8.0;
        }

        let samples: Vec<f32> = to_samples(&buffer);

        // Expected interleaved output: [L0, R0, L1, R1, L2, R2, L3, R3]
        // = [1.0, 5.0, 2.0, 6.0, 3.0, 7.0, 4.0, 8.0]
        assert_eq!(samples.len(), 8);
        assert!((samples[0] - 1.0).abs() < f32::EPSILON);
        assert!((samples[1] - 5.0).abs() < f32::EPSILON);
        assert!((samples[2] - 2.0).abs() < f32::EPSILON);
        assert!((samples[3] - 6.0).abs() < f32::EPSILON);
        assert!((samples[4] - 3.0).abs() < f32::EPSILON);
        assert!((samples[5] - 7.0).abs() < f32::EPSILON);
        assert!((samples[6] - 4.0).abs() < f32::EPSILON);
        assert!((samples[7] - 8.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_to_samples_mono() {
        use symphonia::core::audio::Signal;

        // Create a mono audio buffer with 4 frames
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(4, spec);
        // Reserve 4 frames to make the buffer usable
        buffer.render_reserved(Some(4));

        // Fill mono channel with [1.0, 2.0, 3.0, 4.0]
        {
            let mono = buffer.chan_mut(0);
            mono[0] = 1.0;
            mono[1] = 2.0;
            mono[2] = 3.0;
            mono[3] = 4.0;
        }

        let samples: Vec<f32> = to_samples(&buffer);

        // For mono, output should be unchanged
        assert_eq!(samples.len(), 4);
        assert!((samples[0] - 1.0).abs() < f32::EPSILON);
        assert!((samples[1] - 2.0).abs() < f32::EPSILON);
        assert!((samples[2] - 3.0).abs() < f32::EPSILON);
        assert!((samples[3] - 4.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_to_samples_type_conversion_to_i16() {
        use symphonia::core::audio::Signal;

        // Test that type conversion works correctly
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(2, spec);
        // Reserve 2 frames to make the buffer usable
        buffer.render_reserved(Some(2));

        // Use normalized float values (-1.0 to 1.0)
        {
            let left = buffer.chan_mut(0);
            left[0] = 0.5;
            left[1] = -0.5;

            let right = buffer.chan_mut(1);
            right[0] = 0.25;
            right[1] = -0.25;
        }

        let samples: Vec<i16> = to_samples(&buffer);

        // Expected interleaved output converted to i16
        assert_eq!(samples.len(), 4);
        // 0.5 * 32767 ≈ 16383
        assert!((samples[0] - 16383).abs() <= 1);
        // 0.25 * 32767 ≈ 8191
        assert!((samples[1] - 8191).abs() <= 1);
        // -0.5 * 32768 ≈ -16384
        assert!((samples[2] - (-16384)).abs() <= 1);
        // -0.25 * 32768 ≈ -8192
        assert!((samples[3] - (-8192)).abs() <= 1);
    }

    // Enhanced MockAudioWrite that tracks writes for testing AudioOutput
    struct TrackingMockAudioWrite {
        handle: AudioHandle,
        written_frames: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        flush_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl TrackingMockAudioWrite {
        fn new() -> Self {
            let (tx, _rx) = flume::bounded(1);
            Self {
                handle: AudioHandle::new(tx),
                written_frames: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                flush_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }
    }

    impl AudioWrite for TrackingMockAudioWrite {
        fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
            let frames = decoded.frames();
            self.written_frames
                .fetch_add(frames, std::sync::atomic::Ordering::SeqCst);
            Ok(frames)
        }

        fn flush(&mut self) -> Result<(), AudioOutputError> {
            self.flush_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        fn handle(&self) -> AudioHandle {
            self.handle.clone()
        }
    }

    #[test_log::test]
    fn test_audio_output_write_same_sample_rate() {
        use symphonia::core::audio::Signal;

        // Test that when sample rates match, samples pass through without resampling
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let written_frames = mock_writer.written_frames.clone();

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Create an audio buffer with matching sample rate
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(100, spec);
        buffer.render_reserved(Some(100));
        // Fill with some data
        for ch in 0..2 {
            let chan = buffer.chan_mut(ch);
            #[allow(clippy::cast_precision_loss)]
            for (i, sample) in chan.iter_mut().enumerate().take(100) {
                *sample = (i as f32) / 100.0;
            }
        }

        // Write through AudioWrite trait
        let result = AudioWrite::write(&mut output, buffer);
        assert!(result.is_ok());

        // Verify frames were passed through to the underlying writer
        assert_eq!(
            written_frames.load(std::sync::atomic::Ordering::SeqCst),
            100
        );
    }

    #[test_log::test]
    fn test_audio_output_write_empty_buffer() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let written_frames = mock_writer.written_frames.clone();

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Create an empty audio buffer (0 capacity means 0 frames)
        let buffer: AudioBuffer<f32> = AudioBuffer::new(0, spec);

        // Write through AudioWrite trait
        let result = AudioWrite::write(&mut output, buffer);
        assert!(result.is_ok());

        // No frames should have been written
        assert_eq!(written_frames.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test_log::test]
    fn test_audio_output_flush() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let flush_count = mock_writer.flush_count.clone();

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Flush should delegate to the underlying writer
        let result = AudioWrite::flush(&mut output);
        assert!(result.is_ok());
        assert_eq!(flush_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Multiple flushes should work
        let result = AudioWrite::flush(&mut output);
        assert!(result.is_ok());
        assert_eq!(flush_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test_log::test]
    fn test_audio_output_get_playback_position() {
        struct MockAudioWriteWithPosition {
            handle: AudioHandle,
            position: f64,
        }

        impl AudioWrite for MockAudioWriteWithPosition {
            fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
                Ok(decoded.frames())
            }

            fn flush(&mut self) -> Result<(), AudioOutputError> {
                Ok(())
            }

            fn get_playback_position(&self) -> Option<f64> {
                Some(self.position)
            }

            fn handle(&self) -> AudioHandle {
                self.handle.clone()
            }
        }

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let (tx, _rx) = flume::bounded(1);
        let mock_writer = MockAudioWriteWithPosition {
            handle: AudioHandle::new(tx),
            position: 42.5,
        };

        let output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // get_playback_position should delegate to the underlying writer
        assert_eq!(output.get_playback_position(), Some(42.5));
    }

    #[test_log::test]
    fn test_audio_output_get_output_spec() {
        struct MockAudioWriteWithSpec {
            handle: AudioHandle,
            output_spec: SignalSpec,
        }

        impl AudioWrite for MockAudioWriteWithSpec {
            fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
                Ok(decoded.frames())
            }

            fn flush(&mut self) -> Result<(), AudioOutputError> {
                Ok(())
            }

            fn get_output_spec(&self) -> Option<SignalSpec> {
                Some(self.output_spec)
            }

            fn handle(&self) -> AudioHandle {
                self.handle.clone()
            }
        }

        let input_spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let output_spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let (tx, _rx) = flume::bounded(1);
        let mock_writer = MockAudioWriteWithSpec {
            handle: AudioHandle::new(tx),
            output_spec,
        };

        let output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            input_spec,
            Box::new(mock_writer),
        );

        // get_output_spec should delegate to the underlying writer
        let spec = output.get_output_spec();
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().rate, 48000);
    }

    #[test_log::test]
    fn test_audio_output_audio_decode_decoded_success() {
        use moosicbox_audio_decoder::AudioDecode;
        use symphonia::core::audio::Signal;
        use symphonia::core::codecs::{CODEC_TYPE_NULL, CodecParameters};
        use symphonia::core::formats::{Packet, Track};

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let written_frames = mock_writer.written_frames.clone();

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Create an audio buffer with matching sample rate
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(50, spec);
        buffer.render_reserved(Some(50));
        for ch in 0..2 {
            let chan = buffer.chan_mut(ch);
            #[allow(clippy::cast_precision_loss)]
            for (i, sample) in chan.iter_mut().enumerate().take(50) {
                *sample = (i as f32) / 100.0;
            }
        }

        // Create dummy packet and track for the AudioDecode trait
        let packet = Packet::new_from_slice(0, 0, 0, &[]);
        let track = Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone());

        // Call decoded through AudioDecode trait
        let result = AudioDecode::decoded(&mut output, buffer, &packet, &track);
        assert!(result.is_ok());

        // Verify frames were passed through to the underlying writer
        assert_eq!(written_frames.load(std::sync::atomic::Ordering::SeqCst), 50);
    }

    #[test_log::test]
    fn test_audio_output_audio_decode_flush_success() {
        use moosicbox_audio_decoder::AudioDecode;

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let flush_count = mock_writer.flush_count.clone();

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Call flush through AudioDecode trait
        let result = AudioDecode::flush(&mut output);
        assert!(result.is_ok());

        // Verify flush was called on underlying writer
        assert_eq!(flush_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test]
    fn test_audio_output_audio_decode_flush_error() {
        use moosicbox_audio_decoder::AudioDecode;

        struct FailingFlushWriter {
            handle: AudioHandle,
        }

        impl AudioWrite for FailingFlushWriter {
            fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
                Ok(decoded.frames())
            }

            fn flush(&mut self) -> Result<(), AudioOutputError> {
                Err(AudioOutputError::StreamClosed)
            }

            fn handle(&self) -> AudioHandle {
                self.handle.clone()
            }
        }

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let (tx, _rx) = flume::bounded(1);
        let mock_writer = FailingFlushWriter {
            handle: AudioHandle::new(tx),
        };

        let mut output = AudioOutput::new(
            "test-id".to_string(),
            "Test Output".to_string(),
            spec,
            Box::new(mock_writer),
        );

        // Call flush through AudioDecode trait - should convert error
        let result = AudioDecode::flush(&mut output);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_box_dyn_audio_write_audio_decode_decoded() {
        use moosicbox_audio_decoder::AudioDecode;
        use symphonia::core::audio::Signal;
        use symphonia::core::codecs::{CODEC_TYPE_NULL, CodecParameters};
        use symphonia::core::formats::{Packet, Track};

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let written_frames = mock_writer.written_frames.clone();

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);

        // Create an audio buffer
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(25, spec);
        buffer.render_reserved(Some(25));
        for ch in 0..2 {
            let chan = buffer.chan_mut(ch);
            for sample in chan.iter_mut().take(25) {
                *sample = 0.5;
            }
        }

        let packet = Packet::new_from_slice(0, 0, 0, &[]);
        let track = Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone());

        // Call decoded through AudioDecode trait on Box<dyn AudioWrite>
        let result = AudioDecode::decoded(&mut boxed_writer, buffer, &packet, &track);
        assert!(result.is_ok());

        // Verify frames were written
        assert_eq!(written_frames.load(std::sync::atomic::Ordering::SeqCst), 25);
    }

    #[test_log::test]
    fn test_box_dyn_audio_write_audio_decode_flush() {
        use moosicbox_audio_decoder::AudioDecode;

        let _spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let flush_count = mock_writer.flush_count.clone();

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);

        // Call flush through AudioDecode trait on Box<dyn AudioWrite>
        let result = AudioDecode::flush(&mut boxed_writer);
        assert!(result.is_ok());

        // Verify flush was called
        assert_eq!(flush_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test]
    fn test_ref_mut_dyn_audio_write_audio_decode_decoded() {
        use moosicbox_audio_decoder::AudioDecode;
        use symphonia::core::audio::Signal;
        use symphonia::core::codecs::{CODEC_TYPE_NULL, CodecParameters};
        use symphonia::core::formats::{Packet, Track};

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let written_frames = mock_writer.written_frames.clone();

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);
        let writer_ref: &mut dyn AudioWrite = &mut *boxed_writer;

        // Create an audio buffer
        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(30, spec);
        buffer.render_reserved(Some(30));
        for ch in 0..2 {
            let chan = buffer.chan_mut(ch);
            for sample in chan.iter_mut().take(30) {
                *sample = 0.3;
            }
        }

        let packet = Packet::new_from_slice(0, 0, 0, &[]);
        let track = Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone());

        // Call decoded through AudioDecode trait on &mut dyn AudioWrite
        let result = AudioDecode::decoded(&mut { writer_ref }, buffer, &packet, &track);
        assert!(result.is_ok());

        // Verify frames were written
        assert_eq!(written_frames.load(std::sync::atomic::Ordering::SeqCst), 30);
    }

    #[test_log::test]
    fn test_ref_mut_dyn_audio_write_audio_decode_flush() {
        use moosicbox_audio_decoder::AudioDecode;

        let _spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mock_writer = TrackingMockAudioWrite::new();
        let flush_count = mock_writer.flush_count.clone();

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);
        let writer_ref: &mut dyn AudioWrite = &mut *boxed_writer;

        // Call flush through AudioDecode trait on &mut dyn AudioWrite
        let result = AudioDecode::flush(&mut { writer_ref });
        assert!(result.is_ok());

        // Verify flush was called
        assert_eq!(flush_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test_log::test]
    fn test_box_dyn_audio_write_audio_decode_write_error() {
        use moosicbox_audio_decoder::AudioDecode;
        use symphonia::core::audio::Signal;
        use symphonia::core::codecs::{CODEC_TYPE_NULL, CodecParameters};
        use symphonia::core::formats::{Packet, Track};

        struct FailingWriter {
            handle: AudioHandle,
        }

        impl AudioWrite for FailingWriter {
            fn write(&mut self, _decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
                Err(AudioOutputError::StreamClosed)
            }

            fn flush(&mut self) -> Result<(), AudioOutputError> {
                Ok(())
            }

            fn handle(&self) -> AudioHandle {
                self.handle.clone()
            }
        }

        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let (tx, _rx) = flume::bounded(1);
        let mock_writer = FailingWriter {
            handle: AudioHandle::new(tx),
        };

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);

        let mut buffer: AudioBuffer<f32> = AudioBuffer::new(10, spec);
        buffer.render_reserved(Some(10));

        let packet = Packet::new_from_slice(0, 0, 0, &[]);
        let track = Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone());

        // Call decoded - should convert AudioOutputError to AudioDecodeError
        let result = AudioDecode::decoded(&mut boxed_writer, buffer, &packet, &track);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_box_dyn_audio_write_audio_decode_flush_error() {
        use moosicbox_audio_decoder::AudioDecode;

        struct FailingFlushWriter {
            handle: AudioHandle,
        }

        impl AudioWrite for FailingFlushWriter {
            fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
                Ok(decoded.frames())
            }

            fn flush(&mut self) -> Result<(), AudioOutputError> {
                Err(AudioOutputError::Interrupt)
            }

            fn handle(&self) -> AudioHandle {
                self.handle.clone()
            }
        }

        let (tx, _rx) = flume::bounded(1);
        let mock_writer = FailingFlushWriter {
            handle: AudioHandle::new(tx),
        };

        let mut boxed_writer: Box<dyn AudioWrite> = Box::new(mock_writer);

        // Call flush - should convert AudioOutputError to AudioDecodeError
        let result = AudioDecode::flush(&mut boxed_writer);
        assert!(result.is_err());
    }
}
