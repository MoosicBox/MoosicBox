use std::result;

use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::units::Duration;
use thiserror::Error;

pub trait AudioOutput {
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
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

pub type Result<T> = result::Result<T, AudioOutputError>;

#[cfg(all(feature = "pulseaudio", target_os = "linux"))]
mod pulseaudio;

#[cfg(feature = "cpal")]
mod cpal;

#[cfg(feature = "pulseaudio-simple")]
pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Box<dyn AudioOutput>> {
    pulseaudio::simple::try_open(spec, duration)
}

#[cfg(feature = "cpal")]
pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Box<dyn AudioOutput>> {
    cpal::player::try_open(spec, duration)
}

#[cfg(not(any(feature = "cpal", feature = "pulseaudio-simple")))]
pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Box<dyn AudioOutput>> {
    #[cfg(feature = "pulseaudio")]
    compile_error!("Must use 'pulseaudio-simple' feature");
    #[cfg(not(feature = "pulseaudio"))]
    compile_error!("Must specify a valid audio output feature. e.g. cpal or pulseaudio-simple");

    unreachable!()
}
