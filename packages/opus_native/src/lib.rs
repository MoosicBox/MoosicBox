#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

pub mod error;
pub mod range;
#[cfg(feature = "silk")]
pub mod silk;

pub use error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono = 1,
    Stereo = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Hz8000 = 8000,
    Hz12000 = 12000,
    Hz16000 = 16000,
    Hz24000 = 24000,
    Hz48000 = 48000,
}

pub struct Decoder {
    #[allow(dead_code)]
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,
}

impl Decoder {
    /// Creates a new Opus decoder.
    ///
    /// # Errors
    ///
    /// Returns an error if decoder initialization fails (not yet implemented).
    pub const fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self> {
        Ok(Self {
            sample_rate,
            channels,
        })
    }

    /// Decodes an Opus packet to signed 16-bit PCM.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails (not yet implemented - will be implemented in Phase 6).
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize> {
        let _ = (self, input, output, fec);
        todo!("Implement in Phase 6")
    }

    /// Decodes an Opus packet to floating point PCM.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails (not yet implemented - will be implemented in Phase 6).
    pub fn decode_float(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [f32],
        fec: bool,
    ) -> Result<usize> {
        let _ = (self, input, output, fec);
        todo!("Implement in Phase 6")
    }

    /// Resets the decoder state.
    ///
    /// # Errors
    ///
    /// Returns an error if reset fails (not yet implemented - will be implemented in Phase 6).
    pub fn reset_state(&mut self) -> Result<()> {
        let _ = self;
        todo!("Implement in Phase 6")
    }
}
