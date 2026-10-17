//! Opus audio encoder implementation.

#![allow(clippy::module_name_repetitions)]

use super::AudioEncoder;
use crate::{AudioOutputError, to_samples};
use bytes::Bytes;
use moosicbox_audio_encoder::opus_stream::OpusStreamEncoder;
pub use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode,
};
use std::sync::{Arc, Mutex};
use symphonia::core::audio::{Audio, AudioBuffer, AudioSpec};

#[derive(Clone, Default)]
struct EncodedSink(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for EncodedSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("Opus output lock poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Fallible explicitly selected Opus encoder with accurate gapless Ogg finalization.
/// Legacy [`OpusEncoder`] remains available under the `opus` compatibility feature.
pub struct SelectedOpusEncoder {
    encoder: OpusStreamEncoder<EncodedSink>,
    sink: EncodedSink,
    spec: AudioSpec,
    resampler: Option<moosicbox_resampler::Resampler<f32>>,
    input_spec: Option<AudioSpec>,
    input_frames: u64,
    resampled_frames: u64,
    finished: bool,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
}
impl SelectedOpusEncoder {
    /// Construct a selected encoder. Native-only builds never instantiate libopus.
    ///
    /// # Errors
    /// * Returns unsupported configuration/backend or encoder initialization errors.
    pub fn new(config: OpusEncodeConfig) -> Result<Self, AudioOutputError> {
        let sink = EncodedSink::default();
        let spec = AudioSpec::new(
            config.sample_rate,
            if config.channels == 1 {
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_MONO
            } else {
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO
            },
        );
        let encoder = OpusStreamEncoder::new(sink.clone(), config)?;
        Ok(Self {
            encoder,
            sink,
            spec,
            resampler: None,
            input_spec: None,
            input_frames: 0,
            resampled_frames: 0,
            finished: false,
            writer: None,
        })
    }
    /// Construct an encoder that writes Ogg bytes to the supplied sink.
    ///
    /// # Errors
    /// * Returns the same initialization errors as [`Self::new`].
    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(
        writer: W,
        config: OpusEncodeConfig,
    ) -> Result<Self, AudioOutputError> {
        let mut encoder = Self::new(config)?;
        encoder.writer = Some(Box::new(writer));
        Ok(encoder)
    }
    /// Return the implementation selected at construction.
    #[must_use]
    pub const fn backend(&self) -> OpusEncodeBackend {
        self.encoder.backend()
    }
    /// Return the requested coding mode (no automatic native mode switching).
    #[must_use]
    pub const fn mode(&self) -> OpusEncodeMode {
        self.encoder.config().mode
    }

    fn take(&self) -> Result<Bytes, AudioOutputError> {
        let mut bytes = self
            .sink
            .0
            .lock()
            .map_err(|_| AudioOutputError::Interrupt)?;
        Ok(std::mem::take(&mut *bytes).into())
    }
}
impl AudioEncoder for SelectedOpusEncoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        if self.finished {
            return Err(AudioOutputError::StreamEnd);
        }
        if decoded.spec().channels() != self.spec.channels() {
            return Err(AudioOutputError::UnsupportedChannels(
                decoded.spec().channels().count(),
            ));
        }
        if self
            .input_spec
            .as_ref()
            .is_some_and(|spec| spec != decoded.spec())
        {
            return Err(std::io::Error::other("Opus input specification changed midstream").into());
        }
        if self.input_spec.is_none() {
            self.input_spec = Some(decoded.spec().clone());
            if decoded.spec().rate() != self.spec.rate() {
                self.resampler = Some(moosicbox_resampler::Resampler::new(
                    decoded.spec().clone(),
                    self.spec.rate() as usize,
                    decoded.capacity() as u64,
                ));
            }
        }
        self.input_frames = self
            .input_frames
            .checked_add(u64::try_from(decoded.frames()).map_err(std::io::Error::other)?)
            .ok_or_else(|| std::io::Error::other("Opus input length overflow"))?;
        let target = self
            .input_frames
            .checked_mul(u64::from(self.spec.rate()))
            .ok_or_else(|| std::io::Error::other("Opus resampled length overflow"))?
            / u64::from(decoded.spec().rate());
        if let Some(resampler) = &mut self.resampler {
            if let Some(pcm) = resampler.resample(&decoded) {
                let channels = self.spec.channels().count();
                let frames = pcm.len() / channels;
                let retain = frames.min(
                    usize::try_from(target.saturating_sub(self.resampled_frames))
                        .unwrap_or(usize::MAX),
                );
                self.encoder.write(&pcm[..retain * channels])?;
                self.resampled_frames += retain as u64;
            }
        } else {
            self.encoder.write(&to_samples(&decoded))?;
        }
        self.take()
    }
    fn finish(&mut self) -> Result<Bytes, AudioOutputError> {
        if self.finished {
            return Ok(Bytes::new());
        }
        self.finished = true;
        if let Some(resampler) = &mut self.resampler {
            let input_rate = self
                .input_spec
                .as_ref()
                .ok_or_else(|| std::io::Error::other("missing Opus input rate"))?
                .rate();
            let target = self
                .input_frames
                .checked_mul(u64::from(self.spec.rate()))
                .ok_or_else(|| std::io::Error::other("Opus resampled length overflow"))?
                / u64::from(input_rate);
            let channels = self.spec.channels().count();
            while let Some(pcm) = resampler.flush() {
                let retain = (pcm.len() / channels).min(
                    usize::try_from(target.saturating_sub(self.resampled_frames))
                        .unwrap_or(usize::MAX),
                );
                self.encoder.write(&pcm[..retain * channels])?;
                self.resampled_frames += retain as u64;
            }
            if self.resampled_frames != target {
                return Err(std::io::Error::other(
                    "resampler did not produce the expected Opus input length",
                )
                .into());
            }
        }
        self.encoder.finish()?;
        self.take()
    }
    fn spec(&self) -> AudioSpec {
        self.spec.clone()
    }
}

