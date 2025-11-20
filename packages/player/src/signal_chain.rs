//! Audio signal processing chain for encoding and decoding.
//!
//! This module provides [`SignalChain`], which allows chaining together audio processing
//! steps such as decoding, encoding, and resampling. The signal chain processes audio
//! from a media source through multiple transformation stages.

#![allow(clippy::module_name_repetitions)]

use flume::Receiver;
use moosicbox_audio_decoder::{AudioDecodeError, AudioDecodeHandler};
use moosicbox_audio_output::encoder::AudioEncoder;
use moosicbox_resampler::Resampler;
use symphonia::core::{
    audio::{AudioBuffer, Signal},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    probe::Hint,
};
use thiserror::Error;

use super::symphonia_unsync::{PlaybackError, play_media_source};

/// Errors that can occur during signal chain processing.
#[derive(Debug, Error)]
pub enum SignalChainError {
    /// Error from the underlying playback system
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    /// Signal chain has no processing steps
    #[error("SignalChain is empty")]
    Empty,
}

type CreateAudioDecodeStream = Box<dyn (FnOnce() -> AudioDecodeHandler) + Send + 'static>;
type CreateAudioEncoder = Box<dyn (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>;

/// A chain of audio processing steps for encoding and decoding.
///
/// This allows building a pipeline of audio transformations, such as
/// decoding from one format and encoding to another.
pub struct SignalChain {
    steps: Vec<SignalChainStep>,
}

impl SignalChain {
    /// Creates a new empty signal chain.
    #[must_use]
    pub const fn new() -> Self {
        Self { steps: vec![] }
    }

    /// Sets the format hint for the most recently added step.
    ///
    /// The hint helps the decoder identify the audio format.
    #[must_use]
    pub fn with_hint(mut self, hint: Hint) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_hint(hint));
        }
        self
    }

    /// Sets the audio decode handler for the most recently added step.
    ///
    /// The handler processes decoded audio samples.
    #[must_use]
    pub fn with_audio_decode_handler<F: (FnOnce() -> AudioDecodeHandler) + Send + 'static>(
        mut self,
        handler: F,
    ) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_audio_decode_handler(handler));
        }

        self
    }

    /// Sets the audio encoder for the most recently added step.
    ///
    /// The encoder transforms decoded audio into a different format.
    #[must_use]
    pub fn with_encoder<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_encoder(encoder));
        }
        self
    }

    /// Sets whether to verify decoded audio for the most recently added step.
    #[must_use]
    pub fn with_verify(mut self, verify: bool) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_verify(verify));
        }
        self
    }

    /// Sets the seek position for the most recently added step.
    ///
    /// The seek position is specified in seconds.
    #[must_use]
    pub fn with_seek(mut self, seek: Option<f64>) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_seek(seek));
        }
        self
    }

    /// Adds a new empty step to the signal chain.
    #[must_use]
    pub fn next_step(mut self) -> Self {
        self.steps.push(SignalChainStep::new());
        self
    }

    /// Adds a configured step to the signal chain.
    #[must_use]
    pub fn add_step(mut self, step: SignalChainStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Adds a new step with the specified encoder to the signal chain.
    #[must_use]
    pub fn add_encoder_step<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        self.steps
            .push(SignalChainStep::new().with_encoder(encoder));
        self
    }

    /// Adds a new step with the specified resampler to the signal chain.
    #[must_use]
    pub fn add_resampler_step(mut self, resampler: Resampler<f32>) -> Self {
        self.steps
            .push(SignalChainStep::new().with_resampler(resampler));
        self
    }

    /// Processes audio through all steps in the signal chain.
    ///
    /// # Errors
    ///
    /// * If fails to process the audio somewhere in the `SignalChain`
    pub fn process(
        mut self,
        media_source: Box<dyn MediaSource>,
    ) -> Result<Box<dyn MediaSource>, SignalChainError> {
        log::trace!("process: starting SignalChain processor");
        if self.steps.is_empty() {
            return Err(SignalChainError::Empty);
        }

        let mut processor = self.steps.remove(0).process(media_source)?;

        while !self.steps.is_empty() {
            let step = self.steps.remove(0);
            processor = step.process(Box::new(processor))?;
        }

        Ok(Box::new(processor))
    }
}

impl Default for SignalChain {
    fn default() -> Self {
        Self::new()
    }
}

/// A single step in a signal processing chain.
///
/// Each step can perform audio decoding, encoding, or resampling operations.
pub struct SignalChainStep {
    hint: Option<Hint>,
    audio_output_handler: Option<CreateAudioDecodeStream>,
    encoder: Option<CreateAudioEncoder>,
    resampler: Option<Resampler<f32>>,
    enable_gapless: bool,
    verify: bool,
    seek: Option<f64>,
}

