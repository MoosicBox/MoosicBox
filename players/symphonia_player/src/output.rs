use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::units::Duration;
use thiserror::Error;

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
    #[error("InterruptError")]
    Interrupt,
}

#[cfg(feature = "pulseaudio")]
mod pulseaudio;

#[cfg(feature = "cpal")]
mod cpal;

#[cfg(feature = "pulseaudio-standard")]
pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    pulseaudio::standard::try_open(spec, duration)
}

#[cfg(feature = "pulseaudio-simple")]
pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    pulseaudio::simple::try_open(spec, duration)
}

#[cfg(feature = "cpal")]
pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    cpal::player::try_open(spec, duration)
}

#[cfg(not(any(
    feature = "cpal",
    feature = "pulseaudio-standard",
    feature = "pulseaudio-simple"
)))]
pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    #[cfg(feature = "pulseaudio")]
    compile_error!("Must use 'pulseaudio-standard' or 'pulseaudio-simple' feature");
    #[cfg(not(feature = "pulseaudio"))]
    compile_error!("Must specify a valid audio output feature. e.g. cpal, pulseaudio-standard, or pulseaudio-simple");

    unreachable!()
}