impl crate::AudioWrite for SelectedOpusEncoder {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        let bytes = self.encode(decoded)?;
        if let Some(writer) = &mut self.writer {
            writer.write_all(&bytes)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> Result<(), AudioOutputError> {
        let bytes = self.finish()?;
        if let Some(writer) = &mut self.writer {
            writer.write_all(&bytes)?;
            writer.flush()?;
        }
        Ok(())
    }
    fn handle(&self) -> crate::AudioHandle {
        unimplemented!("Opus encoder has no playback commands")
    }
}
impl moosicbox_audio_decoder::AudioDecode for SelectedOpusEncoder {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _: &symphonia::core::packet::Packet,
        _: &symphonia::core::formats::Track,
    ) -> Result<(), moosicbox_audio_decoder::AudioDecodeError> {
        crate::AudioWrite::write(self, decoded)
            .map(|_| ())
            .map_err(|error| moosicbox_audio_decoder::AudioDecodeError::Other(Box::new(error)))
    }
    fn flush(&mut self) -> Result<(), moosicbox_audio_decoder::AudioDecodeError> {
        crate::AudioWrite::flush(self)
            .map_err(|error| moosicbox_audio_decoder::AudioDecodeError::Other(Box::new(error)))
    }
}

#[cfg(feature = "opus")]
mod legacy {
    use super::Mutex;
    use std::sync::RwLock;

    use bytes::Bytes;
    use moosicbox_audio_decoder::{
        AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
    };
    use moosicbox_audio_encoder::opus::{
        OPUS_STREAM_COMMENTS_HEADER, OPUS_STREAM_IDENTIFICATION_HEADER, encoder_opus,
    };
    use moosicbox_stream_utils::{ByteStream, ByteWriter};
    use ogg::{PacketWriteEndInfo, PacketWriter};
    use switchy_async::task::JoinHandle;
    use symphonia::core::{
        audio::{Audio, AudioBuffer, AudioSpec},
        formats::Track,
        packet::Packet,
        units::Duration,
    };

    use crate::{AudioOutputError, AudioWrite, to_samples};
    use moosicbox_resampler::Resampler;

    use super::AudioEncoder;

    const STEREO_20MS: usize = 48000 * 2 * 20 / 1000;

