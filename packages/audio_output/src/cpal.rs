use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SizedSample, StreamConfig};
use rb::{RbConsumer, RbProducer, SpscRb, RB};
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
            log::debug!("\toutput: {output:?}",);
        }
        for input in device
            .supported_input_configs()
            .map_err(|_e| AudioOutputError::NoOutputs)?
        {
            log::debug!("\tinput: {input:?}",);
        }

        let name = device.name().unwrap_or("(Unknown)".into());
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

struct CpalAudioOutputImpl<T: AudioOutputSample>
where
    T: AudioOutputSample,
{
    spec: SignalSpec,
    ring_buf_producer: rb::Producer<T>,
    sample_buf: Option<SampleBuffer<T>>,
    stream: cpal::Stream,
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

        // Create a ring buffer with a capacity for up-to 200ms of audio.
        let ring_len = ((200 * config.sample_rate.0 as usize) / 1000) * num_channels;

        let ring_buf = SpscRb::new(ring_len);
        let (ring_buf_producer, ring_buf_consumer) = (ring_buf.producer(), ring_buf.consumer());

        let stream = device
            .build_output_stream(
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
            )
            .map_err(|e| {
                log::error!("Audio output stream open error: {e:?}");

                AudioOutputError::OpenStream
            })?;

        // Start the output stream.
        if let Err(err) = stream.play() {
            log::error!("Audio output stream play error: {}", err);

            return Err(AudioOutputError::PlayStream);
        }

        Ok(Self {
            spec,
            ring_buf_producer,
            stream,
            sample_buf: None,
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

        Ok(())
    }
}

#[allow(unused)]
fn list_devices(host: &Host) {
    for dv in host.output_devices().unwrap() {
        log::debug!("device: {}", dv.name().unwrap());
        for output in dv.supported_output_configs().unwrap() {
            log::debug!("\toutput: {output:?}",);
        }
        for input in dv.supported_input_configs().unwrap() {
            log::debug!("\tinput: {input:?}",);
        }
    }
}

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
        .flat_map(|devices| devices.into_iter())
        .filter_map(|device| device.try_into().ok())
}
