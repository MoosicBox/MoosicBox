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
    audio::AudioBuffer,
    formats::probe::Hint,
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
};
use thiserror::Error;

use super::symphonia_unsync::{PlaybackError, play_media_source_with_options_result};

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

    /// Select decoding behavior for the most recently added step.
    #[must_use]
    pub fn with_decode_options(mut self, options: crate::DecodeOptions) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_decode_options(options));
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
    decode_options: crate::DecodeOptions,
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
            decode_options: crate::DecodeOptions {
                decoder: symphonia::core::codecs::audio::AudioDecoderOptions::default()
                    .verify(true),
                ..crate::DecodeOptions::default()
            },
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
        self.decode_options.decoder.verify = verify;
        self
    }

    /// Select decoding behavior and the exact Opus backend for this step.
    #[must_use]
    pub const fn with_decode_options(mut self, options: crate::DecodeOptions) -> Self {
        self.decode_options = options;
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

        let receiver = play_media_source_with_options_result(
            mss,
            &hint,
            self.decode_options,
            None,
            self.seek,
        )?;

        let encoder = self.encoder.map(|get_encoder| get_encoder());

        Ok(SignalChainStepProcessor {
            encoder,
            resampler: self.resampler,
            receiver,
            overflow: vec![],
            drained: false,
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
/// and outputs the processed audio data. The processor receives decoded audio
/// buffers through a channel, optionally resamples and encodes them, and
/// provides the output bytes through the `Read` trait.
pub struct SignalChainStepProcessor {
    /// Optional encoder for transforming audio format
    encoder: Option<Box<dyn AudioEncoder>>,
    /// Optional resampler for sample rate conversion
    resampler: Option<Resampler<f32>>,
    /// Channel receiver for incoming audio buffers
    receiver: Receiver<Result<AudioBuffer<f32>, moosicbox_audio_decoder::DecodeError>>,
    /// Buffer for bytes that overflow the read buffer
    overflow: Vec<u8>,
    drained: bool,
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
        if buf.is_empty() {
            return Ok(0);
        }
        if !self.overflow.is_empty() {
            log::debug!("buf len={} overflow len={}", buf.len(), self.overflow.len());
            let end = std::cmp::min(buf.len(), self.overflow.len());
            // FIXME: find a better way
            buf[..end].copy_from_slice(&self.overflow.drain(..end).collect::<Vec<_>>());

            log::debug!("Returned buffer from overflow buf");
            return Ok(end);
        }

        if buf.is_empty() || self.drained {
            return Ok(0);
        }
        let bytes = loop {
            let audio = match self
                .receiver
                .recv_timeout(std::time::Duration::from_secs(1))
            {
                Ok(Err(error)) => {
                    self.drained = true;
                    return Err(std::io::Error::other(error));
                }
                Ok(Ok(audio)) => {
                    if let Some(resampler) = &mut self.resampler {
                        let Some(audio) = resampler
                            .resample_to_audio_buffer(&audio)
                            .map_err(std::io::Error::other)?
                        else {
                            continue;
                        };
                        audio
                    } else {
                        audio
                    }
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "audio input timed out",
                    ));
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    let tail = if let Some(resampler) = &mut self.resampler {
                        let spec = resampler.output_spec().clone();
                        resampler
                            .flush()
                            .map(|samples| moosicbox_resampler::to_audio_buffer(samples, spec))
                            .transpose()
                            .map_err(std::io::Error::other)?
                    } else {
                        None
                    };
                    if let Some(tail) = tail {
                        tail
                    } else {
                        self.drained = true;
                        let bytes = self
                            .encoder
                            .as_mut()
                            .map(|encoder| encoder.finish())
                            .transpose()
                            .map_err(std::io::Error::other)?
                            .unwrap_or_default();
                        if bytes.is_empty() {
                            return Ok(0);
                        }
                        break bytes;
                    }
                }
            };
            let Some(encoder) = &mut self.encoder else {
                return Err(std::io::Error::other("signal chain has no encoder"));
            };
            let bytes = encoder.encode(audio).map_err(|error| {
                self.drained = true;
                std::io::Error::other(error)
            })?;
            if !bytes.is_empty() {
                break bytes;
            }
            if self.drained {
                return Ok(0);
            }
        };

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

    use std::{
        io::Read as _,
        sync::{Arc, Mutex},
    };
    use symphonia::core::audio::{Audio as _, AudioMut as _, AudioSpec, layouts};

    struct InspectEncoder {
        spec: AudioSpec,
        calls: Arc<Mutex<usize>>,
    }
    impl AudioEncoder for InspectEncoder {
        fn encode(
            &mut self,
            audio: AudioBuffer<f32>,
        ) -> Result<bytes::Bytes, moosicbox_audio_output::AudioOutputError> {
            assert_eq!(audio.spec(), &self.spec);
            assert!(audio.frames() > 0);
            assert!(
                audio
                    .iter_planes()
                    .all(|plane| plane.iter().any(|sample| sample.abs() > 0.01))
            );
            *self.calls.lock().unwrap() += 1;
            Ok(bytes::Bytes::from_static(&[1, 2, 3, 4]))
        }
        fn spec(&self) -> AudioSpec {
            self.spec.clone()
        }
    }

    fn partial_input(spec: AudioSpec) -> AudioBuffer<f32> {
        let mut input = AudioBuffer::<f32>::new(spec, 4096);
        input.render_silence(Some(512));
        for plane in input.iter_planes_mut() {
            plane.fill(0.25);
        }
        input
    }

    #[test_log::test]
    fn test_decode_options_are_explicit_and_verify_builder_preserves_backend() {
        let options = crate::DecodeOptions {
            opus_backend: crate::OpusDecodeBackend::Native,
            decoder: symphonia::core::codecs::audio::AudioDecoderOptions::default().gapless(false),
        };
        let chain = SignalChain::new()
            .next_step()
            .with_decode_options(options)
            .with_verify(false);
        assert_eq!(
            chain.steps[0].decode_options.opus_backend,
            crate::OpusDecodeBackend::Native
        );
        assert!(!chain.steps[0].decode_options.decoder.gapless);
        assert!(!chain.steps[0].decode_options.decoder.verify);
        assert_eq!(
            SignalChainStep::new().decode_options.opus_backend,
            crate::OpusDecodeBackend::Libopus
        );
    }

    #[test_log::test]
    fn test_partial_inputs_continue_and_disconnect_drains_once() {
        for chunks in [2, 9] {
            let input_spec = AudioSpec::new(44_100, layouts::CHANNEL_LAYOUT_MONO);
            let output_spec = AudioSpec::new(48_000, layouts::CHANNEL_LAYOUT_MONO);
            let calls = Arc::new(Mutex::new(0));
            let (tx, receiver) = flume::unbounded();
            for _ in 0..chunks {
                tx.send(Ok(partial_input(input_spec.clone()))).unwrap();
            }
            drop(tx);
            let mut processor = SignalChainStepProcessor {
                encoder: Some(Box::new(InspectEncoder {
                    spec: output_spec,
                    calls: calls.clone(),
                })),
                resampler: Some(Resampler::new(input_spec, 48_000, 4096)),
                receiver,
                overflow: vec![],
                drained: false,
            };
            let mut bytes = Vec::new();
            // Tiny reads force overflow handling after both ordinary and EOF encoding.
            let mut byte = [0];
            while processor.read(&mut byte).unwrap() != 0 {
                bytes.push(byte[0]);
            }
            let expected_calls = if chunks == 2 { 1 } else { 2 };
            assert_eq!(*calls.lock().unwrap(), expected_calls);
            assert_eq!(bytes, [1, 2, 3, 4].repeat(expected_calls));
            assert_eq!(processor.read(&mut byte).unwrap(), 0);
            assert_eq!(*calls.lock().unwrap(), expected_calls);
        }
    }

    struct DrainEncoder {
        finishes: Arc<Mutex<usize>>,
        fail: bool,
    }
    impl AudioEncoder for DrainEncoder {
        fn encode(
            &mut self,
            _audio: AudioBuffer<f32>,
        ) -> Result<bytes::Bytes, moosicbox_audio_output::AudioOutputError> {
            Ok(bytes::Bytes::new())
        }
        fn finish(&mut self) -> Result<bytes::Bytes, moosicbox_audio_output::AudioOutputError> {
            *self.finishes.lock().unwrap() += 1;
            if self.fail {
                return Err(moosicbox_audio_output::AudioOutputError::Interrupt);
            }
            Ok(bytes::Bytes::from_static(&[5, 6, 7]))
        }
        fn spec(&self) -> AudioSpec {
            AudioSpec::new(48_000, layouts::CHANNEL_LAYOUT_MONO)
        }
    }

    #[test_log::test]
    fn test_encoder_finish_once_with_overflow_and_error() {
        for fail in [false, true] {
            let finishes = Arc::new(Mutex::new(0));
            let (tx, receiver) = flume::unbounded();
            drop(tx);
            let mut processor = SignalChainStepProcessor {
                encoder: Some(Box::new(DrainEncoder {
                    finishes: finishes.clone(),
                    fail,
                })),
                resampler: None,
                receiver,
                overflow: vec![],
                drained: false,
            };
            let mut byte = [0];
            if fail {
                assert_eq!(
                    processor.read(&mut byte).unwrap_err().kind(),
                    std::io::ErrorKind::Other
                );
            } else {
                for expected in [5, 6, 7] {
                    assert_eq!(processor.read(&mut byte).unwrap(), 1);
                    assert_eq!(byte[0], expected);
                }
            }
            assert_eq!(processor.read(&mut byte).unwrap(), 0);
            assert_eq!(*finishes.lock().unwrap(), 1);
        }
    }

    #[test_log::test]
    fn test_terminal_decoder_error_is_not_clean_eof() {
        let (tx, receiver) = flume::unbounded();
        tx.send(Err(moosicbox_audio_decoder::DecodeError::InvalidSource))
            .unwrap();
        drop(tx);
        let mut processor = SignalChainStepProcessor {
            encoder: Some(Box::new(FailingEncoder)),
            resampler: None,
            receiver,
            overflow: vec![9],
            drained: false,
        };
        let mut byte = [0];
        assert_eq!(processor.read(&mut byte).unwrap(), 1);
        assert_eq!(byte, [9]);
        let error = processor.read(&mut byte).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert!(
            error
                .get_ref()
                .unwrap()
                .is::<moosicbox_audio_decoder::DecodeError>()
        );
        assert_eq!(processor.read(&mut byte).unwrap(), 0);
    }

    struct FailingEncoder;
    impl AudioEncoder for FailingEncoder {
        fn encode(
            &mut self,
            _audio: AudioBuffer<f32>,
        ) -> Result<bytes::Bytes, moosicbox_audio_output::AudioOutputError> {
            Err(moosicbox_audio_output::AudioOutputError::Interrupt)
        }
        fn spec(&self) -> AudioSpec {
            AudioSpec::new(48_000, layouts::CHANNEL_LAYOUT_MONO)
        }
    }

    #[test_log::test]
    fn test_disconnect_tail_encoder_error_propagates_once() {
        let spec = AudioSpec::new(44_100, layouts::CHANNEL_LAYOUT_MONO);
        let (tx, receiver) = flume::unbounded();
        tx.send(Ok(partial_input(spec.clone()))).unwrap();
        drop(tx);
        let mut processor = SignalChainStepProcessor {
            encoder: Some(Box::new(FailingEncoder)),
            resampler: Some(Resampler::new(spec, 48_000, 4096)),
            receiver,
            overflow: vec![],
            drained: false,
        };
        let mut buf = [0; 8];
        assert_eq!(
            processor.read(&mut buf).unwrap_err().kind(),
            std::io::ErrorKind::Other
        );
        assert_eq!(processor.read(&mut buf).unwrap(), 0);
    }

    #[test_log::test]
    fn test_resampling_materializes_mono_and_stereo_once() {
        for channels in [layouts::CHANNEL_LAYOUT_MONO, layouts::CHANNEL_LAYOUT_STEREO] {
            let input_spec = AudioSpec::new(44_100, channels.clone());
            let output_spec = AudioSpec::new(48_000, channels);
            let mut input = AudioBuffer::<f32>::new(input_spec.clone(), 4096);
            input.render_silence(Some(4096));
            for plane in input.iter_planes_mut() {
                plane.fill(0.25);
            }
            let calls = Arc::new(Mutex::new(0));
            let (tx, receiver) = flume::unbounded();
            tx.send(Ok(input)).unwrap();
            let mut processor = SignalChainStepProcessor {
                encoder: Some(Box::new(InspectEncoder {
                    spec: output_spec,
                    calls: calls.clone(),
                })),
                resampler: Some(Resampler::new(input_spec, 48_000, 4096)),
                receiver,
                overflow: vec![],
                drained: false,
            };
            let mut first = [0; 2];
            assert_eq!(processor.read(&mut first).unwrap(), 2);
            assert_eq!(first, [1, 2]);
            let mut rest = [0; 8];
            assert_eq!(processor.read(&mut rest).unwrap(), 2);
            assert_eq!(&rest[..2], &[3, 4]);
            assert_eq!(*calls.lock().unwrap(), 1);
        }
    }

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
        assert!(step.decode_options.decoder.gapless);
        assert!(step.decode_options.decoder.verify);
        assert!(step.seek.is_none());
    }

    #[test_log::test]
    fn test_signal_chain_step_default_has_defaults() {
        let step = SignalChainStep::default();
        assert!(step.hint.is_none());
        assert!(step.audio_output_handler.is_none());
        assert!(step.encoder.is_none());
        assert!(step.resampler.is_none());
        assert!(step.decode_options.decoder.gapless);
        assert!(step.decode_options.decoder.verify);
        assert!(step.seek.is_none());
    }

    #[test_log::test]
    fn test_signal_chain_step_with_verify_sets_value() {
        let step = SignalChainStep::new().with_verify(false);
        assert!(!step.decode_options.decoder.verify);

        let step = SignalChainStep::new().with_verify(true);
        assert!(step.decode_options.decoder.verify);
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
        assert!(!step.decode_options.decoder.verify);
        assert_eq!(step.seek, Some(20.0));
    }

    #[test_log::test]
    fn test_signal_chain_process_empty_chain_returns_error() {
        {
            use std::io::Cursor;

            // Create an empty signal chain
            let chain = SignalChain::new();

            // Create a minimal media source (empty cursor)
            let media_source: Box<dyn MediaSource> = Box::new(Cursor::new(vec![]));

            // Processing an empty chain should return SignalChainError::Empty
            let result = chain.process(media_source);
            assert!(result.is_err());

            // Verify it's the Empty error variant by checking the string representation
            let err = result.err().unwrap();
            let err_str = err.to_string();
            assert!(
                err_str.to_lowercase().contains("empty"),
                "Expected 'empty' in error message, got: {err_str}"
            );
        }
    }

    #[test_log::test]
    fn test_signal_chain_error_display() {
        // Test that SignalChainError::Empty displays correctly
        let error = SignalChainError::Empty;
        let err_str = error.to_string().to_lowercase();
        assert!(
            err_str.contains("empty"),
            "Expected 'empty' in error message, got: {err_str}"
        );
    }

    #[test_log::test]
    fn test_signal_chain_processor_error_display() {
        {
            // Test that SignalChainProcessorError variants display correctly
            use moosicbox_audio_decoder::AudioDecodeError;

            // Use the OpenStream variant which exists in AudioDecodeError
            let audio_error = SignalChainProcessorError::AudioDecode(AudioDecodeError::OpenStream);
            assert_ne!(audio_error.to_string().len(), 0);
        }
    }

    #[test_log::test]
    fn test_signal_chain_with_encoder_on_empty_chain_does_nothing() {
        // Test that calling with_encoder on an empty chain doesn't panic
        let chain = SignalChain::new().with_encoder(|| {
            // This shouldn't be called since the chain is empty
            panic!("Encoder factory should not be called on empty chain")
        });
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_with_audio_decode_handler_on_empty_chain_does_nothing() {
        // Test that calling with_audio_decode_handler on an empty chain doesn't panic
        let chain = SignalChain::new().with_audio_decode_handler(|| {
            // This shouldn't be called since the chain is empty
            panic!("Handler factory should not be called on empty chain")
        });
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_with_verify_on_empty_chain_does_nothing() {
        // Test that calling with_verify on an empty chain doesn't panic
        let chain = SignalChain::new().with_verify(false);
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_with_seek_on_empty_chain_does_nothing() {
        // Test that calling with_seek on an empty chain doesn't panic
        let chain = SignalChain::new().with_seek(Some(10.0));
        assert_eq!(chain.steps.len(), 0);
    }

    #[test_log::test]
    fn test_signal_chain_add_encoder_step_creates_step_with_encoder() {
        {
            use moosicbox_audio_output::AudioOutputError;

            /// Mock encoder for testing `add_encoder_step`
            struct MockEncoder;

            impl moosicbox_audio_output::encoder::AudioEncoder for MockEncoder {
                fn encode(
                    &mut self,
                    _decoded: symphonia::core::audio::AudioBuffer<f32>,
                ) -> Result<bytes::Bytes, AudioOutputError> {
                    Ok(bytes::Bytes::new())
                }

                fn spec(&self) -> symphonia::core::audio::AudioSpec {
                    symphonia::core::audio::AudioSpec::new(
                        44100,
                        symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
                    )
                }
            }

            let chain = SignalChain::new().add_encoder_step(|| {
                // Return a minimal encoder for testing
                Box::new(MockEncoder)
            });
            assert_eq!(chain.steps.len(), 1);
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_seek_returns_error() {
        {
            use std::io::{Seek, SeekFrom};

            // Create a processor with a channel that will never receive
            let (_tx, rx) = flume::unbounded();
            let mut processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![],
                drained: false,
            };

            // Seeking should always return an error
            let result = processor.seek(SeekFrom::Start(0));
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::Other);
            assert!(err.to_string().contains("does not support seeking"));
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_is_not_seekable() {
        {
            use symphonia::core::io::MediaSource;

            // Create a processor with a channel that will never receive
            let (_tx, rx) = flume::unbounded();
            let processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![],
                drained: false,
            };

            // MediaSource should report as not seekable
            assert!(!processor.is_seekable());
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_byte_len_returns_none() {
        {
            use symphonia::core::io::MediaSource;

            // Create a processor with a channel that will never receive
            let (_tx, rx) = flume::unbounded();
            let processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![],
                drained: false,
            };

            // MediaSource should report unknown byte length
            assert!(processor.byte_len().is_none());
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_read_from_overflow() {
        {
            use std::io::Read;

            // Create a processor with pre-populated overflow
            let (_tx, rx) = flume::unbounded::<
                Result<AudioBuffer<f32>, moosicbox_audio_decoder::DecodeError>,
            >();
            let mut processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![1, 2, 3, 4, 5, 6, 7, 8],
                drained: false,
            };

            // Read should pull from overflow first
            let mut buf = [0u8; 4];
            let result = processor.read(&mut buf);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 4);
            assert_eq!(&buf, &[1, 2, 3, 4]);

            // Overflow should have remaining bytes
            assert_eq!(processor.overflow, vec![5, 6, 7, 8]);
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_read_entire_overflow() {
        {
            use std::io::Read;

            // Create a processor with pre-populated overflow
            let (_tx, rx) = flume::unbounded::<
                Result<AudioBuffer<f32>, moosicbox_audio_decoder::DecodeError>,
            >();
            let mut processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![10, 20, 30],
                drained: false,
            };

            // Read buffer larger than overflow
            let mut buf = [0u8; 10];
            let result = processor.read(&mut buf);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 3);
            assert_eq!(&buf[..3], &[10, 20, 30]);

            // Overflow should be empty
            assert_eq!(processor.overflow.len(), 0);
        }
    }

    #[test_log::test]
    fn test_signal_chain_step_processor_read_timeout_when_no_data() {
        {
            use std::io::Read;

            // Create a processor with empty overflow and a receiver that won't receive
            let (tx, rx) = flume::unbounded::<
                Result<AudioBuffer<f32>, moosicbox_audio_decoder::DecodeError>,
            >();
            let mut processor = SignalChainStepProcessor {
                encoder: None,
                resampler: None,
                receiver: rx,
                overflow: vec![],
                drained: false,
            };

            // Keep the sender alive: temporary starvation is not EOF.
            let sender = tx;

            // Read should timeout and return an error
            let mut buf = [0u8; 10];
            let result = processor.read(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
            assert!(!processor.drained);
            drop(sender);
            assert_eq!(processor.read(&mut buf).unwrap(), 0);
        }
    }
}
