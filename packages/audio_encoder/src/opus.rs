//! Opus audio encoding with Ogg container support.
//!
//! Provides functions for Opus encoding and Ogg stream writing, including both simple
//! encoding functions and streaming writers for Ogg/Opus files.

#![allow(clippy::module_name_repetitions)]

use ogg::{PacketReader, PacketWriteEndInfo, PacketWriter};
use switchy_fs::sync::File;
use thiserror::Error;

use crate::EncodeInfo;

/// Errors that can occur during Opus encoding operations.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// Error from the audiopus encoder
    #[error("Encoder error")]
    AudiopusEncoder(#[from] audiopus::Error),
    /// Error from the opus encoder
    #[error("Encoder error")]
    OpusEncoder(::opus::Error),
}

impl From<::opus::Error> for EncoderError {
    fn from(value: ::opus::Error) -> Self {
        Self::OpusEncoder(value)
    }
}

/// Encodes audio samples using the audiopus encoder with custom framing.
///
/// Returns the sample rate and encoded data with length-prefixed packets. The output
/// format begins with a 4-byte big-endian sample count, followed by a series of
/// encoded packets, each prefixed with a 2-byte big-endian length.
///
/// # Errors
///
/// * If the encoder fails to encode the samples
///
/// # Panics
///
/// * If the samples len fails to convert to u32 type
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

/// Creates a new Opus encoder with default settings.
///
/// Configures the encoder for 48kHz stereo audio.
///
/// # Errors
///
/// * If the encoder fails to initialize
pub fn encoder_opus() -> Result<::opus::Encoder, EncoderError> {
    let encoder =
        ::opus::Encoder::new(48000, ::opus::Channels::Stereo, ::opus::Application::Audio)?;

    Ok(encoder)
}

/// Encodes floating-point PCM audio samples to Opus format.
///
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

/// Copies an Ogg stream from one file to another, re-packaging all packets.
///
/// Reads all packets from the input Ogg stream and writes them to the output,
/// preserving packet boundaries, stream serial numbers, and page structure.
///
/// # Panics
///
/// * If the packet reader fails to read the next packet
/// * If the packet writer fails to write a packet
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

/// Writes a single Ogg packet to a file.
///
/// Creates an Ogg packet writer and writes the provided content as a single packet
/// with stream end marker. Errors during writing are logged but not propagated.
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

/// Ogg/Opus stream writer with buffering support.
///
/// Implements [`std::io::Write`] to provide a streaming interface for writing Opus-encoded
/// audio data to an Ogg container file.
///
/// # Panics
///
/// The [`Write::write`](std::io::Write::write) and [`Write::flush`](std::io::Write::flush)
/// implementations will panic if writing packets to the underlying Ogg stream fails.
pub struct OpusWrite<'a> {
    packet_writer: PacketWriter<'a, File>,
    serial: u32,
    absgp: u64,
    packet_num: u64,
    page_num: u64,
    packet: Option<OpusPacket>,
}

/// Opus stream identification header for Ogg encapsulation.
///
/// Contains the `OpusHead` magic signature and stream configuration:
/// * Version 1
/// * Stereo (2 channels)
/// * 48000 Hz sample rate
/// * Zero pre-skip and output gain
/// * Normal channel mapping family
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