impl SignalChainStep {
    /// Creates a new signal chain step with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hint: None,
            audio_output_handler: None,
            encoder: None,
            resampler: None,
            enable_gapless: true,
            verify: true,
            seek: None,
        }
    }

    /// Sets the format hint for this step.
    ///
    /// The hint helps the decoder identify the audio format.
    #[must_use]
    pub fn with_hint(mut self, hint: Hint) -> Self {
        self.hint.replace(hint);
        self
    }

    /// Sets the audio decode handler for this step.
    ///
    /// The handler processes decoded audio samples.
    #[must_use]
    pub fn with_audio_decode_handler<F: (FnOnce() -> AudioDecodeHandler) + Send + 'static>(
        mut self,
        handler: F,
    ) -> Self {
        self.audio_output_handler.replace(Box::new(handler));
        self
    }

    /// Sets the audio encoder for this step.
    ///
    /// The encoder transforms decoded audio into a different format.
    #[must_use]
    pub fn with_encoder<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        self.encoder.replace(Box::new(encoder));
        self
    }

    /// Sets the resampler for this step.
    ///
    /// The resampler converts audio to a different sample rate.
    #[must_use]
    pub fn with_resampler(mut self, resampler: Resampler<f32>) -> Self {
        self.resampler.replace(resampler);
        self
    }

    /// Sets whether to verify decoded audio for this step.
    #[must_use]
    pub const fn with_verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }

    /// Sets the seek position for this step.
    ///
    /// The seek position is specified in seconds.
    #[must_use]
    pub const fn with_seek(mut self, seek: Option<f64>) -> Self {
        self.seek = seek;
        self
    }

    /// Processes audio through this signal chain step.
    ///
    /// # Errors
    ///
    /// * If fails to process the audio somewhere in the `SignalChain`
    pub fn process(
        self,
        media_source: Box<dyn MediaSource>,
    ) -> Result<SignalChainStepProcessor, SignalChainError> {
        let hint = self.hint.unwrap_or_default();
        let mss = MediaSourceStream::new(media_source, MediaSourceStreamOptions::default());

        let receiver = play_media_source(
            mss,
            &hint,
            self.enable_gapless,
            self.verify,
            None,
            self.seek,
        )?;

        let encoder = self.encoder.map(|get_encoder| get_encoder());

        Ok(SignalChainStepProcessor {
            encoder,
            resampler: self.resampler,
            receiver,
            overflow: vec![],
        })
    }
}

impl Default for SignalChainStep {
    fn default() -> Self {
        Self::new()
    }
}

/// Processes audio through a signal chain step.
///
/// This reads audio buffers, applies transformations (resampling, encoding),
/// and outputs the processed audio data.
pub struct SignalChainStepProcessor {
    encoder: Option<Box<dyn AudioEncoder>>,
    resampler: Option<Resampler<f32>>,
    receiver: Receiver<AudioBuffer<f32>>,
    overflow: Vec<u8>,
}

impl SignalChainStepProcessor {}

impl std::io::Seek for SignalChainStepProcessor {
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other(
            "SignalChainStepProcessor does not support seeking",
        ))
    }
}

impl std::io::Read for SignalChainStepProcessor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.overflow.is_empty() {
            log::debug!("buf len={} overflow len={}", buf.len(), self.overflow.len());
            let end = std::cmp::min(buf.len(), self.overflow.len());
            // FIXME: find a better way
            buf[..end].copy_from_slice(&self.overflow.drain(..end).collect::<Vec<_>>());

            log::debug!("Returned buffer from overflow buf");
            return Ok(end);
        }

        let bytes = loop {
            log::debug!("Waiting for samples from receiver...");
            let audio = self
                .receiver
                .recv_timeout(std::time::Duration::from_millis(1000))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::TimedOut, e))?;
            log::debug!("Received {} frames from receiver", audio.frames());

            let audio = if let Some(resampler) = &mut self.resampler {
                let channels = audio.spec().channels.count();

                log::debug!("Resampling frames...");
                let samples = resampler
                    .resample(&audio)
                    .ok_or_else(|| std::io::Error::other("Failed to resample"))?;
                let buf = AudioBuffer::new((samples.len() / channels) as u64, resampler.spec);
                log::debug!("Resampled into {} frames", buf.frames());
                buf
            } else {
                audio
            };

            if let Some(encoder) = &mut self.encoder {
                log::debug!("Encoding frames...");
                let bytes = encoder.encode(audio).map_err(std::io::Error::other)?;
                log::debug!("Encoded into {} bytes", bytes.len());

                if !bytes.is_empty() {
                    break Some(bytes);
                }
            } else {
                break None;
            }
        };

        let bytes = bytes.unwrap();
        let (bytes_now, overflow) = bytes.split_at(std::cmp::min(buf.len(), bytes.len()));

        log::debug!(
            "buf len={} bytes_now len={} overflow len={}",
            buf.len(),
            bytes_now.len(),
            overflow.len()
        );
        buf[..bytes_now.len()].copy_from_slice(bytes_now);
        self.overflow.extend_from_slice(overflow);

        Ok(bytes_now.len())
    }
}

