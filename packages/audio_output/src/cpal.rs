#![allow(clippy::module_name_repetitions)]

use atomic_float::AtomicF64;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SizedSample, StreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use symphonia::core::audio::{
    AudioBuffer, Channels, Layout, RawSample, SampleBuffer, Signal as _, SignalSpec,
};
use symphonia::core::conv::{ConvertibleSample, IntoSample};
use symphonia::core::units::Duration;

use crate::{AudioOutputError, AudioOutputFactory, AudioWrite, ProgressTracker};

/// Small buffer size in seconds with minimal initial buffering (vs previous 30-second buffer with 10s initial buffering)
const BUFFER_SECONDS: usize = 2;

pub struct CpalAudioOutput {
    #[allow(unused)]
    device: cpal::Device,
    write: Box<dyn AudioWrite>,
}

impl AudioWrite for CpalAudioOutput {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        self.write.write(decoded)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        log::debug!(
            "🔊 CpalAudioOutput flush called - delegating to simplified CPAL implementation"
        );
        let result = self.write.flush();
        log::debug!("🔊 CpalAudioOutput flush completed - result: {result:?}");
        result
    }

    fn get_playback_position(&self) -> Option<f64> {
        self.write.get_playback_position()
    }

    fn set_consumed_samples(
        &mut self,
        consumed_samples: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        self.write.set_consumed_samples(consumed_samples);
    }

    fn set_volume(&mut self, volume: f64) {
        self.write.set_volume(volume);
    }

    fn set_shared_volume(&mut self, shared_volume: std::sync::Arc<atomic_float::AtomicF64>) {
        self.write.set_shared_volume(shared_volume);
    }

    fn get_output_spec(&self) -> Option<SignalSpec> {
        self.write.get_output_spec()
    }

    fn set_progress_callback(
        &mut self,
        callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
        self.write.set_progress_callback(callback);
    }
}

trait AudioOutputSample:
    cpal::Sample
    + ConvertibleSample
    + SizedSample
    + IntoSample<f32>
    + RawSample
    + std::marker::Send
    + 'static
{
}

impl AudioOutputSample for f32 {}
impl AudioOutputSample for i16 {}
impl AudioOutputSample for u16 {}
impl AudioOutputSample for i8 {}
impl AudioOutputSample for i32 {}
impl AudioOutputSample for u8 {}
impl AudioOutputSample for u32 {}
impl AudioOutputSample for f64 {}

impl CpalAudioOutput {
    /// # Errors
    ///
    /// * If the relevant `CpalAudioOutputImpl` fails to initialize
    pub fn new(device: cpal::Device, format: SampleFormat) -> Result<Self, AudioOutputError> {
        Ok(Self {
            write: match format {
                cpal::SampleFormat::F32 => Box::new(CpalAudioOutputImpl::<f32>::new(&device)?),
                cpal::SampleFormat::I16 => Box::new(CpalAudioOutputImpl::<i16>::new(&device)?),
                cpal::SampleFormat::U16 => Box::new(CpalAudioOutputImpl::<u16>::new(&device)?),
                cpal::SampleFormat::I8 => Box::new(CpalAudioOutputImpl::<i8>::new(&device)?),
                cpal::SampleFormat::I32 => Box::new(CpalAudioOutputImpl::<i32>::new(&device)?),
                cpal::SampleFormat::I64 => Box::new(CpalAudioOutputImpl::<i32>::new(&device)?),
                cpal::SampleFormat::U8 => Box::new(CpalAudioOutputImpl::<u8>::new(&device)?),
                cpal::SampleFormat::U32 => Box::new(CpalAudioOutputImpl::<u32>::new(&device)?),
                cpal::SampleFormat::U64 => Box::new(CpalAudioOutputImpl::<u32>::new(&device)?),
                cpal::SampleFormat::F64 => Box::new(CpalAudioOutputImpl::<f64>::new(&device)?),
                _ => unreachable!(),
            },
            device,
        })
    }
}

impl TryFrom<Device> for AudioOutputFactory {
    type Error = AudioOutputError;

    fn try_from(device: Device) -> Result<Self, Self::Error> {
        for output in device
            .supported_output_configs()
            .map_err(|_e| AudioOutputError::NoOutputs)?
        {
            log::trace!("\toutput: {output:?}",);
        }
        for input in device
            .supported_input_configs()
            .map_err(|_e| AudioOutputError::NoOutputs)?
        {
            log::trace!("\tinput: {input:?}",);
        }

        let name = device.name().unwrap_or_else(|_| "(Unknown)".into());
        let config = device
            .default_output_config()
            .map_err(|_e| AudioOutputError::NoOutputs)?;
        let spec = SignalSpec {
            rate: config.sample_rate().0,
            channels: Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        };

        let id = format!("cpal:{name}");

        Ok(Self::new(id, name, spec, move || {
            let format = config.sample_format();
            Ok(Box::new(CpalAudioOutput::new(device.clone(), format)?))
        }))
    }
}

