#![allow(clippy::module_name_repetitions)]

use std::fs::File;

use ogg::{PacketReader, PacketWriteEndInfo, PacketWriter};
use thiserror::Error;

use crate::EncodeInfo;

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Encoder error")]
    AudiopusEncoder(#[from] audiopus::Error),
    #[error("Encoder error")]
    OpusEncoder(::opus::Error),
}

impl From<::opus::Error> for EncoderError {
    fn from(value: ::opus::Error) -> Self {
        Self::OpusEncoder(value)
    }
}

/// # Panics
///
/// * If the samples len fails to convert to u32 type
///
/// # Errors
///
/// * If the encoder fails to encode the samples
pub fn encode_audiopus(samples: &[f32]) -> Result<(u32, Vec<u8>), EncoderError> {
    use audiopus::{
        Application, Bitrate, Channels, Error as OpusError, ErrorCode as OpusErrorCode, SampleRate,
        coder::Encoder,
    };
    let sample_rate = SampleRate::Hz48000;
    let mut encoder = Encoder::new(sample_rate, Channels::Stereo, Application::Audio)?;
    encoder.set_bitrate(Bitrate::Max)?; //BitsPerSecond(24000))?;

    #[allow(clippy::cast_sign_loss)]
    let frame_size = (sample_rate as i32 / 1000 * 2 * 20) as usize;

    let mut output = vec![0u8; samples.len().max(256)];
    let mut samples_i = 0;
    let mut output_i = 0;
    let mut end_buffer = vec![0f32; frame_size];

    // Store number of samples.
    {
        let samples: u32 = samples.len().try_into().unwrap();
        let bytes = samples.to_be_bytes();
        output[..4].clone_from_slice(&bytes[..4]);
        output_i += 4;
    }

    while samples_i < samples.len() {
        match encoder.encode_float(
            if samples_i + frame_size < samples.len() {
                &samples[samples_i..(samples_i + frame_size)]
            } else {
                end_buffer[..(samples.len() - samples_i)].clone_from_slice(
                    &samples[samples_i..((samples.len() - samples_i) + samples_i)],
                );

                &end_buffer
            },
            &mut output[output_i + 2..],
        ) {
            Ok(pkt_len) => {
                samples_i += frame_size;
                let bytes = u16::try_from(pkt_len).unwrap().to_be_bytes();
                output[output_i] = bytes[0];
                output[output_i + 1] = bytes[1];
                output_i += pkt_len + 2;
            }
            Err(OpusError::Opus(OpusErrorCode::BufferTooSmall)) => {
                log::error!(
                    "Needed to increase buffer size, opus is compressing less well than expected."
                );
                output.resize(output.len() * 2, 0u8);
            }
            Err(e) => {
                return Err(EncoderError::AudiopusEncoder(e));
            }
        }
    }

    output.truncate(output_i);

    #[allow(clippy::cast_sign_loss)]
    Ok((sample_rate as i32 as u32, output))
}

/// # Errors
///
/// * If the encoder fails to initialize
pub fn encoder_opus() -> Result<::opus::Encoder, EncoderError> {
    let encoder =
        ::opus::Encoder::new(48000, ::opus::Channels::Stereo, ::opus::Application::Audio)?;

    Ok(encoder)
}

/// # Errors
///
/// * If the encoder fails to encode the samples
pub fn encode_opus_float(
    encoder: &mut ::opus::Encoder,
    input: &[f32],
    output: &mut [u8],
) -> Result<EncodeInfo, EncoderError> {
    let len = encoder.encode_float(input, output)?;

    Ok(EncodeInfo {
        output_size: len,
        input_consumed: input.len(),
    })
}

/// # Panics
///
/// * If the packet reader fails to read the next packet
pub fn read_write_ogg(mut read: std::fs::File, mut write: std::fs::File) {
    let mut pck_rdr = PacketReader::new(&mut read);

    // This call doesn't discard anything as nothing has
    // been stored yet, but it does set bits that
    // make reading logic a bit more tolerant towards
    // errors.
    pck_rdr.delete_unread_packets();

    let mut pck_wtr = PacketWriter::new(&mut write);

    loop {
        let r = pck_rdr.read_packet().unwrap();
        match r {
            Some(pck) => {
                let (inf_d, inf) = if pck.last_in_stream() {
                    ("end_stream", PacketWriteEndInfo::EndStream)
                } else if pck.last_in_page() {
                    ("end_page", PacketWriteEndInfo::EndPage)
                } else {
                    ("normal", PacketWriteEndInfo::NormalPacket)
                };
                let stream_serial = pck.stream_serial();
                let absgp_page = pck.absgp_page();
                log::debug!(
                    "stream_serial={} absgp_page={} len={} inf_d={inf_d}",
                    stream_serial,
                    absgp_page,
                    pck.data.len()
                );
                pck_wtr
                    .write_packet(pck.data, stream_serial, inf, absgp_page)
                    .unwrap();
            }
            // End of stream
            None => break,
        }
    }
}

