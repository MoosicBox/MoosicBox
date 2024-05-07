use bytes::Bytes;
use symphonia::core::audio::{AudioBuffer, Signal as _, SignalSpec};
use symphonia::core::conv::{FromSample, IntoSample as _};
use symphonia::core::formats::{Packet, Track};
use symphonia::core::units::Duration;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub trait AudioOutput {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError>;
    fn flush(&mut self) -> Result<(), AudioOutputError>;
}

pub trait AudioEncoder: Send + Sync {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError>;
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
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
    #[error(transparent)]
    IO(#[from] std::io::Error),
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
type OpenFunc = Box<dyn FnMut(SignalSpec, Duration) -> Result<InnerType, AudioOutputError> + Send>;
type AudioFilter =
    Box<dyn FnMut(&mut AudioBuffer<f32>, &Packet, &Track) -> Result<(), AudioOutputError> + Send>;

pub struct AudioOutputHandler {
    pub cancellation_token: Option<CancellationToken>,
    filters: Vec<AudioFilter>,
    open_outputs: Vec<OpenFunc>,
    outputs: Vec<InnerType>,
}

impl AudioOutputHandler {
    pub fn new() -> Self {
        Self {
            cancellation_token: None,
            filters: vec![],
            open_outputs: vec![],
            outputs: vec![],
        }
    }

    pub fn with_filter(mut self, filter: AudioFilter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn with_output(mut self, open_output: OpenFunc) -> Self {
        self.open_outputs.push(open_output);
        self
    }

    pub fn with_cancellation_token(mut self, cancellation_token: CancellationToken) -> Self {
        self.cancellation_token.replace(cancellation_token);
        self
    }

    fn run_filters(
        &mut self,
        decoded: &mut AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioOutputError> {
        for filter in &mut self.filters {
            log::trace!("Running audio filter");
            filter(decoded, packet, track)?;
        }
        Ok(())
    }

    pub fn write(
        &mut self,
        mut decoded: AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioOutputError> {
        self.run_filters(&mut decoded, packet, track)?;

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

    pub fn contains_outputs_to_open(&self) -> bool {
        !self.open_outputs.is_empty()
    }

    pub fn try_open(
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

#[allow(unused)]
fn to_samples<S: FromSample<f32> + Default + Clone>(decoded: AudioBuffer<f32>) -> Vec<S> {
    let n_channels = decoded.spec().channels.count();
    let n_samples = decoded.frames() * n_channels;
    let mut buf: Vec<S> = vec![S::default(); n_samples];

    // Interleave the source buffer channels into the sample buffer.
    for ch in 0..n_channels {
        let ch_slice = decoded.chan(ch);

        for (dst, decoded) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
            *dst = (*decoded).into_sample();
        }
    }

    buf
}