/// Shared state between main thread and audio callback - much simpler than the previous version
struct SharedAudioState<T> {
    /// Small buffer for pending audio data (2 seconds max vs previous 30 seconds)
    buffer: Mutex<VecDeque<T>>,
    /// Condition variable to signal when buffer has space (backpressure)
    space_available: Condvar,

    /// Volume control (immediate effect, no bypass needed) - wrapped in Mutex for replacement
    volume: Mutex<Arc<atomic_float::AtomicF64>>,

    /// Consumed samples tracking - wrapped in Mutex for replacement
    consumed_samples: Mutex<Arc<AtomicUsize>>,

    /// Stream control
    stream_started: AtomicBool,
    end_of_stream: AtomicBool,

    /// Progress tracker references for audio callback updates
    progress_consumed_samples: Arc<AtomicUsize>,
    progress_sample_rate: Arc<AtomicU32>,
    progress_channels: Arc<AtomicU32>,
    #[allow(clippy::type_complexity)]
    progress_callback: Arc<RwLock<Option<Box<dyn Fn(f64) + Send + Sync + 'static>>>>,
    progress_last_reported_position: Arc<AtomicF64>,
    progress_threshold: f64,
}

/// Simplified CPAL implementation with small buffer and backpressure
struct CpalAudioOutputImpl<T: AudioOutputSample> {
    spec: SignalSpec,
    stream: cpal::Stream,
    sample_buf: Option<SampleBuffer<T>>,

    /// Shared state between main thread and audio callback
    shared_state: Arc<SharedAudioState<T>>,

    /// Progress tracking
    progress_tracker: ProgressTracker,
}

impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
    pub fn new(device: &cpal::Device) -> Result<Self, AudioOutputError> {
        let config = device
            .default_output_config()
            .map_err(|_e| AudioOutputError::UnsupportedOutputConfiguration)?
            .config();

        log::debug!("Got default config: {config:?}");

        let num_channels = config.channels as usize;

        let config = if num_channels <= 2 {
            config
        } else {
            StreamConfig {
                channels: 2,
                sample_rate: config.sample_rate,
                buffer_size: cpal::BufferSize::Default,
            }
        };

        let spec = SignalSpec {
            rate: config.sample_rate.0,
            channels: if num_channels >= 2 {
                Layout::Stereo.into_channels()
            } else {
                Layout::Mono.into_channels()
            },
        };

        // Create shared state with small buffer (2 seconds max vs previous 30 seconds)
        let buffer_capacity = (BUFFER_SECONDS * config.sample_rate.0 as usize) * num_channels;
        log::debug!(
            "Creating small buffer with {} samples capacity ({} seconds at {}Hz, {} channels)",
            buffer_capacity,
            BUFFER_SECONDS,
            config.sample_rate.0,
            num_channels
        );

        // Setup progress tracking
        let progress_tracker = ProgressTracker::new(Some(0.1));
        progress_tracker.set_audio_spec(config.sample_rate.0, u32::try_from(num_channels).unwrap());

        // Get progress tracker references for audio callback
        let (
            progress_consumed_samples,
            progress_sample_rate,
            progress_channels,
            progress_callback,
            progress_last_reported_position,
        ) = progress_tracker.get_callback_refs();

        let shared_state = Arc::new(SharedAudioState {
            buffer: Mutex::new(VecDeque::with_capacity(buffer_capacity)),
            space_available: Condvar::new(),
            volume: Mutex::new(Arc::new(atomic_float::AtomicF64::new(1.0))),
            consumed_samples: Mutex::new(Arc::new(AtomicUsize::new(0))),
            stream_started: AtomicBool::new(false),
            end_of_stream: AtomicBool::new(false),
            progress_consumed_samples,
            progress_sample_rate,
            progress_channels,
            progress_callback,
            progress_last_reported_position,
            progress_threshold: 0.1,
        });

        let callback_state = shared_state.clone();

        // Create CPAL stream with callback
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    Self::audio_callback(data, &callback_state);
                },
                move |err| log::error!("Audio output error: {err}"),
                None,
            )
            .map_err(|e| {
                log::error!("Audio output stream open error: {e:?}");
                AudioOutputError::OpenStream
            })?;

        log::debug!(
            "✅ CPAL simplified implementation initialized - buffer: {buffer_capacity} samples ({BUFFER_SECONDS} seconds)"
        );

        Ok(Self {
            spec,
            stream,
            sample_buf: None,
            shared_state,
            progress_tracker,
        })
    }

    /// Audio callback - runs in real-time audio thread
    /// Much simpler than the previous version with no complex completion tracking
    fn audio_callback(output: &mut [T], state: &SharedAudioState<T>) {
        let Ok(mut buffer) = state.buffer.lock() else {
            // On poison error, fill with silence
            output.iter_mut().for_each(|s| *s = T::MID);
            return;
        };

        // Read samples from buffer
        let samples_to_read = std::cmp::min(output.len(), buffer.len());

        for output_sample in output.iter_mut().take(samples_to_read) {
            if let Some(sample) = buffer.pop_front() {
                *output_sample = sample;
            } else {
                *output_sample = T::MID;
            }
        }

        // Apply volume immediately (no bypass needed like in the old implementation)
        if let Ok(volume_ref) = state.volume.lock() {
            #[allow(clippy::cast_possible_truncation)]
            let volume = volume_ref.load(Ordering::Relaxed) as f32;
            if volume < 0.999 && samples_to_read > 0 {
                log::trace!("CPAL: applying volume {volume:.3} to {samples_to_read} samples");
                for sample in &mut output[..samples_to_read] {
                    let original: f32 = (*sample).into_sample();
                    let adjusted = original * volume;
                    *sample = <T as symphonia::core::conv::FromSample<f32>>::from_sample(adjusted);
                }
            }
        }

        // Fill remaining with silence
        output[samples_to_read..]
            .iter_mut()
            .for_each(|s| *s = T::MID);

        // Update consumed samples (simple atomic increment)
        if let Ok(consumed_ref) = state.consumed_samples.lock() {
            consumed_ref.fetch_add(samples_to_read, Ordering::Relaxed);
        }

        // Update progress tracker with consumed samples
        ProgressTracker::update_from_callback_refs(
            &state.progress_consumed_samples,
            &state.progress_sample_rate,
            &state.progress_channels,
            &state.progress_callback,
            &state.progress_last_reported_position,
            samples_to_read,
            state.progress_threshold,
        );

        // Signal that space is available for more data (backpressure mechanism)
        state.space_available.notify_one();
    }

    fn init_sample_buf(&mut self, duration: Duration) -> &mut SampleBuffer<T> {
        if self.sample_buf.is_none() {
            let spec = self.spec;
            let sample_buf = SampleBuffer::<T>::new(duration, spec);
            self.sample_buf = Some(sample_buf);
        }
        self.sample_buf.as_mut().unwrap()
    }
}