pub fn write_ogg(file: std::fs::File, content: &[u8]) {
    let mut writer = PacketWriter::new(file);

    if let Err(err) = writer.write_packet(content, 0, PacketWriteEndInfo::EndStream, 0) {
        log::error!("Error: {err:?}");
    }
}

struct OpusPacket {
    content: Vec<u8>,
    packet_num: u64,
    page_num: u64,
    absgp: u64,
    info: PacketWriteEndInfo,
}

pub struct OpusWrite<'a> {
    packet_writer: PacketWriter<'a, File>,
    serial: u32,
    absgp: u64,
    packet_num: u64,
    page_num: u64,
    packet: Option<OpusPacket>,
}

// Construct Opus Stream Header Packet data
pub const OPUS_STREAM_IDENTIFICATION_HEADER: [u8; 19] = [
    // Opus magic signature ("OpusHead")
    b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd',
    // Version number (2 bytes, little endian)
    0x01, // Version 1
    // Number of channels (1 byte)
    0x02, // Stereo
    // Pre-skip (2 bytes, little endian)
    0x00, 0x00, // Zero pre-skip
    // Input sample rate (4 bytes, little endian)
    0x80, 0xBB, 0x00, 0x00, // 48000 Hz
    // 0x44, 0xAC, 0x00, 0x00, // 44100 Hz
    // 0xC0, 0x5D, 0x00, 0x00, // 24000 Hz
    // Output gain (2 bytes, little endian)
    0x00, 0x00, // Zero output gain
    // Channel mapping family (1 byte)
    0x00, // Channel mapping: "normal"
];

// Construct Opus Stream Header Packet data
pub const OPUS_STREAM_COMMENTS_HEADER: [u8; 23] = [
    // Opus magic signature ("OpusHead")
    b'O', b'p', b'u', b's', b'T', b'a', b'g', b's',
    // Vendor String Length (32 bits, unsigned, little endian)
    0x07, 0x00, 0x00, 0x00, // ENCODER len
    b'E', b'N', b'C', b'O', b'D', b'E', b'R',
    // User Comment List Length (32 bits, unsigned, little endian)
    0x00, 0x00, 0x00, 0x00, // Comment List len
];

impl OpusWrite<'_> {
    /// # Panics
    ///
    /// * If the output file fails to be opened
    #[must_use]
    pub fn new(path: &str) -> Self {
        let _ = std::fs::remove_file(path);
        let file = std::fs::OpenOptions::new()
            .create(true) // To create a new file
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();

        let packet_writer = PacketWriter::new(file);
        let absgp = 0;

        Self {
            packet_writer,
            serial: 2_873_470_314,
            absgp,
            packet_num: 0,
            page_num: 0,
            packet: None,
        }
    }
}

impl std::io::Write for OpusWrite<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let info = PacketWriteEndInfo::NormalPacket;

        let packet = OpusPacket {
            content: buf.to_vec(),
            info,
            absgp: self.absgp,
            packet_num: self.packet_num,
            page_num: self.page_num,
        };
        if let Some(packet) = self.packet.replace(packet) {
            let info_d = match packet.info {
                PacketWriteEndInfo::EndPage => "end_page",
                PacketWriteEndInfo::NormalPacket => "normal",
                PacketWriteEndInfo::EndStream => "end_stream",
            };
            log::debug!(
                "writing stream_serial={} absgp_page={}, len={}, info_d={} packet_num={} page_num={}",
                self.serial,
                packet.absgp,
                packet.content.len(),
                info_d,
                packet.packet_num,
                packet.page_num
            );
            self.packet_writer
                .write_packet(packet.content, self.serial, packet.info, packet.absgp)
                .unwrap();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(packet) = self.packet.take() {
            let info = PacketWriteEndInfo::EndStream;
            let info_d = match info {
                PacketWriteEndInfo::EndPage => "end_page",
                PacketWriteEndInfo::NormalPacket => "normal",
                PacketWriteEndInfo::EndStream => "end_stream",
            };
            log::debug!(
                "writing stream_serial={} absgp_page={}, len={}, info_d={} packet_num={} page_num={}",
                self.serial,
                packet.absgp,
                packet.content.len(),
                info_d,
                packet.packet_num,
                packet.page_num
            );
            self.packet_writer
                .write_packet(packet.content, self.serial, info, packet.absgp)?;
        }
        Ok(())
    }
}