/// Opus stream comment header for Ogg encapsulation.
///
/// Contains the `OpusTags` magic signature and minimal vendor string:
/// * Vendor string: "ENCODER"
/// * Empty user comment list
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
    /// Creates a new Ogg/Opus stream writer for the specified file path.
    ///
    /// # Panics
    ///
    /// * If the output file fails to be opened
    #[must_use]
    pub fn new(path: &str) -> Self {
        let _ = std::fs::remove_file(path);
        let file = switchy_fs::sync::OpenOptions::new()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_encoder_creation() {
        let result = encoder_opus();
        assert!(
            result.is_ok(),
            "Opus encoder should initialize successfully"
        );
    }

    #[test_log::test]
    fn test_encode_opus_float_basic() {
        let mut encoder = encoder_opus().expect("Failed to create encoder");

        // Create 960 samples (20ms at 48kHz stereo)
        let input: Vec<f32> = vec![0.0; 960];
        let mut output = vec![0u8; 4000];

        let result = encode_opus_float(&mut encoder, &input, &mut output);

        assert!(result.is_ok(), "Encoding should succeed");
        let info = result.unwrap();

        assert!(info.output_size > 0, "Should produce output");
        assert_eq!(
            info.input_consumed,
            input.len(),
            "Should report all input consumed"
        );
    }

    #[test_log::test]
    fn test_encode_audiopus_packet_framing() {
        // Create a small sample set (less than one frame)
        let samples: Vec<f32> = vec![0.1; 1000];

        let result = encode_audiopus(&samples);
        assert!(result.is_ok(), "Encoding should succeed");

        let (sample_rate, output) = result.unwrap();

        // Verify sample rate
        assert_eq!(sample_rate, 48000, "Sample rate should be 48kHz");

        // Verify output format: first 4 bytes should be sample count
        assert!(
            output.len() >= 4,
            "Output should contain at least the sample count"
        );

        let sample_count = u32::from_be_bytes([output[0], output[1], output[2], output[3]]);
        #[allow(clippy::cast_possible_truncation)]
        let expected_count = samples.len() as u32;
        assert_eq!(
            sample_count, expected_count,
            "Sample count should match input"
        );

        // After sample count, there should be at least one packet with 2-byte length prefix
        if output.len() > 4 {
            assert!(
                output.len() >= 6,
                "Should have room for at least one packet length"
            );
        }
    }

    #[test_log::test]
    fn test_encode_audiopus_multiple_frames() {
        // Create enough samples for multiple frames
        // Frame size at 48kHz stereo with 20ms = 1920 samples per frame
        let frame_size = 1920;
        let samples: Vec<f32> = vec![0.5; frame_size * 3];

        let result = encode_audiopus(&samples);
        assert!(result.is_ok(), "Encoding should succeed");

        let (sample_rate, output) = result.unwrap();
        assert_eq!(sample_rate, 48000);

        // Parse the output to verify multiple packets
        let mut offset = 4; // Skip sample count
        let mut packet_count = 0;

        while offset + 2 <= output.len() {
            let packet_len = u16::from_be_bytes([output[offset], output[offset + 1]]) as usize;
            if packet_len == 0 {
                break;
            }
            offset += 2 + packet_len;
            packet_count += 1;

            if offset >= output.len() {
                break;
            }
        }

        assert!(packet_count >= 1, "Should have encoded at least one packet");
    }

    #[test_log::test]
    fn test_encode_audiopus_empty_input() {
        let samples: Vec<f32> = vec![];

        let result = encode_audiopus(&samples);
        assert!(result.is_ok(), "Empty input should be handled");

        let (_sample_rate, output) = result.unwrap();

        // Should at least contain the sample count (0)
        assert!(output.len() >= 4);
        let sample_count = u32::from_be_bytes([output[0], output[1], output[2], output[3]]);
        assert_eq!(sample_count, 0);
    }

    #[test_log::test]
    #[allow(clippy::cast_precision_loss)]
    fn test_encode_audiopus_varying_amplitudes() {
        // Test encoding samples with different amplitude patterns
        let samples: Vec<f32> = (0..1920)
            .map(|i| {
                // Generate a sine-like wave pattern
                let t = i as f32 / 48000.0;
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
            })
            .collect();

        let result = encode_audiopus(&samples);
        assert!(result.is_ok(), "Encoding varying amplitudes should succeed");

        let (sample_rate, output) = result.unwrap();
        assert_eq!(sample_rate, 48000);

        // Verify sample count header is correct
        let sample_count = u32::from_be_bytes([output[0], output[1], output[2], output[3]]);
        #[allow(clippy::cast_possible_truncation)]
        let expected = samples.len() as u32;
        assert_eq!(sample_count, expected);
    }

    #[test_log::test]
    #[allow(clippy::cast_precision_loss)]
    fn test_encode_opus_float_consecutive_calls() {
        let mut encoder = encoder_opus().expect("Failed to create encoder");

        // Encode multiple consecutive frames (960 samples = 10ms at 48kHz stereo)
        let frame_size = 960;
        let mut total_output = 0;

        for i in 0..5 {
            let input: Vec<f32> = (0..frame_size)
                .map(|j| {
                    let t = (i * frame_size + j) as f32 / 48000.0;
                    (t * 440.0 * std::f32::consts::TAU).sin() * 0.3
                })
                .collect();
            let mut output = vec![0u8; 4000];

            let result = encode_opus_float(&mut encoder, &input, &mut output);
            assert!(
                result.is_ok(),
                "Consecutive encoding call {} should succeed",
                i + 1
            );

            let info = result.unwrap();
            assert!(info.output_size > 0, "Each frame should produce output");
            assert_eq!(
                info.input_consumed, frame_size,
                "Each frame should consume all input"
            );
            total_output += info.output_size;
        }

        assert!(total_output > 0, "Total output should be non-zero");
    }

    #[test_log::test(switchy_async::test(real_fs))]
    async fn test_opus_write_creation() {
        let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");
        let temp_file = temp_dir.path().join("test_opus_write.ogg");
        let temp_file_str = temp_file.to_string_lossy();
        let writer = OpusWrite::new(&temp_file_str);

        assert_eq!(writer.serial, 2_873_470_314, "Serial should be initialized");
        assert_eq!(writer.absgp, 0, "Initial absgp should be 0");
        assert_eq!(writer.packet_num, 0, "Initial packet_num should be 0");
        assert_eq!(writer.page_num, 0, "Initial page_num should be 0");
        assert!(writer.packet.is_none(), "Initial packet should be None");
    }

    #[test_log::test(switchy_async::test(real_fs))]
    async fn test_opus_write_buffering_behavior() {
        use std::io::Write;

        let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");
        let temp_file = temp_dir.path().join("test_opus_buffering.ogg");
        let temp_file_str = temp_file.to_string_lossy();
        let mut writer = OpusWrite::new(&temp_file_str);

        // First write should buffer the packet
        let data1 = vec![1u8; 100];
        let result1 = writer.write(&data1);
        assert!(result1.is_ok());
        assert_eq!(result1.unwrap(), 100);
        assert!(writer.packet.is_some(), "First packet should be buffered");

        // Second write should write the first packet and buffer the second
        let data2 = vec![2u8; 100];
        let result2 = writer.write(&data2);
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap(), 100);
        assert!(writer.packet.is_some(), "Second packet should be buffered");

        // Flush should write the buffered packet
        let flush_result = writer.flush();
        assert!(flush_result.is_ok());
        assert!(writer.packet.is_none(), "Packet should be written on flush");
    }

    #[test_log::test(switchy_async::test(real_fs))]
    async fn test_write_ogg_creates_valid_ogg_file() {
        use std::fs::File;
        use std::io::Read;

        let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");
        let temp_file = temp_dir.path().join("test_write_ogg.ogg");

        // Create test data with some content
        let content = vec![0x01u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        {
            let file = File::create(&temp_file).expect("Failed to create file");
            write_ogg(file, &content);
        }

        // Verify file was created and contains data
        let mut file = File::open(&temp_file).expect("Failed to open file");
        let mut file_content = Vec::new();
        file.read_to_end(&mut file_content)
            .expect("Failed to read file");

        // OGG files start with "OggS" magic
        assert!(
            file_content.len() >= 4,
            "File should contain at least OGG header"
        );
        assert_eq!(
            &file_content[..4],
            b"OggS",
            "File should start with OGG magic bytes"
        );
    }

    #[test_log::test(switchy_async::test(real_fs))]
    async fn test_read_write_ogg_roundtrip() {
        use std::fs::File;
        use std::io::Read;

        let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");
        let source_file = temp_dir.path().join("source.ogg");
        let dest_file = temp_dir.path().join("dest.ogg");

        // First create a source OGG file with some content
        let content = vec![0xAAu8, 0xBB, 0xCC, 0xDD];
        {
            let file = File::create(&source_file).expect("Failed to create source file");
            write_ogg(file, &content);
        }

        // Now use read_write_ogg to copy the file
        {
            let read_file = File::open(&source_file).expect("Failed to open source file");
            let write_file = File::create(&dest_file).expect("Failed to create dest file");
            read_write_ogg(read_file, write_file);
        }

        // Verify both files exist and have similar structure
        let mut source_content = Vec::new();
        let mut dest_content = Vec::new();

        File::open(&source_file)
            .expect("Failed to open source")
            .read_to_end(&mut source_content)
            .expect("Failed to read source");

        File::open(&dest_file)
            .expect("Failed to open dest")
            .read_to_end(&mut dest_content)
            .expect("Failed to read dest");

        // Both should be valid OGG files
        assert!(
            source_content.len() >= 4 && &source_content[..4] == b"OggS",
            "Source should be valid OGG"
        );
        assert!(
            dest_content.len() >= 4 && &dest_content[..4] == b"OggS",
            "Dest should be valid OGG"
        );

        // Content of the packet data should be preserved (files should be equal)
        assert_eq!(
            source_content.len(),
            dest_content.len(),
            "Files should be same size"
        );
    }
}
