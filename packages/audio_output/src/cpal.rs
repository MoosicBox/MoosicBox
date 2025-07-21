#![allow(clippy::module_name_repetitions)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SizedSample, StreamConfig};
use rb::{RB, RbConsumer, RbProducer, SpscRb};
use symphonia::core::audio::{
    AudioBuffer, Channels, Layout, RawSample, SampleBuffer, Signal as _, SignalSpec,
};
use symphonia::core::conv::{ConvertibleSample, IntoSample};
use symphonia::core::units::Duration;

use crate::{
    AudioOutputError, AudioOutputFactory, AudioWrite, ProgressTracker,
    command::{AudioCommand, AudioHandle, AudioResponse, CommandMessage},
};

// Stream command types for immediate processing
#[derive(Debug, Clone)]
pub enum StreamCommand {
    Pause,
    Resume,
    Reset,
}

const INITIAL_BUFFER_SECONDS: usize = 10;

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
            "🔊 CpalAudioOutput flush called - delegating to underlying CPAL implementation"
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

    fn handle(&self) -> AudioHandle {
        self.write.handle()
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

struct CpalAudioOutputImpl<T: AudioOutputSample> {
    spec: SignalSpec,
    ring_buf_producer: rb::Producer<T>,
    sample_buf: Option<SampleBuffer<T>>,
    initial_buffering: bool,
    buffered_samples: usize,
    buffering_threshold: usize,
    consumed_samples_shared:
        std::sync::Arc<std::sync::RwLock<std::sync::Arc<std::sync::atomic::AtomicUsize>>>, // Track actual consumption by CPAL
    volume_shared: std::sync::Arc<std::sync::RwLock<std::sync::Arc<atomic_float::AtomicF64>>>, // For immediate volume changes
    total_samples_written: std::sync::Arc<std::sync::atomic::AtomicUsize>, // Track total samples written to ring buffer
    // Track the actual CPAL output sample rate for accurate progress calculation
    cpal_output_sample_rate: std::sync::Arc<std::sync::atomic::AtomicU32>,
    cpal_output_channels: std::sync::Arc<std::sync::atomic::AtomicU32>,
    completion_condvar: std::sync::Arc<std::sync::Condvar>,
    completion_mutex: std::sync::Arc<std::sync::Mutex<bool>>, // true when ring buffer is empty
    draining: std::sync::Arc<std::sync::atomic::AtomicBool>,  // true when we're in flush/drain mode
    progress_tracker: ProgressTracker,
    // Command handling
    command_receiver: Option<flume::Receiver<CommandMessage>>,
    command_handle: AudioHandle,
}

impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
    #[allow(clippy::too_many_lines)]
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

        // Create a ring buffer with a capacity for up-to 30 seconds of audio (larger buffer to prevent underruns).
        let ring_len = (30 * config.sample_rate.0 as usize) * num_channels;
        log::debug!(
            "Creating ring buffer with {} samples capacity (30 seconds at {}Hz, {} channels)",
            ring_len,
            config.sample_rate.0,
            num_channels
        );

        let ring_buf = SpscRb::new(ring_len);
        let (ring_buf_producer, ring_buf_consumer) = (ring_buf.producer(), ring_buf.consumer());

        // Create atomic counter for tracking consumed samples - wrapped in RwLock so it can be replaced
        let consumed_samples = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let consumed_samples_shared = std::sync::Arc::new(std::sync::RwLock::new(consumed_samples));
        let consumed_samples_callback = consumed_samples_shared.clone();

        // Create volume atomic for immediate volume changes - wrapped in RwLock so it can be replaced
        let volume_atomic = std::sync::Arc::new(atomic_float::AtomicF64::new(1.0));
        let volume_shared = std::sync::Arc::new(std::sync::RwLock::new(volume_atomic));
        let volume_callback = volume_shared.clone();

        // Track the actual CPAL output sample rate and channels for accurate progress calculation
        let cpal_output_sample_rate =
            std::sync::Arc::new(std::sync::atomic::AtomicU32::new(config.sample_rate.0));
        #[allow(clippy::cast_possible_truncation)]
        let cpal_output_channels =
            std::sync::Arc::new(std::sync::atomic::AtomicU32::new(num_channels as u32));

        // Event-driven ring buffer empty notification
        let (completion_mutex, completion_condvar) = (
            std::sync::Arc::new(std::sync::Mutex::new(false)),
            std::sync::Arc::new(std::sync::Condvar::new()),
        );

        // Flag to indicate we're in drain mode (flush called, no more data coming)
        let draining = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Progress tracking setup using ProgressTracker
        let progress_tracker = ProgressTracker::new(Some(0.1)); // 0.1 second threshold
        progress_tracker.set_audio_spec(config.sample_rate.0, u32::try_from(num_channels).unwrap());

        // Command handling setup
        let (command_sender, command_receiver) = flume::unbounded();
        let command_handle = AudioHandle::new(command_sender);

        // Get callback references for use in the audio callback
        let (
            progress_consumed_samples,
            progress_sample_rate,
            progress_channels,
            progress_callback,
            progress_last_position,
        ) = progress_tracker.get_callback_refs();

        let completion_mutex_callback = completion_mutex.clone();
        let completion_condvar_callback = completion_condvar.clone();
        let draining_callback = draining.clone();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    // Write out as many samples as possible from the ring buffer to the audio
                    // output.
                    let written = ring_buf_consumer.read(data).unwrap_or(0);

                    // Apply volume immediately in the CPAL callback for instant effect
                    // This bypasses the 10-15s ring buffer delay
                    let volume = volume_callback.read().map_or(1.0, |atomic| {
                        atomic.load(std::sync::atomic::Ordering::SeqCst)
                    });

                    // Apply volume to the written samples if volume is not 1.0
                    if written > 0 && volume <= 0.999 {
                        log::trace!(
                            "CPAL: applying volume to written samples - volume={volume:.3}"
                        );
                        // Apply proper volume scaling to all samples
                        for data in data.iter_mut().take(written) {
                            let original_sample: f32 = (*data).into_sample();
                            #[allow(clippy::cast_possible_truncation)]
                            let adjusted_sample = original_sample * volume as f32;

                            // Apply the volume-adjusted sample
                            *data = <T as symphonia::core::conv::FromSample<f32>>::from_sample(
                                adjusted_sample,
                            );
                        }
                    }

                    // Track actual consumption by CPAL - get the current counter
                    if let Ok(counter) = consumed_samples_callback.read() {
                        counter.fetch_add(written, std::sync::atomic::Ordering::SeqCst);

                        // Progress callback logic using ProgressTracker
                        ProgressTracker::update_from_callback_refs(
                            &progress_consumed_samples,
                            &progress_sample_rate,
                            &progress_channels,
                            &progress_callback,
                            &progress_last_position,
                            written,
                            0.1, // threshold
                        );
                    }

                    // Mute any remaining samples.
                    data[written..].iter_mut().for_each(|s| *s = T::MID);

                    // Check if we're in draining mode
                    if draining_callback.load(std::sync::atomic::Ordering::SeqCst) {
                        // Signal ring buffer empty when no data was available to read
                        if written == 0 {
                            if let Ok(mut empty_flag) = completion_mutex_callback.try_lock() {
                                if !*empty_flag {
                                    *empty_flag = true;
                                    completion_condvar_callback.notify_one();
                                }
                            }
                        }
                    }
                },
                move |err| log::error!("Audio output error: {err}"),
                None,
            )
            .map_err(|e| {
                log::error!("Audio output stream open error: {e:?}");

                AudioOutputError::OpenStream
            })?;

        // Create stream command channel for immediate event-driven processing
        let (stream_command_sender, stream_command_receiver) = flume::unbounded::<StreamCommand>();

        // Spawn dedicated thread that owns the stream for immediate command processing
        // This ensures commands work on macOS (thread safety) and eliminates polling delays
        std::thread::spawn(move || {
            log::debug!("CPAL stream control thread started");

            // Event-driven loop - immediate response to commands, no polling!
            while let Ok(command) = stream_command_receiver.recv() {
                log::trace!("CPAL stream control: processing command: {command:?}");
                match command {
                    StreamCommand::Pause => {
                        if let Err(e) = stream.pause() {
                            log::error!("Failed to pause CPAL stream: {e:?}");
                        } else {
                            log::debug!("CPAL stream paused");
                        }
                    }
                    StreamCommand::Resume => {
                        if let Err(e) = stream.play() {
                            log::error!("Failed to resume CPAL stream: {e:?}");
                        } else {
                            log::debug!("CPAL stream resumed");
                        }
                    }
                    StreamCommand::Reset => {
                        if let Err(e) = stream.pause() {
                            log::error!("Failed to reset CPAL stream: {e:?}");
                        } else {
                            log::debug!("CPAL stream reset");
                        }
                    }
                }
            }

            log::debug!("CPAL stream control thread stopped");
        });

        // Calculate buffering threshold for 10 seconds of audio (REQUIRED to prevent start truncation)
        let buffering_threshold =
            INITIAL_BUFFER_SECONDS * config.sample_rate.0 as usize * num_channels;

        // DON'T start the stream yet - wait until we have 10 seconds buffered
        log::debug!(
            "🔍 CPAL stream created but not started - buffering threshold: {} samples (10 seconds at {}Hz, {} channels)",
            buffering_threshold,
            config.sample_rate.0,
            num_channels
        );

        // Debug the actual config vs expected spec for progress calculation
        log::debug!(
            "🔍 CPAL CONFIG: actual_sample_rate={}, actual_channels={}, expected_spec_rate={}, expected_spec_channels={}",
            config.sample_rate.0,
            num_channels,
            spec.rate,
            spec.channels.count()
        );

        // Check for potential sample rate mismatch that could cause progress calculation issues
        moosicbox_assert::assert!(
            config.sample_rate.0 == spec.rate,
            "🚨 SAMPLE RATE MISMATCH: CPAL config sample rate ({}) != expected spec rate ({}) - this will cause progress calculation errors!",
            config.sample_rate.0,
            spec.rate
        );

        moosicbox_assert::assert!(
            num_channels == spec.channels.count(),
            "🚨 CHANNEL COUNT MISMATCH: CPAL config channels ({}) != expected spec channels ({}) - this will cause progress calculation errors!",
            num_channels,
            spec.channels.count()
        );

        let mut instance = Self {
            spec,
            ring_buf_producer,
            sample_buf: None,
            initial_buffering: true,
            buffered_samples: 0,
            buffering_threshold,
            consumed_samples_shared,
            volume_shared,
            total_samples_written: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            cpal_output_sample_rate,
            cpal_output_channels,
            completion_condvar,
            completion_mutex,
            draining,
            progress_tracker,
            command_receiver: Some(command_receiver),
            command_handle,
        };

        // Start the command processor task
        instance.start_command_processor(stream_command_sender);

        Ok(instance)
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
        // Stream commands are now processed immediately by the dedicated thread
        // No need for lazy processing here

        // Do nothing if there are no audio frames.
        if decoded.frames() == 0 {
            return Ok(0);
        }

        self.init_sample_buf(decoded.capacity() as Duration);
        let sample_buf = self.sample_buf.as_mut().unwrap();

        // Resampling is not required. Interleave the sample for cpal using a sample buffer.
        sample_buf.copy_interleaved_typed(&decoded);

        let mut samples = sample_buf.samples();

        let bytes = samples.len();

        // Debug sample buffer details for progress calculation troubleshooting
        log::trace!(
            "🔍 Sample buffer: decoded.frames()={}, decoded.spec.channels={}, samples.len()={}, bytes={}",
            decoded.frames(),
            decoded.spec().channels.count(),
            samples.len(),
            bytes
        );

        // Write all samples to the ring buffer.
        loop {
            match self
                .ring_buf_producer
                .write_blocking_timeout(samples, std::time::Duration::from_millis(30000)) // Increased timeout for end-of-track
            {
                Ok(Some(written)) => {
                    // Track total samples written to ring buffer
                    self.total_samples_written.fetch_add(written, std::sync::atomic::Ordering::SeqCst);

                    // Track buffered samples during initial buffering
                    if self.initial_buffering {
                        self.buffered_samples += written;
                        #[allow(clippy::cast_precision_loss)]
                        let buffered_seconds = self.buffered_samples as f32
                            / (self.spec.rate as f32 * self.spec.channels.count() as f32);

                        // Start stream once we have 10 seconds buffered OR when flush is called
                        // (which indicates we have all the available audio data)
                        if self.buffered_samples >= self.buffering_threshold {
                            log::debug!(
                                "Initial buffering complete: {buffered_seconds:.2} seconds buffered, starting stream now"
                            );

                            // Use existing command infrastructure to start the stream
                            if let Err(e) = self.command_handle.resume_immediate() {
                                log::error!("Failed to start stream: {e}");
                                return Err(AudioOutputError::StreamClosed);
                            }

                            log::debug!("Stream started successfully");

                            self.initial_buffering = false;
                        }
                    }

                    if written == samples.len() {
                        break;
                    }
                    samples = &samples[written..];
                }
                Ok(None) => {
                    // Buffer is full, wait a bit and try again
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(err) => {
                    log::error!("Ring buffer write error: {err}");
                    return Err(AudioOutputError::StreamClosed);
                }
            }
        }

        Ok(bytes)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        // Stream commands are now processed immediately by the dedicated thread
        // No need for lazy processing here

        // If there is a resampler, then it may need to be flushed
        // depending on the number of samples it has.

        // FORCE STREAM START if still in initial buffering when flush is called
        // This handles cases where total audio content is less than the buffering threshold
        // (e.g., seeking near the end of tracks)
        if self.initial_buffering {
            #[allow(clippy::cast_precision_loss)]
            let buffered_seconds = self.buffered_samples as f32
                / (self.spec.rate as f32 * self.spec.channels.count() as f32);

            log::debug!(
                "🔊 FLUSH: Stream still in initial buffering with {buffered_seconds:.2}s - forcing stream start for short audio content"
            );

            // Use existing command infrastructure to start the stream
            if let Err(e) = self.command_handle.resume_immediate() {
                log::error!("Failed to start stream for short audio: {e}");
                return Err(AudioOutputError::StreamClosed);
            }

            log::debug!("Stream started successfully for short audio content");

            self.initial_buffering = false;
        }

        let total_written = self
            .total_samples_written
            .load(std::sync::atomic::Ordering::SeqCst);

        if total_written == 0 {
            log::debug!("No samples written, skipping ring buffer drain");
        } else {
            log::debug!(
                "🔊 CPAL FLUSH: Entering drain mode and waiting for ring buffer to empty ({total_written} samples were written)"
            );

            // Set draining mode and reset the empty flag
            self.draining
                .store(true, std::sync::atomic::Ordering::SeqCst);
            if let Ok(mut empty_flag) = self.completion_mutex.lock() {
                *empty_flag = false;
            }

            let start_time = std::time::Instant::now();

            // Wait for the CPAL callback to signal ring buffer empty
            if let Ok(mut empty_flag) = self.completion_mutex.lock() {
                while !*empty_flag {
                    match self
                        .completion_condvar
                        .wait_timeout(empty_flag, std::time::Duration::from_secs(30))
                    {
                        Ok((new_flag, timeout_result)) => {
                            empty_flag = new_flag;
                            if timeout_result.timed_out() {
                                log::warn!(
                                    "⚠️ Ring buffer drain timeout after 30s - proceeding anyway"
                                );
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("Ring buffer drain wait error: {e}");
                            break;
                        }
                    }
                }
            }

            let drain_time = start_time.elapsed();
            log::debug!(
                "✅ Ring buffer drained! Wait time: {:.3}s - completing immediately to avoid silence",
                drain_time.as_secs_f64()
            );

            // Clear draining mode
            self.draining
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Stream control is now handled via commands
        log::debug!("🔊 CPAL FLUSH: All samples consumed");

        // Reset state for next track
        self.initial_buffering = true;
        self.buffered_samples = 0;
        self.total_samples_written
            .store(0, std::sync::atomic::Ordering::SeqCst);
        if let Ok(counter) = self.consumed_samples_shared.read() {
            counter.store(0, std::sync::atomic::Ordering::SeqCst);
        }

        // Reset progress tracker for next track
        self.progress_tracker.reset();

        Ok(())
    }

    fn get_playback_position(&self) -> Option<f64> {
        Some(self.get_playback_position())
    }

    fn set_consumed_samples(
        &mut self,
        consumed_samples: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let current_value = consumed_samples.load(std::sync::atomic::Ordering::SeqCst);
        log::debug!("CPAL: set_consumed_samples called with value: {current_value}");

        // Replace the existing consumed_samples counter with the new one
        if let Ok(mut counter) = self.consumed_samples_shared.write() {
            *counter = consumed_samples;
            log::debug!(
                "CPAL: consumed_samples counter replaced, preserving value: {current_value}"
            );
        } else {
            log::error!("CPAL: failed to acquire write lock for consumed_samples");
        }

        // Also update the progress tracker with the initial value
        self.progress_tracker.set_consumed_samples(current_value);
    }

    fn set_volume(&mut self, volume: f64) {
        // Set volume on the current volume atomic
        if let Ok(atomic) = self.volume_shared.read() {
            atomic.store(volume, std::sync::atomic::Ordering::SeqCst);
            log::debug!("CPAL impl: volume set to {volume}");
        } else {
            log::error!("CPAL impl: failed to acquire read lock for volume");
        }
    }

    fn set_shared_volume(&mut self, shared_volume: std::sync::Arc<atomic_float::AtomicF64>) {
        // Replace the volume atomic with the shared one
        if let Ok(mut atomic) = self.volume_shared.write() {
            let old_volume = atomic.load(std::sync::atomic::Ordering::SeqCst);
            let new_volume = shared_volume.load(std::sync::atomic::Ordering::SeqCst);
            *atomic = shared_volume;
            log::info!(
                "CPAL impl: shared volume reference set - old volume: {old_volume:.3}, new volume: {new_volume:.3}"
            );
        } else {
            log::error!("CPAL impl: failed to acquire write lock for shared volume");
        }
    }

    fn get_output_spec(&self) -> Option<symphonia::core::audio::SignalSpec> {
        Some(self.get_output_audio_spec())
    }

    fn set_progress_callback(
        &mut self,
        callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
        self.progress_tracker.set_callback(callback);
    }

    fn handle(&self) -> AudioHandle {
        self.command_handle.clone()
    }
}

impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
    /// Get the actual playback position in seconds based on consumed samples
    pub fn get_playback_position(&self) -> f64 {
        self.progress_tracker.get_position().unwrap_or(0.0)
    }

    /// Get the actual output sample rate (not the input sample rate)
    pub fn get_output_sample_rate(&self) -> u32 {
        self.cpal_output_sample_rate
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get the actual output channel count
    pub fn get_output_channels(&self) -> u32 {
        self.cpal_output_channels
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get the actual output audio specification
    pub fn get_output_audio_spec(&self) -> symphonia::core::audio::SignalSpec {
        let rate = self.get_output_sample_rate();
        let channels = self.get_output_channels();

        // Create channels based on channel count
        let symphonia_channels = match channels {
            1 => symphonia::core::audio::Layout::Mono.into_channels(),
            2 => symphonia::core::audio::Layout::Stereo.into_channels(),
            // For other channel counts, use the spec field channels as fallback
            _ => self.spec.channels,
        };

        symphonia::core::audio::SignalSpec {
            rate,
            channels: symphonia_channels,
        }
    }

    fn start_command_processor(&mut self, stream_command_sender: flume::Sender<StreamCommand>) {
        if let Some(command_receiver) = self.command_receiver.take() {
            let volume_shared = self.volume_shared.clone();

            moosicbox_task::spawn("cpal_command_processor", async move {
                while let Ok(command_msg) = command_receiver.recv_async().await {
                    let response = Self::process_command(
                        &command_msg.command,
                        &volume_shared,
                        &stream_command_sender,
                    );

                    // Send response if requested
                    if let Some(response_sender) = command_msg.response_sender {
                        let _ = response_sender.send_async(response.clone()).await;
                    }
                }
            });
        }
    }

    fn process_command(
        command: &AudioCommand,
        volume_shared: &std::sync::Arc<std::sync::RwLock<std::sync::Arc<atomic_float::AtomicF64>>>,
        stream_command_sender: &flume::Sender<StreamCommand>,
    ) -> AudioResponse {
        match command {
            AudioCommand::SetVolume(volume) => volume_shared.read().map_or_else(
                |_| AudioResponse::Error("Failed to set volume".to_string()),
                |atomic| {
                    atomic.store(*volume, std::sync::atomic::Ordering::SeqCst);
                    log::debug!("CPAL command processor: volume set to {volume}");
                    AudioResponse::Success
                },
            ),
            AudioCommand::Pause => match stream_command_sender.try_send(StreamCommand::Pause) {
                Ok(()) => {
                    log::debug!("CPAL command processor: sent pause command");
                    AudioResponse::Success
                }
                Err(e) => {
                    log::error!("Failed to send pause command: {e}");
                    AudioResponse::Error("Failed to send pause command".to_string())
                }
            },
            AudioCommand::Resume => match stream_command_sender.try_send(StreamCommand::Resume) {
                Ok(()) => {
                    log::debug!("CPAL command processor: sent resume command");
                    AudioResponse::Success
                }
                Err(e) => {
                    log::error!("Failed to send resume command: {e}");
                    AudioResponse::Error("Failed to send resume command".to_string())
                }
            },
            AudioCommand::Seek(_position) => {
                // Seeking would require coordination with the audio decoder
                // For now, return an error as this needs to be implemented at a higher level
                AudioResponse::Error("Seek not implemented at CPAL level".to_string())
            }

            AudioCommand::Flush => {
                // Flush would need to coordinate with the main audio thread
                // For now, just return success
                log::debug!("CPAL command processor: flush requested");
                AudioResponse::Success
            }
            AudioCommand::Reset => match stream_command_sender.try_send(StreamCommand::Reset) {
                Ok(()) => {
                    log::debug!("CPAL command processor: sent reset command");
                    AudioResponse::Success
                }
                Err(e) => {
                    log::error!("Failed to send reset command: {e}");
                    AudioResponse::Error("Failed to send reset command".to_string())
                }
            },
        }
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
