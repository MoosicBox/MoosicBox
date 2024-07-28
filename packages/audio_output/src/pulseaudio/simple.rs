use std::time::SystemTime;

use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;
use moosicbox_env_utils::option_env_u32;
use symphonia::core::audio::SignalSpec;
use symphonia::core::audio::*;
use symphonia::core::units::Duration;

use crate::{
    pulseaudio::common::map_channels_to_pa_channelmap, AudioOutputError, AudioOutputFactory,
    AudioWrite,
};

static SAMPLE_RATE: Option<u32> = option_env_u32!("PULSEAUDIO_RESAMPLE_RATE");

pub struct PulseAudioOutput {
    spec: SignalSpec,
    pa: psimple::Simple,
    sample_buf: Option<RawSampleBuffer<f32>>,
}

impl PulseAudioOutput {
    pub fn try_open(spec: SignalSpec) -> Result<PulseAudioOutput, AudioOutputError> {
        // Create a PulseAudio stream specification.
        let pa_spec = pulse::sample::Spec {
            format: pulse::sample::Format::FLOAT32NE,
            channels: spec.channels.count() as u8,
            rate: spec.rate,
        };

        moosicbox_assert::assert!(pa_spec.is_valid());

        let pa_ch_map = map_channels_to_pa_channelmap(spec.channels);

        // Create a PulseAudio connection.
        let pa_result = psimple::Simple::new(
            None,                               // Use default server
            "Symphonia Player",                 // Application name
            pulse::stream::Direction::Playback, // Playback stream
            None,                               // Default playback device
            "Music",                            // Description of the stream
            &pa_spec,                           // Signal specification
            pa_ch_map.as_ref(),                 // Channel map
            None,                               // Custom buffering attributes
        );

        match pa_result {
            Ok(pa) => Ok(PulseAudioOutput {
                spec,
                pa,
                sample_buf: None,
            }),
            Err(err) => {
                log::error!("audio output stream open error: {}", err);

                Err(AudioOutputError::OpenStream)
            }
        }
    }

    fn init_sample_buf(&mut self, duration: Duration) -> &mut RawSampleBuffer<f32> {
        if self.sample_buf.is_none() {
            let spec = self.spec;
            // An interleaved buffer is required to send data to PulseAudio. Use a SampleBuffer to
            // move data between Symphonia AudioBuffers and the byte buffers required by PulseAudio.
            let sample_buf = RawSampleBuffer::<f32>::new(duration, spec);
            self.sample_buf = Some(sample_buf);
        }
        self.sample_buf.as_mut().unwrap()
    }
}

impl AudioWrite for PulseAudioOutput {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        let frame_count = decoded.frames();
        // Do nothing if there are no audio frames.
        if frame_count == 0 {
            log::trace!("No decoded frames. Returning");
            return Ok(0);
        }

        self.init_sample_buf(decoded.capacity() as Duration);
        let sample_buf = self.sample_buf.as_mut().unwrap();
        log::trace!("Interleaving samples");
        // Resampling is not required. Interleave the sample for cpal using a sample buffer.
        sample_buf.copy_interleaved_typed(&decoded);
        let buffer = sample_buf.as_bytes();

        log::trace!(
            "Writing to pulse audio {} frames, {} bytes",
            frame_count,
            buffer.len()
        );
        let start = SystemTime::now();
        // Write interleaved samples to PulseAudio.
        match self.pa.write(buffer) {
            Err(err) => {
                log::error!("audio output stream write error: {}", err);

                Err(AudioOutputError::StreamClosed)
            }
            _ => {
                let end = SystemTime::now();
                let took_ms = end.duration_since(start).unwrap().as_millis();
                if took_ms >= 500 {
                    log::error!("Detected audio interrupt");
                    return Err(AudioOutputError::Interrupt);
                } else {
                    log::trace!("Successfully wrote to pulse audio. Took {}ms", took_ms);
                }
                Ok(buffer.len())
            }
        }
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        // Flush is best-effort, ignore the returned result.
        let _ = self.pa.drain();
        Ok(())
    }
}

pub fn scan_default_output() -> Option<AudioOutputFactory> {
    let spec = SignalSpec {
        rate: SAMPLE_RATE.unwrap_or(pulse::sample::Spec::RATE_MAX),
        channels: Layout::Stereo.into_channels(),
    };
    Some(AudioOutputFactory::new(
        "PulseAudio Simple".to_string(),
        spec,
        move || Ok(Box::new(PulseAudioOutput::try_open(spec)?)),
    ))
}

pub fn scan_available_outputs() -> impl Iterator<Item = AudioOutputFactory> {
    scan_default_output().into_iter()
}
