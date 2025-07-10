use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;
use moosicbox_env_utils::option_env_u32;
use symphonia::core::audio::{AudioBuffer, Layout, RawSampleBuffer, Signal, SignalSpec};
use symphonia::core::units::Duration;

use crate::{
    AudioOutputError, AudioOutputFactory, AudioWrite, ProgressTracker,
    pulseaudio::common::map_channels_to_pa_channelmap,
};

static SAMPLE_RATE: Option<u32> = option_env_u32!("PULSEAUDIO_RESAMPLE_RATE");

pub struct PulseAudioOutput {
    spec: SignalSpec,
    pa: psimple::Simple,
    sample_buf: Option<RawSampleBuffer<f32>>,
    progress_tracker: ProgressTracker,
}

impl PulseAudioOutput {
    /// # Panics
    ///
    /// * If fails to convert the channels count to u8
    /// * If the spec is invalid
    ///
    /// # Errors
    ///
    /// * If `psimple::Simple::new` fails to initialize with the given spec
    pub fn try_open(spec: SignalSpec) -> Result<Self, AudioOutputError> {
        // Create a PulseAudio stream specification.
        let pa_spec = pulse::sample::Spec {
            format: pulse::sample::Format::FLOAT32NE,
            channels: u8::try_from(spec.channels.count()).unwrap(),
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
            Ok(pa) => Ok(Self {
                spec,
                pa,
                sample_buf: None,
                progress_tracker: {
                    let tracker = ProgressTracker::new(Some(0.1));
                    tracker
                        .set_audio_spec(spec.rate, u32::try_from(spec.channels.count()).unwrap());
                    tracker
                },
            }),
            Err(err) => {
                log::error!("audio output stream open error: {err}");

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
        let start = switchy_time::now();
        // Write interleaved samples to PulseAudio.
        if let Err(err) = self.pa.write(buffer) {
            log::error!("audio output stream write error: {err}");

            Err(AudioOutputError::StreamClosed)
        } else {
            let end = switchy_time::now();
            let took_ms = end.duration_since(start).unwrap().as_millis();
            if took_ms >= 500 {
                log::error!("Detected audio interrupt");
                return Err(AudioOutputError::Interrupt);
            }

            log::trace!("Successfully wrote to pulse audio. Took {took_ms}ms");

            // Update progress tracker with consumed samples
            // Calculate samples from bytes written
            let bytes_per_sample = std::mem::size_of::<f32>();
            let channels = self.spec.channels.count();
            let samples_written = buffer.len() / (bytes_per_sample * channels);
            self.progress_tracker
                .update_consumed_samples(samples_written);

            Ok(buffer.len())
        }
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        // Flush is best-effort, ignore the returned result.
        let _ = self.pa.drain();

        // Reset progress tracker for next track
        self.progress_tracker.reset();

        Ok(())
    }

    fn get_playback_position(&self) -> Option<f64> {
        self.progress_tracker.get_position()
    }

    fn set_consumed_samples(
        &mut self,
        consumed_samples: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let current_value = consumed_samples.load(std::sync::atomic::Ordering::SeqCst);
        log::debug!("PulseAudio Simple: set_consumed_samples called with value: {current_value}");

        // Update the progress tracker with the initial value
        self.progress_tracker.set_consumed_samples(current_value);
    }

    fn set_volume(&mut self, _volume: f64) {
        // PulseAudio volume control could be implemented here
        log::debug!("PulseAudio Simple: set_volume called but not implemented");
    }

    fn set_shared_volume(&mut self, _shared_volume: std::sync::Arc<atomic_float::AtomicF64>) {
        // PulseAudio shared volume could be implemented here
        log::debug!("PulseAudio Simple: set_shared_volume called but not implemented");
    }

    fn get_output_spec(&self) -> Option<crate::SignalSpec> {
        Some(self.spec)
    }

    fn set_progress_callback(
        &mut self,
        callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>,
    ) {
        self.progress_tracker.set_callback(callback);
    }
}

#[must_use]
pub fn scan_default_output() -> Option<AudioOutputFactory> {
    let spec = SignalSpec {
        rate: SAMPLE_RATE.unwrap_or(pulse::sample::Spec::RATE_MAX),
        channels: Layout::Stereo.into_channels(),
    };

    let id = "pulseaudio-simple:default".to_string();

    Some(AudioOutputFactory::new(
        id,
        "PulseAudio Simple".to_string(),
        spec,
        move || Ok(Box::new(PulseAudioOutput::try_open(spec)?)),
    ))
}

pub fn scan_available_outputs() -> impl Iterator<Item = AudioOutputFactory> {
    scan_default_output().into_iter()
}
