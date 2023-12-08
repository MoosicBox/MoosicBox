use std::cell::RefCell;
use std::fs::File;

use crate::output::{AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::resampler::Resampler;
use crate::{play_file_path_str, PlaybackHandle};

use bytes::Bytes;
use futures::Stream;
use lazy_static::lazy_static;
use log::debug;
use moosicbox_converter::mp3::encoder_mp3;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::audio::*;
use symphonia::core::conv::IntoSample;
use symphonia::core::units::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct Mp3Encoder<W>
where
    W: std::io::Write,
{
    resampler: RefCell<Option<Resampler<i16>>>,
    senders: RefCell<Vec<UnboundedSender<Bytes>>>,
    on_bytes: RefCell<Option<W>>,
    encoder: mp3lame_encoder::Encoder,
}

impl<W> Mp3Encoder<W>
where
    W: std::io::Write,
{
    pub fn new(writer: W) -> Self {
        Self {
            resampler: RefCell::new(None),
            senders: RefCell::new(vec![]),
            on_bytes: RefCell::new(Some(writer)),
            encoder: encoder_mp3().unwrap(),
        }
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        self.resampler
            .borrow_mut()
            .replace(Resampler::<i16>::new(spec, 48000_usize, duration));
    }

    pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Self, AudioOutputError> {
        Ok(Self {
            resampler: RefCell::new(Some(Resampler::<i16>::new(spec, 48000_usize, duration))),
            senders: RefCell::new(vec![]),
            on_bytes: RefCell::new(None),
            encoder: encoder_mp3().unwrap(),
        })
    }

    pub fn bytes_receiver(&mut self) -> UnboundedReceiver<Bytes> {
        let (sender, receiver) = unbounded_channel();
        self.senders.borrow_mut().push(sender);
        receiver
    }

    pub fn stream(&mut self) -> impl Stream<Item = Bytes> {
        let (sender, receiver) = unbounded_channel();
        self.senders.borrow_mut().push(sender);
        UnboundedReceiverStream::new(receiver)
    }

    fn write_output(&mut self, buf: &[i16]) -> usize {
        let mut written = 0;
        match moosicbox_converter::mp3::encode_mp3(&mut self.encoder, buf) {
            Ok((output_buf, info)) => {
                let len = info.output_size;
                let bytes = Bytes::from(output_buf);
                self.senders.borrow_mut().retain(|sender| {
                    if sender.send(bytes.clone()).is_err() {
                        debug!("Receiver has disconnected. Removing sender.");
                        false
                    } else {
                        true
                    }
                });
                let mut binding = self.on_bytes.borrow_mut();
                if let Some(on_bytes) = binding.as_mut() {
                    loop {
                        let count = on_bytes.write(&bytes).unwrap();
                        if count >= bytes.len() {
                            break;
                        }
                    }
                }
                written += len;
            }
            Err(err) => {
                panic!("Failed to convert: {err:?}");
            }
        }
        written
    }
}

impl<W> AudioOutput for Mp3Encoder<W>
where
    W: std::io::Write,
{
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        let s =
            &mut AudioBuffer::<i16>::new(decoded.capacity() as Duration, decoded.spec().to_owned());
        decoded.convert(s);

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
        self.write_output(buf);

        Ok(n_samples)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        debug!("Flushing");

        Ok(())
    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    Ok(Box::new(Mp3Encoder::<File>::try_open(spec, duration)?))
}

pub fn encode_mp3_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_mp3(path, writer);

    stream
}

pub fn encode_mp3<T: std::io::Write + Send + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move {
        let mut audio_output_handler = AudioOutputHandler::new(Box::new(move |spec, duration| {
            let mut encoder: Mp3Encoder<T> = Mp3Encoder::new(writer.clone());
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
            log::error!("Failed to encode to mp3: {err:?}");
        }
    })
}
