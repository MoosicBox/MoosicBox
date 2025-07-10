#![allow(clippy::module_name_repetitions)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SizedSample, StreamConfig};
use rb::{RB, RbConsumer, RbProducer, SpscRb};
use symphonia::core::audio::{
    AudioBuffer, Channels, Layout, RawSample, SampleBuffer, Signal as _, SignalSpec,
};
use symphonia::core::conv::{ConvertibleSample, IntoSample};
use symphonia::core::units::Duration;

use crate::{AudioOutputError, AudioOutputFactory, AudioWrite};

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
        self.write.flush()
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
    stream: cpal::Stream,
    initial_buffering: bool,
    buffered_samples: usize,
    buffering_threshold: usize, // 10 seconds worth of samples
    consumed_samples_shared:
        std::sync::Arc<std::sync::RwLock<std::sync::Arc<std::sync::atomic::AtomicUsize>>>, // Track actual consumption by CPAL
    volume_shared: std::sync::Arc<std::sync::RwLock<std::sync::Arc<atomic_float::AtomicF64>>>, // For immediate volume changes
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

        // Create a ring buffer with a capacity for up-to 15 seconds of audio (need more than 10s for buffering).
        let ring_len = (15 * config.sample_rate.0 as usize) * num_channels;
        log::debug!(
            "Creating ring buffer with {} samples capacity (15 seconds at {}Hz, {} channels)",
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
                        log::debug!(
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
                        let old_value =
                            counter.fetch_add(written, std::sync::atomic::Ordering::SeqCst);
                        // Only log the first few times to avoid spam
                        if written > 0 && old_value < 10000 {
                            log::debug!(
                                "CPAL callback: wrote {written} samples, total consumed: {}",
                                old_value + written
                            );
                        }
                    }

                    // Mute any remaining samples.
                    data[written..].iter_mut().for_each(|s| *s = T::MID);
                },
                move |err| log::error!("Audio output error: {err}"),
                None,
            )
            .map_err(|e| {
                log::error!("Audio output stream open error: {e:?}");

                AudioOutputError::OpenStream
            })?;

        // Calculate buffering threshold for 10 seconds of audio
        let buffering_threshold = 10 * config.sample_rate.0 as usize * num_channels;

        // DON'T start the stream yet - wait until we have 10 seconds buffered
        log::debug!(
            "CPAL stream created but not started - buffering threshold: {} samples (10 seconds at {}Hz, {} channels)",
            buffering_threshold,
            config.sample_rate.0,
            num_channels
        );

        Ok(Self {
            spec,
            ring_buf_producer,
            stream,
            sample_buf: None,
            initial_buffering: true,
            buffered_samples: 0,
            buffering_threshold,
            consumed_samples_shared,
            volume_shared,
        })
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

        // Resampling is not required. Interleave the sample for cpal using a sample buffer.
        sample_buf.copy_interleaved_typed(&decoded);

        let mut samples = sample_buf.samples();

        let bytes = samples.len();

        // Write all samples to the ring buffer.
        loop {
            match self
                .ring_buf_producer
                .write_blocking_timeout(samples, std::time::Duration::from_millis(5000))
            {
                Ok(Some(written)) => {
                    // Track buffered samples during initial buffering
                    if self.initial_buffering {
                        self.buffered_samples += written;
                        #[allow(clippy::cast_precision_loss)]
                        let buffered_seconds = self.buffered_samples as f32
                            / (self.spec.rate as f32 * self.spec.channels.count() as f32);

                        // Log progress every 2 seconds
                        if self.buffered_samples
                            % (2 * self.spec.rate as usize * self.spec.channels.count())
                            == 0
                        {
                            log::debug!(
                                "Buffering progress: {buffered_seconds:.2} seconds buffered"
                            );
                        }

                        // Start stream once we have 10 seconds buffered
                        if self.buffered_samples >= self.buffering_threshold {
                            log::debug!(
                                "Initial buffering complete: {buffered_seconds:.2} seconds buffered, starting stream now"
                            );
                            if let Err(err) = self.stream.play() {
                                log::error!("Audio output stream play error: {err}");
                                return Err(AudioOutputError::PlayStream);
                            }
                            self.initial_buffering = false;
                        }
                    }

                    samples = &samples[written..];
                }
                Ok(None) => break,
                Err(_err) => return Err(AudioOutputError::Interrupt),
            }
        }

        Ok(bytes)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        // If there is a resampler, then it may need to be flushed
        // depending on the number of samples it has.

        // Flush is best-effort, ignore the returned result.
        let _ = self.stream.pause();

        // Reset buffering state for next track
        self.initial_buffering = true;
        self.buffered_samples = 0;
        if let Ok(counter) = self.consumed_samples_shared.read() {
            counter.store(0, std::sync::atomic::Ordering::SeqCst);
        }

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
            // DON'T reset the counter - preserve whatever value was already in it
            log::debug!(
                "CPAL: consumed_samples counter replaced, preserving value: {current_value}"
            );
        } else {
            log::error!("CPAL: failed to acquire write lock for consumed_samples");
        }
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
}

impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
    /// Get the actual playback position in seconds based on consumed samples
    pub fn get_playback_position(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        self.consumed_samples_shared.read().map_or(0.0, |counter| {
            let consumed_samples = counter.load(std::sync::atomic::Ordering::SeqCst);
            consumed_samples as f64
                / (f64::from(self.spec.rate) * self.spec.channels.count() as f64)
        })
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
