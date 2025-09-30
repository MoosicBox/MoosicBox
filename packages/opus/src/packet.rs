use bytes::Bytes;
use log::debug;

use crate::{
    error::{Error, Result},
    frame::{OpusFrame, decode_frame_length},
    toc::TocByte,
};

/// Parsed Opus packet.
#[derive(Debug, Clone)]
pub struct OpusPacket {
    /// Table of contents byte
    pub toc: TocByte,
    /// Decoded frames
    pub frames: Vec<OpusFrame>,
    /// Optional padding
    pub padding: Bytes,
}

impl OpusPacket {
    /// Parse an Opus packet from bytes.
    ///
    /// # Errors
    ///
    /// * `PacketTooShort` - If the packet is empty or too short for the declared structure
    /// * `InvalidPacket` - If the packet structure is invalid according to RFC 6716
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(Error::PacketTooShort(0));
        }

        debug!("Parsing Opus packet, size: {} bytes", data.len());

        let toc = TocByte::parse(data[0])?;
        let (frames, padding_bytes) = match toc.frame_code() {
            0 => parse_code_0(&data[1..])?,
            1 => parse_code_1(&data[1..])?,
            2 => parse_code_2(&data[1..])?,
            3 => parse_code_3(&data[1..])?,
            _ => unreachable!(),
        };

        Ok(Self {
            toc,
            frames,
            padding: Bytes::from(padding_bytes),
        })
    }
}

/// Parse code 0 packet (single frame).
///
/// # Errors
///
/// This function validates according to RFC 6716 but currently always succeeds.
/// DTX (empty) frames are valid.
///
/// # Returns
///
/// Returns a tuple of (frames, `padding_bytes`). Code 0 never has padding.
#[allow(clippy::unnecessary_wraps)]
fn parse_code_0(data: &[u8]) -> Result<(Vec<OpusFrame>, Vec<u8>)> {
    // Code 0: Single frame - can be empty (DTX)
    Ok((
        vec![OpusFrame {
            data: data.to_vec(),
            is_dtx: data.is_empty(),
        }],
        Vec::new(), // No padding in code 0
    ))
}

/// Parse code 1 packet (two equal frames).
///
/// # Errors
///
/// * `PacketTooShort` - If data has less than 2 bytes (1 per frame minimum)
/// * `InvalidPacket` - If the data length is not divisible by 2
///
/// # Returns
///
/// Returns a tuple of (frames, `padding_bytes`). Code 1 never has padding.
fn parse_code_1(data: &[u8]) -> Result<(Vec<OpusFrame>, Vec<u8>)> {
    // Validate minimum length (at least 1 byte per frame)
    if data.len() < 2 {
        return Err(Error::PacketTooShort(data.len()));
    }

    // Validate even length (two equal frames)
    if !data.len().is_multiple_of(2) {
        return Err(Error::InvalidPacket);
    }

    let frame_size = data.len() / 2;
    Ok((
        vec![
            OpusFrame {
                data: data[..frame_size].to_vec(),
                is_dtx: false,
            },
            OpusFrame {
                data: data[frame_size..].to_vec(),
                is_dtx: false,
            },
        ],
        Vec::new(), // No padding in code 1
    ))
}

/// Parse code 2 packet (two variable frames).
///
/// # Errors
///
/// * `PacketTooShort` - If there isn't enough data for the frame length prefix or frame data
/// * `InvalidFrameLength` - If the frame length encoding is invalid
///
/// # Returns
///
/// Returns a tuple of (frames, `padding_bytes`). Code 2 never has padding.
fn parse_code_2(data: &[u8]) -> Result<(Vec<OpusFrame>, Vec<u8>)> {
    // Decode first frame length (also validates minimum packet size)
    let (len1, offset) = decode_frame_length(data)?;

    // Validate we have enough data for both frames
    if offset + len1 > data.len() {
        return Err(Error::PacketTooShort(data.len()));
    }

    Ok((
        vec![
            OpusFrame {
                data: data[offset..offset + len1].to_vec(),
                is_dtx: len1 == 0,
            },
            OpusFrame {
                data: data[offset + len1..].to_vec(),
                is_dtx: false,
            },
        ],
        Vec::new(), // No padding in code 2
    ))
}

