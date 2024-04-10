use flume::Receiver;
use symphonia::core::{
    audio::{AudioBuffer, Signal},
    io::{MediaSource, MediaSourceStream},
    probe::Hint,
};
use thiserror::Error;

use crate::{
    output::{AudioEncoder, AudioOutputError, AudioOutputHandler},
    unsync::{play_media_source, PlaybackError},
};

#[derive(Debug, Error)]
pub enum SignalChainError {
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error("SignalChain is empty")]
    Empty,
}

type CreateAudioOutputStream = Box<dyn (FnOnce() -> AudioOutputHandler) + Send + 'static>;
type CreateAudioEncoder = Box<dyn (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>;

pub struct SignalChain {
    steps: Vec<SignalChainStep>,
}

impl SignalChain {
    pub fn new() -> Self {
        Self { steps: vec![] }
    }

    pub fn with_hint(mut self, hint: Hint) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_hint(hint));
        }
        self
    }

    pub fn with_audio_output_handler<F: (FnOnce() -> AudioOutputHandler) + Send + 'static>(
        mut self,
        handler: F,
    ) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_audio_output_handler(handler));
        }

        self
    }

    pub fn with_encoder<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_encoder(encoder));
        }
        self
    }

    pub fn with_verify(mut self, verify: bool) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_verify(verify));
        }
        self
    }

    pub fn with_seek(mut self, seek: Option<f64>) -> Self {
        if let Some(step) = self.steps.pop() {
            self.steps.push(step.with_seek(seek));
        }
        self
    }

    pub fn next_step(mut self) -> Self {
        self.steps.push(SignalChainStep::new());
        self
    }

    pub fn add_step(mut self, step: SignalChainStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn add_encoder_step<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        self.steps
            .push(SignalChainStep::new().with_encoder(encoder));
        self
    }

    pub fn process(
        mut self,
        media_source: Box<dyn MediaSource>,
    ) -> Result<Box<dyn MediaSource>, SignalChainError> {
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

pub struct SignalChainStep {
    hint: Option<Hint>,
    audio_output_handler: Option<CreateAudioOutputStream>,
    encoder: Option<CreateAudioEncoder>,
    enable_gapless: bool,
    verify: bool,
    seek: Option<f64>,
}

impl SignalChainStep {
    pub fn new() -> Self {
        Self {
            hint: None,
            audio_output_handler: None,
            encoder: None,
            enable_gapless: true,
            verify: true,
            seek: None,
        }
    }

    pub fn with_hint(mut self, hint: Hint) -> Self {
        self.hint.replace(hint);
        self
    }

    pub fn with_audio_output_handler<F: (FnOnce() -> AudioOutputHandler) + Send + 'static>(
        mut self,
        handler: F,
    ) -> Self {
        self.audio_output_handler.replace(Box::new(handler));
        self
    }

    pub fn with_encoder<F: (FnOnce() -> Box<dyn AudioEncoder>) + Send + 'static>(
        mut self,
        encoder: F,
    ) -> Self {
        self.encoder.replace(Box::new(encoder));
        self
    }

    pub fn with_verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }

    pub fn with_seek(mut self, seek: Option<f64>) -> Self {
        self.seek = seek;
        self
    }

    pub fn process(
        self,
        media_source: Box<dyn MediaSource>,
    ) -> Result<SignalChainStepProcessor, SignalChainError> {
        let hint = self.hint.unwrap_or_else(|| Hint::new());
        let mss = MediaSourceStream::new(media_source, Default::default());

        let receiver = play_media_source(
            mss,
            &hint,
            self.enable_gapless,
            self.verify,
            None,
            self.seek,
        )?;

        let encoder = if let Some(get_encoder) = self.encoder {
            get_encoder()
        } else {
            return Err(SignalChainError::Empty);
        };

        Ok(SignalChainStepProcessor {
            encoder,
            receiver,
            overflow: vec![],
        })
    }
}

pub struct SignalChainStepProcessor {
    encoder: Box<dyn AudioEncoder>,
    receiver: Receiver<AudioBuffer<f32>>,
    overflow: Vec<u8>,
}

impl SignalChainStepProcessor {}

impl std::io::Seek for SignalChainStepProcessor {
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
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

            log::debug!("Encoding frames...");
            let bytes = self
                .encoder
                .encode(audio)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            log::debug!("Encoded into {} bytes", bytes.len());

            if !bytes.is_empty() {
                break bytes;
            }
        };

        let (bytes_now, overflow) = bytes.split_at(std::cmp::min(buf.len(), bytes.len()));

        log::debug!(
            "buf len={} bytes_now len={} overflow len={}",
            buf.len(),
            bytes_now.len(),
            overflow.len()
        );
        buf[..bytes_now.len()].copy_from_slice(&bytes_now);
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

#[derive(Debug, Error)]
pub enum SignalChainProcessorError {
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    AudioOutput(#[from] AudioOutputError),
}