impl<T: AudioOutputSample> AudioWrite for CpalAudioOutputImpl<T> {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        // Do nothing if there are no audio frames.
        if decoded.frames() == 0 {
            return Ok(0);
        }

        self.init_sample_buf(decoded.capacity() as Duration);
        let sample_buf = self.sample_buf.as_mut().unwrap();

        // Convert to interleaved samples
        sample_buf.copy_interleaved_typed(&decoded);
        let samples = sample_buf.samples();
        let bytes = samples.len();

        log::trace!(
            "🔍 Writing {} samples to small buffer (decoded.frames()={}, channels={})",
            samples.len(),
            decoded.frames(),
            decoded.spec().channels.count()
        );

        // Write samples to buffer with backpressure (key improvement over ring buffer)
        let mut buffer = self
            .shared_state
            .buffer
            .lock()
            .map_err(|_| AudioOutputError::StreamClosed)?;

        let buffer_capacity = buffer.capacity();

        // Wait if buffer is full (backpressure mechanism prevents overflow)
        // Use longer timeout to prevent writers from being dropped prematurely
        while buffer.len() + samples.len() > buffer_capacity {
            log::trace!("Buffer full, waiting for space (backpressure)...");
            buffer = self
                .shared_state
                .space_available
                .wait_timeout(buffer, std::time::Duration::from_millis(5000))
                .map_err(|_| AudioOutputError::StreamClosed)?
                .0;
        }

        // Add samples to buffer
        buffer.extend(samples.iter().copied());

        // Start stream after minimal initial buffering (0.5 seconds vs old 10 seconds)
        // This prevents both initial truncation and writer dropping due to immediate stream start
        let should_start_stream = if self.shared_state.stream_started.load(Ordering::Relaxed) {
            false
        } else {
            let sample_rate = self.spec.rate as usize;
            let channels = self.spec.channels.count();
            let min_buffer_samples = (sample_rate * channels) / 2; // 0.5 seconds

            if buffer.len() >= min_buffer_samples {
                log::debug!(
                    "🔊 Starting CPAL stream after minimal buffering ({} samples, 0.5s)",
                    buffer.len()
                );
                true
            } else {
                log::trace!(
                    "🔊 Building initial buffer: {}/{} samples (waiting for 0.5s)",
                    buffer.len(),
                    min_buffer_samples
                );
                false
            }
        };

