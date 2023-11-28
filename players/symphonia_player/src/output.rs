use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::units::Duration;
use thiserror::Error;

use crate::AudioOutputType;

pub trait AudioOutput {
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError>;
    fn flush(&mut self) -> Result<(), AudioOutputError>;
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error, Clone)]
pub enum AudioOutputError {
    #[error("OpenStreamError")]
    OpenStream,
    #[error("PlayStreamError")]
    PlayStream,
    #[error("StreamClosedError")]
    StreamClosed,
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
}

#[cfg(all(
    not(windows),
    any(feature = "pulseaudio-standard", feature = "pulseaudio-simple")
))]
mod pulseaudio;

#[cfg(feature = "cpal")]
mod cpal;

#[cfg(feature = "opus")]
mod opus;

pub fn try_open(
    audio_output_type: &AudioOutputType,
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    #[cfg(all(
        not(any(
            feature = "cpal",
            feature = "opus",
            feature = "pulseaudio-standard",
            feature = "pulseaudio-simple"
        )),
        feature = "pulseaudio"
    ))]
    compile_error!("Must use 'pulseaudio-standard' or 'pulseaudio-simple' feature");

    #[cfg(not(any(
        feature = "cpal",
        feature = "opus",
        feature = "pulseaudio-standard",
        feature = "pulseaudio-simple",
        feature = "pulseaudio"
    )))]
    compile_error!("Must specify a valid audio output feature. e.g. cpal, opus, pulseaudio-standard, or pulseaudio-simple");

    match audio_output_type {
        #[cfg(feature = "cpal")]
        AudioOutputType::Cpal => cpal::player::try_open(spec, duration),
        #[cfg(all(not(windows), feature = "pulseaudio-standard"))]
        AudioOutputType::PulseAudioStandard => pulseaudio::standard::try_open(spec, duration),
        #[cfg(all(not(windows), feature = "pulseaudio-simple"))]
        AudioOutputType::PulseAudioSimple => pulseaudio::simple::try_open(spec, duration),
        #[cfg(feature = "opus")]
        AudioOutputType::Opus => opus::encoder::try_open(spec, duration),
    }
}
