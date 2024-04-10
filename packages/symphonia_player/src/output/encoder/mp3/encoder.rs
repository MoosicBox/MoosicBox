use std::sync::{Arc, RwLock};

use crate::output::{AudioEncoder, AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use lazy_static::lazy_static;
use moosicbox_converter::mp3::encoder_mp3;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::*;
use symphonia::core::conv::IntoSample;
use symphonia::core::units::Duration;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct Mp3Encoder {
    resampler: Arc<RwLock<Option<Resampler<i16>>>>,
    writer: Option<Box<dyn std::io::Write + Send + Sync>>,
    encoder: mp3lame_encoder::Encoder,
}

impl Mp3Encoder {
    pub fn new() -> Self {
        Self {
            resampler: Arc::new(RwLock::new(None)),
            writer: None,
            encoder: encoder_mp3().unwrap(),
        }
    }

    pub fn with_writer<W: std::io::Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            resampler: Arc::new(RwLock::new(None)),
            writer: Some(Box::new(writer)),
            encoder: encoder_mp3().unwrap(),
        }
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        if spec.rate != 48000 {
            self.resampler
                .write()
                .unwrap()
                .replace(Resampler::<i16>::new(spec, 48000_usize, duration));
        } else {
            self.resampler.write().unwrap().take();
        }
    }

    fn encode_output(&mut self, buf: &[i16]) -> Bytes {
        let mut read = 0;
        let mut written = vec![];
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            log::trace!("Encoding {} bytes {read}..{end}", end - read);
            match moosicbox_converter::mp3::encode_mp3(&mut self.encoder, &buf[read..end]) {
                Ok((output, info)) => {
                    log::trace!(
                        "input_consumed={} output_size={} (output len={})",
                        info.input_consumed,
                        info.output_size,
                        output.len()
                    );
                    written.extend_from_slice(&output);
                    read += info.input_consumed;

                    if read >= buf.len() {
                        break;
                    }
                }
                Err(err) => {
                    panic!("Failed to convert: {err:?}");
                }
            }
        }
        written.into()
    }
}

impl AudioEncoder for Mp3Encoder {
    fn encode(&mut self, decoded: AudioBuffer<f32>) -> Result<Bytes, AudioOutputError> {
        log::debug!("Mp3Encoder encode {} frames", decoded.frames());
        let buf = {
            let s = &mut AudioBuffer::<i16>::new(
                decoded.capacity() as Duration,
                decoded.spec().to_owned(),
            );
            decoded.convert(s);
            let decoded = AudioBufferRef::S16(std::borrow::Cow::Borrowed(s));
            let mut binding = self.resampler.write().unwrap();

            if let Some(resampler) = binding.as_mut() {
                log::debug!("Resampling");
                let mut buf = decoded.make_equivalent();
                decoded.convert(&mut buf);

                resampler
                    .resample(buf)
                    .ok_or(AudioOutputError::StreamEnd)?
                    .to_vec()
            } else {
                log::debug!("Not resampling");
                let n_channels = s.spec().channels.count();
                let n_samples = s.frames() * n_channels;
                let mut buf = vec![0_i16; n_samples];

                // Interleave the source buffer channels into the sample buffer.
                for ch in 0..n_channels {
                    let ch_slice = s.chan(ch);

                    for (dst, s) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
                        *dst = (*s).into_sample();
                    }
                }

                buf
            }
        };

        Ok(self.encode_output(&buf))
    }
}

impl AudioOutput for Mp3Encoder {
    fn write(&mut self, decoded: AudioBuffer<f32>) -> Result<usize, AudioOutputError> {
        if self.writer.is_none() {
            return Ok(0);
        }

        let bytes = self.encode(decoded)?;

        if let Some(writer) = self.writer.as_mut() {
            let mut count = 0;
            loop {
                count += writer.write(&bytes[count..]).unwrap();
                if count >= bytes.len() {
                    break;
                }
            }
        }

        Ok(bytes.len())
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        Ok(())
    }
}

pub fn encode_mp3_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_mp3_spawn(path, writer);

    stream
}

pub fn encode_mp3_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_mp3(path, writer) })
}

pub fn encode_mp3<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            let mut encoder = Mp3Encoder::with_writer(writer.clone());
            encoder.open(spec, duration);
            Ok(Box::new(encoder))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to mp3: {err:?}");
    }
}
