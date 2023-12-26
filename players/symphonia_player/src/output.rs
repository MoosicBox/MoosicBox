use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::formats::{Packet, Track};
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
type AudioFilter =
    Box<dyn FnMut(&mut AudioBufferRef<'_>, &Packet, &Track) -> Result<(), AudioOutputError>>;

pub struct AudioOutputHandler {
    pub(crate) filters: Vec<AudioFilter>,
    pub(crate) open_outputs: Vec<OpenFunc>,
    pub(crate) outputs: Vec<InnerType>,
}

impl AudioOutputHandler {
    pub fn new() -> Self {
        Self {
            filters: vec![],
            open_outputs: vec![],
            outputs: vec![],
        }
    }

    pub fn with_filter(&mut self, filter: AudioFilter) {
        self.filters.push(filter);
    }

    pub fn with_output(&mut self, open_output: OpenFunc) {
        self.open_outputs.push(open_output);
    }

    pub fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<(), AudioOutputError> {
        let len = self.outputs.len();

        for (i, output) in self.outputs.iter_mut().enumerate() {
            if i == len - 1 {
                output.write(decoded)?;
                break;
            } else {
                output.write(decoded.clone())?;
            }
        }

        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), AudioOutputError> {
        for output in self.outputs.iter_mut() {
            output.flush()?;
        }
        Ok(())
    }

    pub(crate) fn try_open(
        &mut self,
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<(), AudioOutputError> {
        for mut open_func in self.open_outputs.drain(..) {
            self.outputs.push((*open_func)(spec, duration)?);
        }
        Ok(())
    }
}

impl Default for AudioOutputHandler {
    fn default() -> Self {
        Self::new()
    }
}
