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

    let header = data[0];
    let frame_count = (header & 0x3F) as usize;
    let vbr = (header & 0x40) != 0;
    let has_padding = (header & 0x80) != 0;

    if frame_count == 0 || frame_count > 48 {
        return Err(Error::InvalidPacket);
    }

    let mut offset = 1;

    let (padding_size_bytes, padding_data_bytes) = if has_padding {
        let mut padding_indicator_bytes = 0;
        let mut total_padding_data = 0;

        loop {
            if offset >= data.len() {
                return Err(Error::PacketTooShort(data.len()));
            }

            let padding_byte = data[offset];
            offset += 1;
            padding_indicator_bytes += 1;

            if padding_byte == 255 {
                total_padding_data += 254;
            } else {
                total_padding_data += padding_byte as usize;
                break;
            }
        }

        (padding_indicator_bytes, total_padding_data)
    } else {
        (0, 0)
    };

    let total_padding = padding_size_bytes + padding_data_bytes;

    if data.len() < 1 + total_padding {
        return Err(Error::PacketTooShort(data.len()));
    }

    let available_frame_data_len = data.len() - 1 - total_padding;

    let padding_bytes = if padding_data_bytes > 0 {
        if data.len() < padding_data_bytes {
            return Err(Error::PacketTooShort(data.len()));
        }
        data[data.len() - padding_data_bytes..].to_vec()
    } else {
        Vec::new()
    };

    if vbr {
        let mut frames = Vec::with_capacity(frame_count);
        let mut total_frame_data = 0;

        let mut frame_lengths = Vec::with_capacity(frame_count);
        for _ in 0..frame_count - 1 {
            if offset >= data.len() - padding_data_bytes {
                return Err(Error::PacketTooShort(data.len()));
            }

            let (length, bytes_read) = decode_frame_length(&data[offset..])?;
            offset += bytes_read;
            total_frame_data += length;
            frame_lengths.push(length);
        }

        if total_frame_data > available_frame_data_len - (offset - 1 - padding_size_bytes) {
            return Err(Error::PacketTooShort(data.len()));
        }
        let last_frame_length =
            available_frame_data_len - (offset - 1 - padding_size_bytes) - total_frame_data;
        frame_lengths.push(last_frame_length);

        for length in frame_lengths {
            if offset + length > data.len() - padding_data_bytes {
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
        if available_frame_data_len == 0 && frame_count > 0 {
            return Err(Error::PacketTooShort(data.len()));
        }

        if !available_frame_data_len.is_multiple_of(frame_count) {
            return Err(Error::InvalidPacket);
        }

        let frame_size = available_frame_data_len / frame_count;
        let mut frames = Vec::with_capacity(frame_count);

        for i in 0..frame_count {
            let start = offset + i * frame_size;
            let end = start + frame_size;

            if end > data.len() - padding_data_bytes {
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
