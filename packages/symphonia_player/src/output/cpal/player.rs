use crate::resampler::Resampler;

use crate::output::{AudioOutput, AudioOutputError};

use cpal::{Host, SizedSample};
use rb::{RbConsumer, RbProducer, SpscRb, RB};
use symphonia::core::audio::{AudioBuffer, RawSample, SampleBuffer, Signal as _, SignalSpec};
use symphonia::core::conv::{ConvertibleSample, IntoSample};
use symphonia::core::units::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct CpalAudioOutput;

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
    pub fn try_open(
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
        // Get default host.
        let host = cpal::default_host();

        // Get the default audio output device.
        let device = match host.default_output_device() {
            Some(device) => device,
            _ => {
                log::error!("Failed to get default audio output device");
                return Err(AudioOutputError::OpenStream);
            }
        };

        log::debug!(
            "Using default audio device: {}",
            device.name().unwrap_or("(Unknown)".to_string())
        );

        for output in device.supported_output_configs().unwrap() {
            println!("\toutput: {output:?}",);
        }
        for input in device.supported_input_configs().unwrap() {
            println!("\tinput: {input:?}",);
        }

        let config = match device.default_output_config() {
            Ok(config) => config,
            Err(err) => {
                log::error!("Failed to get default audio output device config: {}", err);
                return Err(AudioOutputError::OpenStream);
            }
        };

        println!("Using default output: {config:?} with spec {spec:?}");

        // Select proper playback routine based on sample format.
        match config.sample_format() {
            cpal::SampleFormat::F32 => {
                CpalAudioOutputImpl::<f32>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::I16 => {
                CpalAudioOutputImpl::<i16>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::U16 => {
                CpalAudioOutputImpl::<u16>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::I8 => CpalAudioOutputImpl::<i8>::try_open(spec, duration, &device),
            cpal::SampleFormat::I32 => {
                CpalAudioOutputImpl::<i32>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::I64 => {
                CpalAudioOutputImpl::<i32>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::U8 => CpalAudioOutputImpl::<u8>::try_open(spec, duration, &device),
            cpal::SampleFormat::U32 => {
                CpalAudioOutputImpl::<u32>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::U64 => {
                CpalAudioOutputImpl::<u32>::try_open(spec, duration, &device)
            }
            cpal::SampleFormat::F64 => {
                CpalAudioOutputImpl::<f64>::try_open(spec, duration, &device)
            }
            _ => unreachable!(),
        }
    }
}

struct CpalAudioOutputImpl<T: AudioOutputSample>
where
    T: AudioOutputSample,
{
    ring_buf_producer: rb::Producer<T>,
    sample_buf: SampleBuffer<T>,
    stream: cpal::Stream,
    resampler: Option<Resampler<T>>,
}

impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
    pub fn try_open(
        spec: SignalSpec,
        duration: Duration,
        device: &cpal::Device,
    ) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
        let num_channels = spec.channels.count();

        // Output audio stream config.
        let config = if cfg!(not(target_os = "windows")) {
            cpal::StreamConfig {
                channels: num_channels as cpal::ChannelCount,
                sample_rate: cpal::SampleRate(spec.rate),
                buffer_size: cpal::BufferSize::Default,
            }
        } else {
            // Use the default config for Windows.
            device
                .default_output_config()
                .expect("Failed to get the default output config.")
                .config()
        };

        println!("Using default output: {config:?} with spec {spec:?}");

        // Create a ring buffer with a capacity for up-to 200ms of audio.
        let ring_len = ((200 * config.sample_rate.0 as usize) / 1000) * num_channels;

        let ring_buf = SpscRb::new(ring_len);
        let (ring_buf_producer, ring_buf_consumer) = (ring_buf.producer(), ring_buf.consumer());

        let stream_result = device.build_output_stream(
            &config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                // Write out as many samples as possible from the ring buffer to the audio
                // output.
                let written = ring_buf_consumer.read(data).unwrap_or(0);

                // Mute any remaining samples.
                data[written..].iter_mut().for_each(|s| *s = T::MID);
            },
            move |err| log::error!("Audio output error: {}", err),
            None,
        );

        if let Err(err) = stream_result {
            log::error!("Audio output stream open error: {}", err);

            return Err(AudioOutputError::OpenStream);
        }

        let stream = stream_result.unwrap();

        // Start the output stream.
        if let Err(err) = stream.play() {
            log::error!("Audio output stream play error: {}", err);

            return Err(AudioOutputError::PlayStream);
        }

        let sample_buf = SampleBuffer::<T>::new(duration, spec);

        let resampler = if spec.rate != config.sample_rate.0 {
            log::info!("Resampling {} Hz to {} Hz", spec.rate, config.sample_rate.0);
            Some(Resampler::new(
                spec,
                config.sample_rate.0 as usize,
                duration,
            ))
        } else {
            None
        };

        Ok(Box::new(CpalAudioOutputImpl {
            ring_buf_producer,
            sample_buf,
            stream,
            resampler,
        }))
    }
}

impl<T: AudioOutputSample> AudioOutput for CpalAudioOutputImpl<T> {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        // Do nothing if there are no audio frames.
        if decoded.frames() == 0 {
            return Ok(0);
        }

        let mut samples = if let Some(resampler) = &mut self.resampler {
            // Resampling is required. The resampler will return interleaved samples in the
            // correct sample format.
            match resampler.resample(decoded) {
                Some(resampled) => resampled,
                None => return Ok(0),
            }
        } else {
            // Resampling is not required. Interleave the sample for cpal using a sample buffer.
            self.sample_buf.copy_interleaved_typed(&decoded);

            self.sample_buf.samples()
        };

        let bytes = samples.len();

        // Write all samples to the ring buffer.
        loop {
            match self
                .ring_buf_producer
                .write_blocking_timeout(samples, std::time::Duration::from_millis(5000))
            {
                Ok(Some(written)) => {
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
        if let Some(resampler) = &mut self.resampler {
            let mut remaining_samples = resampler.flush().unwrap_or_default();

            while let Some(written) = self.ring_buf_producer.write_blocking(remaining_samples) {
                remaining_samples = &remaining_samples[written..];
            }
        }

        // Flush is best-effort, ignore the returned result.
        let _ = self.stream.pause();

        Ok(())
    }
}

#[allow(unused)]
fn list_devices(host: &Host) {
    for dv in host.output_devices().unwrap() {
        println!("device: {}", dv.name().unwrap());
        for output in dv.supported_output_configs().unwrap() {
            println!("\toutput: {output:?}",);
        }
        for input in dv.supported_input_configs().unwrap() {
            println!("\tinput: {input:?}",);
        }
    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    CpalAudioOutput::try_open(spec, duration)
}
