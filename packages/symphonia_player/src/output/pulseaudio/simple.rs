use std::time::SystemTime;
use symphonia::core::audio::{AudioBufferRef, SignalSpec};

use crate::output::{
    pulseaudio::common::map_channels_to_pa_channelmap, AudioOutput, AudioOutputError,
};

use symphonia::core::audio::*;
use symphonia::core::units::Duration;

use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::{error, trace};

pub struct PulseAudioOutput {
    pa: psimple::Simple,
    sample_buf: RawSampleBuffer<f32>,
}

impl PulseAudioOutput {
    pub fn try_open(
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
        // An interleaved buffer is required to send data to PulseAudio. Use a SampleBuffer to
        // move data between Symphonia AudioBuffers and the byte buffers required by PulseAudio.
        let sample_buf = RawSampleBuffer::<f32>::new(duration, spec);

        // Create a PulseAudio stream specification.
        let pa_spec = pulse::sample::Spec {
            format: pulse::sample::Format::FLOAT32NE,
            channels: spec.channels.count() as u8,
            rate: spec.rate,
        };

        assert!(pa_spec.is_valid());

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
            Ok(pa) => Ok(Box::new(PulseAudioOutput { pa, sample_buf })),
            Err(err) => {
                error!("audio output stream open error: {}", err);

                Err(AudioOutputError::OpenStream)
            }
        }
    }
}

impl AudioOutput for PulseAudioOutput {
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        let frame_count = decoded.frames();
        // Do nothing if there are no audio frames.
        if frame_count == 0 {
            trace!("No decoded frames. Returning");
            return Ok(0);
        }

        trace!("Interleaving samples");
        // Interleave samples from the audio buffer into the sample buffer.
        self.sample_buf.copy_interleaved_ref(decoded);
        let buffer = self.sample_buf.as_bytes();

        trace!(
            "Writing to pulse audio {} frames, {} bytes",
            frame_count,
            buffer.len()
        );
        let start = SystemTime::now();
        // Write interleaved samples to PulseAudio.
        match self.pa.write(buffer) {
            Err(err) => {
                error!("audio output stream write error: {}", err);

                Err(AudioOutputError::StreamClosed)
            }
            _ => {
                let end = SystemTime::now();
                let took_ms = end.duration_since(start).unwrap().as_millis();
                if took_ms >= 500 {
                    error!("Detected audio interrupt");
                    return Err(AudioOutputError::Interrupt);
                } else {
                    trace!("Successfully wrote to pulse audio. Took {}ms", took_ms);
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

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    PulseAudioOutput::try_open(spec, duration)
}
