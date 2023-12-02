use std::cell::RefCell;
use std::fs::File;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use crate::output::{AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::resampler::Resampler;
use crate::{play_file_path_str, Progress};

use bytes::Bytes;
use futures::Stream;
use lazy_static::lazy_static;
use log::{debug, info};
use moosicbox_converter::opus::encoder_opus;
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::audio::*;
use symphonia::core::conv::ReversibleSample;
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

const STEREO_20MS: usize = 48000 * 2 * 20 / 1000;

pub struct OpusEncoder<T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    buf: [f32; STEREO_20MS],
    pos: usize,
    time: usize,
    bytes_read: usize,
    resampler: RefCell<Option<Resampler<T>>>,
    senders: RefCell<Vec<UnboundedSender<Bytes>>>,
    on_bytes: RefCell<Option<W>>,
    encoder: RefCell<opus::Encoder>,
}

impl<T, W> OpusEncoder<T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    pub fn new(writer: W) -> Self {
        Self {
            buf: [0.0; STEREO_20MS],
            pos: 0,
            time: 0,
            bytes_read: 0,
            resampler: RefCell::new(None),
            senders: RefCell::new(vec![]),
            on_bytes: RefCell::new(Some(writer)),
            encoder: RefCell::new(encoder_opus().unwrap()),
        }
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        self.resampler
            .borrow_mut()
            .replace(Resampler::<T>::new(spec, 48000_usize, duration));
    }

    pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Self, AudioOutputError> {
        if spec.rate != 48000 {
            info!("Will resample {} Hz to {} Hz", spec.rate, 48000);
        }
        Ok(Self {
            buf: [0.0; STEREO_20MS],
            pos: 0,
            time: 0,
            bytes_read: 0,
            resampler: RefCell::new(Some(Resampler::<T>::new(spec, 48000_usize, duration))),
            senders: RefCell::new(vec![]),
            on_bytes: RefCell::new(None),
            encoder: RefCell::new(encoder_opus().unwrap()),
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

    fn write_output(&self, buf_size: usize) -> usize {
        let mut read = 0;
        let mut written = 0;
        let mut output_buf = vec![0_u8; buf_size];
        //loop {
        //match moosicbox_converter::encode_audiopus(&self.buf[read..read + buf_size]) {
        match moosicbox_converter::opus::encode_opus_float(
            &mut self.encoder.borrow_mut(),
            &self.buf[read..read + buf_size],
            &mut output_buf,
        ) {
            //Ok((_, output_buf)) => {
            Ok(info) => {
                // let len = output_buf.len();
                // let bytes = Bytes::from(output_buf);
                let len = info.output_size;
                let bytes = Bytes::from(output_buf[written..written + info.output_size].to_vec());
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
                read += buf_size;
                written += len;
                if self.time % 1000 == 0 {
                    debug!(
                        "Info: read={} written={} input_consumed={} output_size={} len={}",
                        read, written, buf_size, len, self.bytes_read
                    );
                }

                // if read >= self.buf.len() {
                //     break;
                // }
            }
            Err(err) => {
                panic!("Failed to convert: {err:?}");
            } //}
        }
        written
    }
}

/*impl Stream for OpusEncoder<i16> {
    type Item = Result<Bytes, Box<dyn std::error::Error>>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.receiver.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(bytes))),
            Poll::Pending => {
                debug!("Pending...");
                Poll::Pending
            }
            Poll::Ready(None) => {
                debug!("Finished");
                Poll::Ready(None)
            }
        }
    }
}*/

impl<T, W> AudioOutput for OpusEncoder<T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        let mut binding = self.resampler.borrow_mut();
        let decoded = binding
            .as_mut()
            .unwrap()
            .resample(decoded)
            .ok_or(AudioOutputError::StreamEnd)?;

        let n_samples = decoded.len();

        for sample in decoded.iter() {
            if self.pos == STEREO_20MS {
                self.time += 20;
                self.bytes_read += self.write_output(STEREO_20MS);
                self.pos = 0;
                if self.time % 1000 == 0 {
                    debug!("time: {}", self.time / 1000);
                }
            }
            self.buf[self.pos] = (*sample).into_sample();
            self.pos += 1;
        }

        if self.pos == STEREO_20MS {
            self.time += 20;
            self.bytes_read += self.write_output(STEREO_20MS);
            self.pos = 0;
            if self.time % 1000 == 0 {
                debug!("time: {}", self.time / 1000);
            }
        }

        Ok(n_samples)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        debug!("Flushing");
        let mut binding = self.resampler.borrow_mut();
        let decoded = binding
            .as_mut()
            .unwrap()
            .flush()
            .ok_or(AudioOutputError::StreamEnd)?;

        for sample in decoded.iter() {
            if self.pos == STEREO_20MS {
                self.time += 20;
                self.bytes_read += self.write_output(STEREO_20MS);
                self.pos = 0;
                if self.time % 1000 == 0 {
                    debug!("time: {}", self.time / 1000);
                }
            }
            self.buf[self.pos] = (*sample).into_sample();
            self.pos += 1;
        }

        if self.pos == STEREO_20MS {
            self.time += 20;
            self.bytes_read += self.write_output(STEREO_20MS);
            self.pos = 0;
            if self.time % 1000 == 0 {
                debug!("time: {}", self.time / 1000);
            }
        }

        /*if self.pos >= STEREO_MIN {
            self.bytes_read += self.write_output(STEREO_MIN);
        }*/
        let mut binding = self.on_bytes.borrow_mut();
        if let Some(on_bytes) = binding.as_mut() {
            on_bytes.flush().unwrap();
        }
        if true {
            return Ok(());
        }

        // let path = match std::env::consts::OS {
        //     "windows" => "C:\\Users\\Brade\\test-win.opus",
        //     _ => "/mnt/c/Users/Brade/test-lin.opus",
        // };

        // let _ = std::fs::remove_file(path);
        // let file = std::fs::OpenOptions::new()
        //     .create(true) // To create a new file
        //     .write(true)
        //     .open(path)
        //     .unwrap();

        // let _ = std::io::Write::write_all(&mut file, &self.output.borrow());
        // moosicbox_converter::write_ogg(file, &self.output.borrow());

        let read = std::fs::OpenOptions::new()
            .read(true)
            .open("/mnt/c/Users/Brade/test-lin.opus")
            .unwrap();

        let write = std::fs::OpenOptions::new()
            .create(true) // To create a new file
            .write(true)
            .open("/mnt/c/Users/Brade/crayons3.opus")
            .unwrap();

        // let _ = std::io::Write::write_all(&mut file, &self.output.borrow());
        moosicbox_converter::opus::read_write_ogg(read, write);

        Ok(())
    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    Ok(Box::new(OpusEncoder::<i16, File>::try_open(
        spec, duration,
    )?))
}

pub fn encode_opus_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = (&writer).stream();

    encode_opus(path, writer);

    stream
}

pub fn encode_opus<T: std::io::Write + Send + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move {
        let mut audio_output_handler = AudioOutputHandler::new(Box::new(move |spec, duration| {
            let mut encoder: OpusEncoder<i16, T> = OpusEncoder::new(writer.clone());
            encoder.open(spec, duration);
            Ok(Box::new(encoder))
        }));

        let progress = Arc::new(RwLock::new(Progress { position: 0.0 }));
        let abort = Arc::new(AtomicBool::new(false));

        if let Err(err) = play_file_path_str(
            &path,
            &mut audio_output_handler,
            true,
            true,
            None,
            None,
            progress.clone(),
            abort.clone(),
        ) {
            log::error!("Failed to encode to opus: {err:?}");
        }
    })
}
