use std::cell::RefCell;
use std::fs::File;
use std::usize;

use crate::output::{AudioOutput, AudioOutputError, AudioOutputHandler};
use crate::play_file_path_str;
use crate::resampler::Resampler;

use bytes::Bytes;
use lazy_static::lazy_static;
use moosicbox_converter::opus::{
    encoder_opus, OPUS_STREAM_COMMENTS_HEADER, OPUS_STREAM_IDENTIFICATION_HEADER,
};
use moosicbox_stream_utils::{ByteStream, ByteWriter};
use ogg::{PacketWriteEndInfo, PacketWriter};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::audio::*;
use symphonia::core::conv::ReversibleSample;
use symphonia::core::units::Duration;
use tokio::sync::mpsc::UnboundedSender;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

const STEREO_20MS: usize = 48000 * 2 * 20 / 1000;

pub struct OpusEncoder<'a, T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    buf: [f32; STEREO_20MS],
    buf_len: usize,
    packet_writer: PacketWriter<'a, Vec<u8>>,
    last_write_pos: usize,
    serial: u32,
    absgp: u64,
    time: usize,
    bytes_read: usize,
    resampler: RefCell<Option<Resampler<T>>>,
    senders: RefCell<Vec<UnboundedSender<Bytes>>>,
    writer: RefCell<Option<W>>,
    encoder: RefCell<opus::Encoder>,
}

impl<T, W> OpusEncoder<'_, T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    pub fn new(writer: W) -> Self {
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
            resampler: RefCell::new(None),
            senders: RefCell::new(vec![]),
            writer: RefCell::new(Some(writer)),
            encoder: RefCell::new(encoder_opus().unwrap()),
        }
    }

    pub fn open(&mut self, spec: SignalSpec, duration: Duration) {
        self.resampler
            .borrow_mut()
            .replace(Resampler::<T>::new(spec, 48000_usize, duration));
    }

    pub fn try_open(
        writer: W,
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<Self, AudioOutputError> {
        let packet_writer = PacketWriter::new(Vec::new());
        if spec.rate != 48000 {
            log::info!("Will resample {} Hz to {} Hz", spec.rate, 48000);
        }
        Ok(Self {
            buf: [0.0; STEREO_20MS],
            buf_len: 0,
            packet_writer,
            last_write_pos: 0,
            serial: 0,
            absgp: 0,
            time: 0,
            bytes_read: 0,
            resampler: RefCell::new(Some(Resampler::<T>::new(spec, 48000_usize, duration))),
            senders: RefCell::new(vec![]),
            writer: RefCell::new(Some(writer)),
            encoder: RefCell::new(encoder_opus().unwrap()),
        })
    }

    fn write_output(&mut self, input: &[f32], buf_size: usize) -> usize {
        let mut read = 0;
        let mut written = 0;
        let mut output_buf = vec![0_u8; buf_size];

        loop {
            log::trace!(
                "Encoding bytes to OPUS input_len={} buf_size={}",
                input.len(),
                buf_size
            );
            let info = moosicbox_converter::opus::encode_opus_float(
                &mut self.encoder.borrow_mut(),
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
            written += info.output_size;

            {
                let mut senders = self.senders.borrow_mut();
                if !senders.is_empty() {
                    let bytes = Bytes::from(section.to_vec());
                    senders.retain(|sender| {
                        if sender.send(bytes.clone()).is_err() {
                            log::debug!("Receiver has disconnected. Removing sender.");
                            false
                        } else {
                            true
                        }
                    });
                }
            }

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
                    PacketWriteEndInfo::NormalPacket,
                    self.absgp,
                )
                .expect("Failed to write packet");

            self.absgp += (info.input_consumed / 2) as u64;

            self.write_new_packet_writer_contents();

            read += buf_size;
            if self.time % 1000 == 0 {
                log::debug!(
                    "Info: read={} written={} input_consumed={} output_size={} len={}",
                    read,
                    written,
                    buf_size,
                    len,
                    self.bytes_read
                );
            }

            if read >= input.len() {
                break;
            }
        }
        written
    }

    fn write_new_packet_writer_contents(&mut self) {
        let writer_contents = self.packet_writer.inner();

        if writer_contents.len() > self.last_write_pos {
            let written_section = &writer_contents[self.last_write_pos..];
            self.last_write_pos = writer_contents.len();

            log::trace!("OPUS packet writer data len={}", writer_contents.len());

            let mut binding = self.writer.borrow_mut();
            if let Some(on_bytes) = binding.as_mut() {
                on_bytes.write_all(written_section).unwrap();
            }
        }
    }

    fn write_samples(&mut self, decoded: Vec<T>) -> usize {
        let samples = [
            self.buf[..self.buf_len].to_vec(),
            decoded
                .into_iter()
                .map(|x| x.into_sample())
                .collect::<Vec<f32>>(),
        ]
        .concat();

        self.buf_len = 0;

        let mut written = 0;

        for chunk in samples.chunks(STEREO_20MS) {
            if chunk.len() < STEREO_20MS {
                self.buf_len = chunk.len();
                self.buf[..self.buf_len].copy_from_slice(chunk);
            } else {
                self.time += 20;
                let byte_count = self.write_output(chunk, STEREO_20MS);
                self.bytes_read += byte_count;
                written += byte_count;
                if self.time % 1000 == 0 {
                    log::debug!("time: {}", self.time / 1000);
                }
            }
        }

        written
    }

    fn get_samples(&self, decoded: AudioBufferRef<'_>) -> Result<Vec<T>, AudioOutputError> {
        Ok(self
            .resampler
            .borrow_mut()
            .as_mut()
            .unwrap()
            .resample(decoded)
            .ok_or(AudioOutputError::StreamEnd)?
            .to_vec())
    }

    fn flush_samples(&self) -> Result<Vec<T>, AudioOutputError> {
        Ok(self
            .resampler
            .borrow_mut()
            .as_mut()
            .unwrap()
            .flush()
            .ok_or(AudioOutputError::StreamEnd)?
            .to_vec())
    }
}

impl<T, W> AudioOutput for OpusEncoder<'_, T, W>
where
    T: ReversibleSample<f32> + 'static,
    W: std::io::Write,
{
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        Ok(self.write_samples(self.get_samples(decoded)?))
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        log::debug!("Flushing");
        self.write_samples(self.flush_samples()?);

        self.packet_writer
            .write_packet(
                vec![],
                self.serial,
                PacketWriteEndInfo::EndStream,
                self.absgp,
            )
            .expect("Failed to write packet end stream");

        self.write_new_packet_writer_contents();

        let mut binding = self.writer.borrow_mut();
        if let Some(on_bytes) = binding.as_mut() {
            on_bytes.flush()?;
        }

        Ok(())
    }
}

pub fn try_open(
    writer: File,
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    Ok(Box::new(OpusEncoder::<i16, File>::try_open(
        writer, spec, duration,
    )?))
}

pub fn encode_opus_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_opus_spawn(path, writer);

    stream
}

pub fn encode_opus_spawn<T: std::io::Write + Send + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    RT.spawn(async move { encode_opus(path, writer) })
}

pub fn encode_opus<T: std::io::Write + Send + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            let mut encoder: OpusEncoder<'_, i16, T> = OpusEncoder::new(writer.clone());
            encoder.open(spec, duration);
            Ok(Box::new(encoder))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to opus: {err:?}");
    }
}