/// Parse code 3 packet (multiple frames).
///
/// # Errors
///
/// * `PacketTooShort` - If the packet is empty or too short for frame count
/// * `InvalidPacket` - If frame count is invalid (0 or >48), or frame structure is invalid
/// * `InvalidFrameLength` - If frame length encoding is invalid in VBR mode
///
/// # Returns
///
/// Returns a tuple of (frames, `padding_bytes`). Padding is extracted if present.
fn parse_code_3(data: &[u8]) -> Result<(Vec<OpusFrame>, Vec<u8>)> {
    if data.is_empty() {
        return Err(Error::PacketTooShort(0));
    }

    // Parse header byte (RFC 6716 Section 3.2.5)
    let header = data[0];
    let frame_count = (header & 0x3F) as usize; // Bits 0-5: frame count
    let vbr = (header & 0x40) != 0; // Bit 6: VBR flag
    let has_padding = (header & 0x80) != 0; // Bit 7: padding flag

    // Validate frame count (1-48)
    if frame_count == 0 || frame_count > 48 {
        return Err(Error::InvalidPacket);
    }

    // Validate minimum packet size for frame count
    if data.len() < 1 + frame_count {
        return Err(Error::PacketTooShort(data.len()));
    }

    // Calculate padding length if present
    let padding_len = if has_padding {
        // Padding length is encoded at the end of the packet
        if data.len() < 2 {
            return Err(Error::PacketTooShort(data.len()));
        }

        // Find padding length by reading backwards
        let last_byte = data[data.len() - 1];
        let padding_length = if last_byte == 0 {
            // Zero means read another byte
            if data.len() < 3 {
                return Err(Error::PacketTooShort(data.len()));
            }
            data[data.len() - 2] as usize
        } else {
            last_byte as usize
        };

        // Padding includes the length bytes themselves
        if last_byte == 0 {
            padding_length + 2
        } else {
            padding_length + 1
        }
    } else {
        0
    };

    // Available data is everything except header and padding
    let available_data_len = data.len() - 1 - padding_len;

    // Extract padding bytes if present
    let padding_bytes = if padding_len > 0 {
        data[data.len() - padding_len..].to_vec()
    } else {
        Vec::new()
    };

    if vbr {
        // VBR mode: each frame (except last) has length prefix
        let mut frames = Vec::with_capacity(frame_count);
        let mut offset = 1; // Start after header byte
        let mut total_frame_data = 0;

        // Decode lengths for first M-1 frames
        let mut frame_lengths = Vec::with_capacity(frame_count);
        for _ in 0..frame_count - 1 {
            if offset >= data.len() - padding_len {
                return Err(Error::PacketTooShort(data.len()));
            }

            let (length, bytes_read) = decode_frame_length(&data[offset..])?;
            offset += bytes_read;
            total_frame_data += length;
            frame_lengths.push(length);
        }

        // Last frame gets remaining data
        if total_frame_data > available_data_len - (offset - 1) {
            return Err(Error::PacketTooShort(data.len()));
        }
        let last_frame_length = available_data_len - (offset - 1) - total_frame_data;
        frame_lengths.push(last_frame_length);

        // Now extract frame data
        for length in frame_lengths {
            if offset + length > data.len() - padding_len {
                return Err(Error::PacketTooShort(data.len()));
            }

            frames.push(OpusFrame {
                data: data[offset..offset + length].to_vec(),
                is_dtx: length == 0,
            });
            offset += length;
        }

        Ok((frames, padding_bytes))
    } else {
        // CBR mode: all frames equal size
        if !available_data_len.is_multiple_of(frame_count) {
            return Err(Error::InvalidPacket);
        }

        let frame_size = available_data_len / frame_count;
        let mut frames = Vec::with_capacity(frame_count);

        for i in 0..frame_count {
            let start = 1 + i * frame_size;
            let end = start + frame_size;

            if end > data.len() - padding_len {
                return Err(Error::PacketTooShort(data.len()));
            }

            frames.push(OpusFrame {
                data: data[start..end].to_vec(),
                is_dtx: false,
            });
        }

        Ok((frames, padding_bytes))
    }
}
