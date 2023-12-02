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
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
}

#[cfg(all(
    not(windows),
    any(feature = "pulseaudio-standard", feature = "pulseaudio-simple")
))]
pub mod pulseaudio;

#[cfg(feature = "cpal")]
pub mod cpal;

pub mod encoder;

type InnerType = Box<dyn AudioOutput>;
type OpenFunc = Box<dyn FnMut(SignalSpec, Duration) -> Result<InnerType, AudioOutputError>>;

pub struct AudioOutputHandler {
    pub(crate) inner: Option<InnerType>,
    pub(crate) try_open: OpenFunc,
}

impl AudioOutputHandler {
    pub fn new(try_open: OpenFunc) -> Self {
        Self {
            inner: None,
            try_open,
        }
    }

    pub(crate) fn try_open(
        &mut self,
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<(), AudioOutputError> {
        self.inner = Some((*self.try_open)(spec, duration)?);
        Ok(())
    }
}
