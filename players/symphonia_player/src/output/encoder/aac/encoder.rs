use std::cell::RefCell;

use crate::output::{AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::resampler::Resampler;
use crate::{play_file_path_str, PlaybackHandle};

use bytes::Bytes;
use lazy_static::lazy_static;
use log::debug;
use moosicbox_converter::aac::encoder_aac;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::AudioBufferRef;
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

pub struct AacEncoder<W>
where
    W: std::io::Write,
{
    resampler: RefCell<Option<Resampler<i16>>>,
    writer: RefCell<Option<W>>,
    encoder: RefCell<fdk_aac::enc::Encoder>,
}

impl<W> AacEncoder<W>
where
    W: std::io::Write,
{
    pub fn new(writer: W) -> Self {
        Self {
            resampler: RefCell::new(None),
            writer: RefCell::new(Some(writer)),
            encoder: RefCell::new(encoder_aac().unwrap()),
        }
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        if spec.rate != 48000 {
            self.resampler
                .borrow_mut()
                .replace(Resampler::<i16>::new(spec, 48000_usize, duration));
        } else {
            self.resampler.borrow_mut().take();
        }
    }

    fn write_output(&self, buf: &[i16]) -> usize {
        let mut read = 0;
        let mut written = 0;
        loop {
            let end = std::cmp::min(read + 1024, buf.len());
            let mut output = [0u8; 2048];
            match moosicbox_converter::aac::encode_aac(
                &mut self.encoder.borrow_mut(),
                &buf[read..end],
                &mut output,
            ) {
                Ok(info) => {
                    let len = info.output_size;
                    let bytes = Bytes::from(output[..info.output_size].to_vec());
                    let mut binding = self.writer.borrow_mut();
                    if let Some(writer) = binding.as_mut() {
                        loop {
                            let count = writer.write(&bytes).unwrap();
                            if count >= bytes.len() {
                                break;
                            }
                        }
                    }
                    read += info.input_consumed;
                    written += len;

                    if read >= buf.len() {
                        break;
                    }
                }
                Err(err) => {
                    panic!("Failed to convert: {err:?}");
                }
            }
        }
        written
    }
}

impl<W> AudioOutput for AacEncoder<W>
where
    W: std::io::Write,
{
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        let s =
            &mut AudioBuffer::<i16>::new(decoded.capacity() as Duration, decoded.spec().to_owned());
        decoded.convert(s);
        let decoded = AudioBufferRef::S16(std::borrow::Cow::Borrowed(s));
        let mut binding = self.resampler.borrow_mut();

        if let Some(resampler) = binding.as_mut() {
            let decoded = resampler
                .resample(decoded)
                .ok_or(AudioOutputError::StreamEnd)?;

            Ok(self.write_output(decoded))
        } else {
            let n_channels = s.spec().channels.count();
            let n_samples = s.frames() * n_channels;
            let buf = &mut vec![0_i16; n_samples];

            // Interleave the source buffer channels into the sample buffer.
            for ch in 0..n_channels {
                let ch_slice = s.chan(ch);

                for (dst, s) in buf[ch..].iter_mut().step_by(n_channels).zip(ch_slice) {
                    *dst = (*s).into_sample();
                }
            }

            Ok(self.write_output(buf))
        }
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        debug!("Flushing");

        Ok(())
    }
}

pub fn encode_aac_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_aac_spawn(path, writer);

    stream
}

pub fn encode_aac_spawn<T: std::io::Write + Send + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_aac(path, writer) })
}

pub fn encode_aac<T: std::io::Write + Send + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler = AudioOutputHandler::new();

    audio_output_handler.with_output(Box::new(move |spec, duration| {
        let mut encoder = AacEncoder::new(writer.clone());
        encoder.open(spec, duration);
        Ok(Box::new(encoder))
    }));

    let handle = PlaybackHandle::default();

    if let Err(err) = play_file_path_str(
        &path,
        &mut audio_output_handler,
        true,
        true,
        None,
        None,
        &handle,
    ) {
        log::error!("Failed to encode to aac: {err:?}");
    }
}