        drop(buffer); // Release lock before potential stream operation

        if should_start_stream {
            self.stream
                .play()
                .map_err(|_| AudioOutputError::PlayStream)?;
            self.shared_state
                .stream_started
                .store(true, Ordering::Relaxed);
        }

        log::trace!("✅ Successfully wrote {} samples to buffer", samples.len());

        Ok(bytes)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        log::debug!("🔊 CPAL flush: waiting for buffer to empty");

        // Set end of stream flag
        self.shared_state
            .end_of_stream
            .store(true, Ordering::Relaxed);

        // Wait for buffer to empty (much simpler than the old completion tracking)
        let mut buffer = self
            .shared_state
            .buffer
            .lock()
            .map_err(|_| AudioOutputError::StreamClosed)?;

        while !buffer.is_empty() {
            log::trace!("Buffer has {} samples remaining", buffer.len());
            buffer = self
                .shared_state
                .space_available
                .wait_timeout(buffer, std::time::Duration::from_millis(100))
                .map_err(|_| AudioOutputError::StreamClosed)?
                .0;
        }
        drop(buffer);

        log::debug!("🔊 Buffer empty, pausing stream");

        // Pause stream
        let _ = self.stream.pause();

        // Reset state (much simpler than the old version)
        self.shared_state
            .end_of_stream
            .store(false, Ordering::Relaxed);
        self.shared_state
            .stream_started
            .store(false, Ordering::Relaxed);
        if let Ok(consumed_ref) = self.shared_state.consumed_samples.lock() {
            consumed_ref.store(0, Ordering::Relaxed);
        }
        self.progress_tracker.reset();

        log::debug!("🔊 CPAL flush completed");

        Ok(())
    }

    fn get_playback_position(&self) -> Option<f64> {
        self.progress_tracker.get_position()
    }

    fn set_consumed_samples(&mut self, consumed_samples: Arc<AtomicUsize>) {
        let current_value = consumed_samples.load(Ordering::Relaxed);
        log::debug!("CPAL: set_consumed_samples called with value: {current_value}");

        // Replace the atomic reference (much simpler than the old RwLock wrapper)
        if let Ok(mut consumed_ref) = self.shared_state.consumed_samples.lock() {
            *consumed_ref = consumed_samples;
            consumed_ref.store(current_value, Ordering::Relaxed);
        }
        self.progress_tracker.set_consumed_samples(current_value);
    }

    fn set_volume(&mut self, volume: f64) {
        // Set volume directly (no delay like in the old implementation)
        if let Ok(volume_ref) = self.shared_state.volume.lock() {
            volume_ref.store(volume, Ordering::Relaxed);
        }
        log::debug!("CPAL: volume set to {volume} (immediate effect)");
    }

    fn set_shared_volume(&mut self, shared_volume: Arc<atomic_float::AtomicF64>) {
        // Replace the volume atomic reference (much simpler than the old RwLock wrapper)
        if let Ok(mut volume_ref) = self.shared_state.volume.lock() {
            let old_volume = volume_ref.load(Ordering::Relaxed);
            let new_volume = shared_volume.load(Ordering::Relaxed);
            *volume_ref = shared_volume;
            log::info!(
                "CPAL: shared volume reference set - old: {old_volume:.3}, new: {new_volume:.3} (immediate effect)"
            );
        }
    }

    fn get_output_spec(&self) -> Option<SignalSpec> {
        Some(self.spec)
    }

    fn set_progress_callback(
        &mut self,
        callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
        self.progress_tracker.set_callback(callback);
    }
}

#[allow(unused)]
fn list_devices(host: &Host) {
    for dv in host.output_devices().unwrap() {
        log::debug!("device: {}", dv.name().unwrap());
        for output in dv.supported_output_configs().unwrap() {
            log::trace!("\toutput: {output:?}",);
        }
        for input in dv.supported_input_configs().unwrap() {
            log::trace!("\tinput: {input:?}",);
        }
    }
}

#[must_use]
pub fn scan_default_output() -> Option<AudioOutputFactory> {
    cpal::default_host()
        .default_output_device()
        .and_then(|x| x.try_into().ok())
}

pub fn scan_available_outputs() -> impl Iterator<Item = AudioOutputFactory> {
    cpal::ALL_HOSTS
        .iter()
        .filter_map(|id| cpal::host_from_id(*id).ok())
        .filter_map(|host| host.devices().ok())
        .flat_map(IntoIterator::into_iter)
        .filter_map(|device| device.try_into().ok())
}