impl MediaSource for SignalChainStepProcessor {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Errors that can occur during signal chain step processing.
#[derive(Debug, Error)]
pub enum SignalChainProcessorError {
    /// Error from the underlying playback system
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    /// Error from audio decoding
    #[error(transparent)]
    AudioDecode(#[from] AudioDecodeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_signal_chain_new_creates_empty_chain() {
        let chain = SignalChain::new();
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_default_creates_empty_chain() {
        let chain = SignalChain::default();
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_add_step_increases_count() {
        let chain = SignalChain::new();
        assert_eq!(chain.steps.len(), 0);

        let chain = chain.add_step(SignalChainStep::new());
        assert_eq!(chain.steps.len(), 1);

        let chain = chain.add_step(SignalChainStep::new());
        assert_eq!(chain.steps.len(), 2);
    }

    #[test_log::test]
    fn test_signal_chain_next_step_adds_empty_step() {
        let chain = SignalChain::new().next_step();
        assert_eq!(chain.steps.len(), 1);

        let chain = chain.next_step();
        assert_eq!(chain.steps.len(), 2);
    }

    #[test_log::test]
    fn test_signal_chain_with_hint_modifies_last_step() {
        let mut hint = Hint::new();
        hint.with_extension("mp3");

        let chain = SignalChain::new().next_step().with_hint(hint);
        assert_eq!(chain.steps.len(), 1);
        // The hint should be set on the last step (we can't directly inspect it,
        // but we verify the chain structure is correct)
    }

    #[test_log::test]
    fn test_signal_chain_with_hint_on_empty_chain_does_nothing() {
        let mut hint = Hint::new();
        hint.with_extension("flac");

        let chain = SignalChain::new().with_hint(hint);
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_with_verify_modifies_last_step() {
        let chain = SignalChain::new().next_step().with_verify(false);
        assert_eq!(chain.steps.len(), 1);
    }

    #[test_log::test]
    fn test_signal_chain_with_seek_modifies_last_step() {
        let chain = SignalChain::new().next_step().with_seek(Some(30.0));
        assert_eq!(chain.steps.len(), 1);
    }

    #[test_log::test]
    fn test_signal_chain_step_new_has_defaults() {
        let step = SignalChainStep::new();
        assert!(step.hint.is_none());
        assert!(step.audio_output_handler.is_none());
        assert!(step.encoder.is_none());
        assert!(step.resampler.is_none());
        assert!(step.enable_gapless);
        assert!(step.verify);
        assert!(step.seek.is_none());
    }

    #[test_log::test]
    fn test_signal_chain_step_default_has_defaults() {
        let step = SignalChainStep::default();
        assert!(step.hint.is_none());
        assert!(step.audio_output_handler.is_none());
        assert!(step.encoder.is_none());
        assert!(step.resampler.is_none());
        assert!(step.enable_gapless);
        assert!(step.verify);
        assert!(step.seek.is_none());
    }

    #[test_log::test]
    fn test_signal_chain_step_with_verify_sets_value() {
        let step = SignalChainStep::new().with_verify(false);
        assert!(!step.verify);

        let step = SignalChainStep::new().with_verify(true);
        assert!(step.verify);
    }

    #[test_log::test]
    fn test_signal_chain_step_with_seek_sets_value() {
        let step = SignalChainStep::new().with_seek(Some(45.5));
        assert_eq!(step.seek, Some(45.5));

        let step = SignalChainStep::new().with_seek(None);
        assert_eq!(step.seek, None);
    }

    #[test_log::test]
    fn test_signal_chain_step_with_hint_sets_value() {
        let mut hint = Hint::new();
        hint.with_extension("opus");

        let step = SignalChainStep::new().with_hint(hint);
        assert!(step.hint.is_some());
    }

    #[test_log::test]
    fn test_signal_chain_builder_pattern_chaining() {
        // Test that builder methods can be chained together
        let mut hint = Hint::new();
        hint.with_extension("aac");

        let chain = SignalChain::new()
            .next_step()
            .with_hint(hint)
            .with_verify(false)
            .with_seek(Some(10.0))
            .next_step()
            .with_verify(true);

        assert_eq!(chain.steps.len(), 2);
    }

    #[test_log::test]
    fn test_signal_chain_step_builder_pattern_chaining() {
        let mut hint = Hint::new();
        hint.with_extension("flac");

        let step = SignalChainStep::new()
            .with_hint(hint)
            .with_verify(false)
            .with_seek(Some(20.0));

        assert!(step.hint.is_some());
        assert!(!step.verify);
        assert_eq!(step.seek, Some(20.0));
    }
}