    /// Opus audio encoder that converts decoded audio to Opus format.
    ///
    /// This encoder uses the Opus library to encode audio samples
    /// and supports automatic resampling to 48kHz (Opus standard).
    pub struct OpusEncoder<'a> {
        buf: [f32; STEREO_20MS],
        buf_len: usize,
        packet_writer: PacketWriter<'a, Vec<u8>>,
        last_write_pos: usize,
        serial: u32,
        absgp: u64,
        time: usize,
        bytes_read: usize,
        resampler: Option<RwLock<Resampler<f32>>>,
        input_rate: Option<u32>,
        finished: bool,
        resample_rate: Option<u32>,
        output_rate: usize,
        duration: Option<Duration>,
        writer: Option<Box<dyn std::io::Write + Send + Sync>>,
        encoder: Mutex<opus::Encoder>,
    }

    impl OpusEncoder<'_> {
        /// Creates a new Opus encoder with default settings.
        ///
        /// Default output sample rate is 48000 Hz (Opus standard).
        ///
        /// # Panics
        ///
        /// * If fails to get the opus encoder
        #[must_use]
        pub fn new() -> Self {
            let packet_writer = PacketWriter::new(Vec::new());

            Self {
                buf: [0.0; STEREO_20MS],
                buf_len: 0,
                packet_writer,
                last_write_pos: 0,
                serial: 0,
                absgp: 0,
                time: 0,
                bytes_read: 0,
                resampler: None,
                input_rate: None,
                finished: false,
                resample_rate: None,
                output_rate: 48000,
                duration: None,
                writer: None,
                encoder: Mutex::new(encoder_opus().unwrap()),
            }
        }

        /// Creates a new Opus encoder with a custom writer.
        ///
        /// # Arguments
        /// * `writer` - Output writer for encoded Opus data
        ///
        /// # Panics
        ///
        /// * If fails to get the opus encoder
        #[must_use]
        pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
            let mut x = Self::new();
            x.writer.replace(Box::new(writer));
            x
        }

        /// Initializes the resampler if needed based on input audio spec.
        ///
        /// # Arguments
        /// * `spec` - Input audio signal specification
        /// * `duration` - Audio duration in samples
        pub fn init_resampler(&mut self, spec: &AudioSpec, duration: Duration) -> &Self {
            if self.resample_rate.is_none_or(|r| r != spec.rate())
                && self.output_rate != spec.rate() as usize
            {
                log::debug!(
                    "Initializing resampler with rate={} duration={}",
                    spec.rate(),
                    duration,
                );
                self.input_rate.replace(spec.rate());
                self.duration.replace(duration);
                self.resample_rate.replace(spec.rate());
                self.resampler.replace(RwLock::new(Resampler::new(
                    spec.clone(),
                    self.output_rate,
                    duration.get(),
                )));
            }
            self
        }

        /// Opens the encoder with the specified audio specification.
        ///
        /// This initializes the resampler and prepares the encoder for encoding.
        ///
        /// # Arguments
        /// * `spec` - Audio signal specification
        /// * `duration` - Audio duration in samples
        #[must_use]
        pub fn open(
            mut self,
            spec: impl std::borrow::Borrow<AudioSpec>,
            duration: Duration,
        ) -> Self {
            self.init_resampler(spec.borrow(), duration);
            self
        }

        fn encode_output(&mut self, input: &[f32], buf_size: usize) -> Bytes {
            let mut read = 0;
            let mut written = vec![];
            let mut output_buf = vec![0_u8; buf_size];

            loop {
                log::trace!(
                    "Encoding bytes to OPUS input_len={} buf_size={}",
                    input.len(),
                    buf_size
                );
                let info = moosicbox_audio_encoder::opus::encode_opus_float(
                    &mut self.encoder.lock().unwrap(),
                    &input[read..read + buf_size],
                    &mut output_buf,
                )
                .expect("Failed to convert");

                log::trace!(
                    "Encoded bytes to OPUS output_size={}/{buf_size} input_consumed={}",
                    info.output_size,
                    info.input_consumed
                );

                let len = info.output_size;
                let section = &output_buf[..info.output_size];

                if self.absgp == 0 {
                    // https://datatracker.ietf.org/doc/html/rfc7845#section-5.1
                    log::debug!("Writing OPUS identification header packet");
                    self.packet_writer
                        .write_packet(
                            OPUS_STREAM_IDENTIFICATION_HEADER.to_vec(),
                            self.serial,
                            PacketWriteEndInfo::EndPage,
                            self.absgp,
                        )
                        .unwrap();

                    // https://datatracker.ietf.org/doc/html/rfc7845#section-5.2
                    log::debug!("Writing OPUS comments header packet");
                    self.packet_writer
                        .write_packet(
                            OPUS_STREAM_COMMENTS_HEADER.to_vec(),
                            self.serial,
                            PacketWriteEndInfo::EndPage,
                            self.absgp,
                        )
                        .unwrap();
                }

                log::trace!("Writing OPUS packet of size {}", section.len());
                self.packet_writer
                    .write_packet(
                        section.to_vec(),
                        self.serial,
                        PacketWriteEndInfo::EndPage,
                        self.absgp + (info.input_consumed / 2) as u64,
                    )
                    .expect("Failed to write packet");

                self.absgp += (info.input_consumed / 2) as u64;

                written.extend_from_slice(&self.write_new_packet_writer_contents());

                read += buf_size;
                if self.time.is_multiple_of(1000) {
                    log::debug!(
                        "Info: read={} written len={} input_consumed={} output_size={} len={}",
                        read,
                        written.len(),
                        buf_size,
                        len,
                        self.bytes_read
                    );
                }

                if read >= input.len() {
                    break;
                }
            }
            written.into()
        }

        fn write_new_packet_writer_contents(&mut self) -> Bytes {
            let writer_contents = self.packet_writer.inner();

            log::debug!(
                "last_write_pos={} current packet_writer len={}",
                self.last_write_pos,
                writer_contents.len()
            );
            if writer_contents.len() > self.last_write_pos {
                let written_section = &writer_contents[self.last_write_pos..];
                let written_section = written_section.to_vec();
                self.last_write_pos = writer_contents.len();

                log::trace!("OPUS packet writer data len={}", writer_contents.len());

                Bytes::from(written_section)
            } else {
                Bytes::new()
            }
        }

        fn write_samples(&mut self, decoded: Vec<f32>) -> Bytes {
            let samples = [self.buf[..self.buf_len].to_vec(), decoded].concat();

            self.buf_len = 0;

            let mut written = vec![];

            for chunk in samples.chunks(STEREO_20MS) {
                if chunk.len() < STEREO_20MS {
                    self.buf_len = chunk.len();
                    self.buf[..self.buf_len].copy_from_slice(chunk);
                } else {
                    self.time += 20;
                    log::debug!("Encoding OPUS chunk...");
                    let bytes = self.encode_output(chunk, STEREO_20MS);
                    let byte_count = bytes.len();
                    log::debug!("Encoded OPUS chunk to {byte_count} bytes");
                    written.extend_from_slice(&bytes);
                    self.bytes_read += byte_count;
                    if self.time.is_multiple_of(1000) {
                        log::debug!("time: {}", self.time / 1000);
                    }
                }
            }

            log::debug!("Encoded OPUS chunks to a total of {} bytes", written.len());

            written.into()
        }

        fn resample_if_needed(
            &mut self,
            decoded: &AudioBuffer<f32>,
        ) -> Result<Vec<f32>, AudioOutputError> {
            if self.finished {
                return Err(AudioOutputError::StreamEnd);
            }
            let spec = decoded.spec();
            if spec.channels() != self.spec().channels() {
                return Err(AudioOutputError::UnsupportedChannels(
                    spec.channels().count(),
                ));
            }
            let duration = Duration::from(decoded.capacity() as u64);

            self.init_resampler(spec, duration);

            if let Some(resampler) = &self.resampler {
                log::debug!(
                    "Resampling input_rate={:?} output_rate={} duration={:?}",
                    self.input_rate,
                    self.output_rate,
                    self.duration
                );

                Ok(resampler
                    .write()
                    .unwrap()
                    .resample(decoded)
                    .map_or_else(Vec::new, <[_]>::to_vec))
            } else {
                log::debug!(
                    "Passing through audio frames={} duration={duration} rate={} channels={:?} channels_count={}",
                    decoded.frames(),
                    spec.rate(),
                    spec.channels(),
                    spec.channels().count(),
                );
                Ok(to_samples(decoded))
            }
        }
    }

    impl Default for OpusEncoder<'_> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl AudioEncoder for OpusEncoder<'_> {
        fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
            log::debug!("OpusEncoder encode {} frames", decoded.frames());

            let decoded = self.resample_if_needed(&decoded)?;

            Ok(self.write_samples(decoded))
        }

        fn finish(&mut self) -> Result<Bytes, AudioOutputError> {
            if self.finished {
                return Ok(Bytes::new());
            }
            self.finished = true;
            let mut tail = Vec::new();
            if let Some(resampler) = &self.resampler {
                let mut resampler = resampler.write().map_err(|_| AudioOutputError::Interrupt)?;
                while let Some(samples) = resampler.flush() {
                    tail.extend_from_slice(samples);
                }
            }
            let mut bytes = self.write_samples(tail).to_vec();
            if self.buf_len > 0 {
                let mut frame = vec![0.0; STEREO_20MS];
                frame[..self.buf_len].copy_from_slice(&self.buf[..self.buf_len]);
                self.buf_len = 0;
                bytes.extend_from_slice(&self.write_samples(frame));
            }
            Ok(bytes.into())
        }

        fn spec(&self) -> AudioSpec {
            AudioSpec::new(
                u32::try_from(self.output_rate).unwrap(),
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            )
        }
    }

    impl AudioDecode for OpusEncoder<'_> {
        fn decoded(
            &mut self,
            decoded: AudioBuffer<f32>,
            _packet: &Packet,
            _track: &Track,
        ) -> Result<(), AudioDecodeError> {
            if self.writer.is_none() {
                return Ok(());
            }

            let bytes = self.encode(decoded).map_err(|e| {
                AudioDecodeError::IO(std::io::Error::other(format!("Failed to encode: {e:?}")))
            })?;

            if let Some(writer) = self.writer.as_mut() {
                let mut count = 0;
                loop {
                    count += match writer.write(&bytes[count..]) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            log::error!("Failed to write: {e:?}");
                            return Err(AudioDecodeError::StreamClosed);
                        }
                    };
                    if count >= bytes.len() {
                        break;
                    }
                }
            }

            Ok(())
        }
        fn flush(&mut self) -> Result<(), AudioDecodeError> {
            AudioWrite::flush(self).map_err(|error| AudioDecodeError::Other(Box::new(error)))
        }
    }

    impl AudioWrite for OpusEncoder<'_> {
        fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
            if self.writer.is_none() {
                return Ok(0);
            }

            let bytes = self.encode(decoded)?;

            if let Some(writer) = self.writer.as_mut() {
                let mut count = 0;
                loop {
                    count += match writer.write(&bytes[count..]) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            log::error!("Failed to write: {e:?}");
                            return Err(AudioOutputError::StreamClosed);
                        }
                    };
                    if count >= bytes.len() {
                        break;
                    }
                }
            }

            Ok(bytes.len())
        }

        fn flush(&mut self) -> Result<(), AudioOutputError> {
            let bytes = self.finish()?;
            if let Some(writer) = &mut self.writer {
                writer.write_all(&bytes)?;
                writer.flush()?;
            }
            Ok(())
        }

        fn handle(&self) -> crate::AudioHandle {
            unimplemented!("OpusEncoder does not support command handling")
        }
    }

    /// Encodes an audio file to Opus format and returns a byte stream.
    ///
    /// This function spawns a background task to encode the audio file
    /// and returns a stream that can be read as the encoding progresses.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file to encode
    #[must_use]
    pub fn encode_opus_stream(path: &str) -> ByteStream {
        let writer = ByteWriter::default();
        let stream = writer.stream();

        encode_opus_spawn(path, writer);

        stream
    }

    /// Spawns a background task to encode an audio file to Opus format.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file to encode
    /// * `writer` - Output writer for encoded Opus data
    pub fn encode_opus_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
        path: &str,
        writer: T,
    ) -> JoinHandle<()> {
        let path = path.to_string();
        switchy_async::runtime::Handle::current().spawn_blocking_with_name(
            "audio_decoder: encode_opus",
            move || {
                encode_opus(&path, writer);
            },
        )
    }

    /// Encodes an audio file to Opus format.
    ///
    /// This function blocks until encoding is complete.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file to encode
    /// * `writer` - Output writer for encoded Opus data
    pub fn encode_opus<T: std::io::Write + Send + Sync + Clone + 'static>(path: &str, writer: T) {
        let mut audio_decode_handler =
            AudioDecodeHandler::new().with_output(Box::new(move |spec, duration| {
                Ok(Box::new(
                    OpusEncoder::with_writer(writer.clone()).open(&spec, duration),
                ))
            }));

        if let Err(err) =
            decode_file_path_str(path, &mut audio_decode_handler, true, true, None, None)
        {
            log::error!("Failed to encode to opus: {err:?}");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use symphonia::core::audio::AudioMut as _;

        #[test_log::test]
        fn finish_emits_partial_frame_once() {
            let spec = AudioSpec::new(
                48_000,
                symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO,
            );
            let mut audio = AudioBuffer::<f32>::new(spec, 100);
            audio.render_silence(Some(100));
            for plane in audio.iter_planes_mut() {
                plane.fill(0.25);
            }
            let mut encoder = OpusEncoder::new();
            assert!(encoder.encode(audio).unwrap().is_empty());
            assert_eq!(encoder.buf_len, 200);
            assert!(!encoder.finish().unwrap().is_empty());
            assert_eq!(encoder.buf_len, 0);
            assert!(encoder.finish().unwrap().is_empty());
        }
    }
}

#[cfg(feature = "opus")]
pub use legacy::*;
