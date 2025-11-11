//! Opus packet framing and parsing according to RFC 6716.
//!
//! This module implements the frame parsing logic specified in RFC 6716 Section 3.2,
//! which handles the extraction of individual frames from Opus packets. Opus packets
//! can contain 1 to 48 frames encoded in various ways (constant bitrate, variable bitrate,
//! with or without padding).
//!
//! The primary entry point is the `parse_frames()` function, which takes a complete
//! Opus packet and returns slices to each individual frame payload.

use crate::{Error, Result, Toc};

fn decode_frame_length(data: &[u8]) -> Result<(usize, usize)> {
    if data.is_empty() {
        return Err(Error::InvalidPacket("Empty frame length data".into()));
    }

    let first = data[0];

    match first {
        0 => Ok((0, 1)),
        1..=251 => Ok((first as usize, 1)),
        _ => {
            if data.len() < 2 {
                return Err(Error::InvalidPacket("Missing second length byte".into()));
            }
            let second = data[1];
            let length = (second as usize * 4) + (first as usize);
            Ok((length, 2))
        }
    }
}

fn parse_code0(packet: &[u8]) -> Result<Vec<&[u8]>> {
    if packet.is_empty() {
        return Err(Error::InvalidPacket("Code 0 packet too short".into()));
    }
    Ok(vec![&packet[1..]])
}

fn parse_code1(packet: &[u8]) -> Result<Vec<&[u8]>> {
    let payload_len = packet.len() - 1;

    if !payload_len.is_multiple_of(2) {
        return Err(Error::InvalidPacket("Code 1 payload must be even".into()));
    }

    let frame_len = payload_len / 2;
    Ok(vec![&packet[1..=frame_len], &packet[1 + frame_len..]])
}

fn parse_code2(packet: &[u8]) -> Result<Vec<&[u8]>> {
    if packet.len() < 2 {
        return Err(Error::InvalidPacket("Code 2 too short".into()));
    }

    let (len1, len_bytes) = decode_frame_length(&packet[1..])?;

    let offset = 1 + len_bytes;
    if offset + len1 > packet.len() {
        return Err(Error::InvalidPacket("Frame 1 too large for packet".into()));
    }

    Ok(vec![
        &packet[offset..offset + len1],
        &packet[offset + len1..],
    ])
}

struct FrameCountByte {
    vbr: bool,
    padding: bool,
    count: u8,
}

impl FrameCountByte {
    fn parse(byte: u8) -> Result<Self> {
        let count = byte & 0x3F;

        if count == 0 {
            return Err(Error::InvalidPacket("Frame count must be ≥1".into()));
        }

        Ok(Self {
            vbr: (byte & 0x80) != 0,
            padding: (byte & 0x40) != 0,
            count,
        })
    }
}

fn decode_padding_length(data: &[u8], packet_len: usize) -> Result<(usize, usize)> {
    let mut len_indicator_bytes = 0;
    let mut padding_data_bytes = 0_usize;

    loop {
        if len_indicator_bytes >= data.len() {
            return Err(Error::InvalidPacket("Incomplete padding".into()));
        }

        let byte = data[len_indicator_bytes];
        len_indicator_bytes += 1;

        if byte == 255 {
            padding_data_bytes += 254;
        } else {
            padding_data_bytes += byte as usize;
            break;
        }
    }

    let total_padding_overhead = len_indicator_bytes + padding_data_bytes;
    if total_padding_overhead > packet_len - 2 {
        return Err(Error::InvalidPacket("Padding exceeds packet size".into()));
    }

    Ok((len_indicator_bytes, padding_data_bytes))
}

fn parse_code3_cbr(
    packet: &[u8],
    offset: usize,
    count: u8,
    padding_data_bytes: usize,
) -> Result<Vec<&[u8]>> {
    let available_for_frames = packet.len() - offset - padding_data_bytes;

    if !available_for_frames.is_multiple_of(count as usize) {
        return Err(Error::InvalidPacket(
            "CBR remainder not divisible by frame count".into(),
        ));
    }

    let frame_len = available_for_frames / (count as usize);
    let mut frames = Vec::with_capacity(count as usize);

    for i in 0..count {
        let start = offset + (i as usize * frame_len);
        let end = start + frame_len;
        frames.push(&packet[start..end]);
    }

    Ok(frames)
}

fn parse_code3_vbr(
    packet: &[u8],
    mut offset: usize,
    count: u8,
    padding_data_bytes: usize,
) -> Result<Vec<&[u8]>> {
    let mut frames = Vec::with_capacity(count as usize);
    let packet_end = packet.len() - padding_data_bytes;

    for _ in 0..(count - 1) {
        let (len, len_bytes) = decode_frame_length(&packet[offset..])?;
        offset += len_bytes;

        if offset + len > packet_end {
            return Err(Error::InvalidPacket("VBR frame exceeds packet".into()));
        }

        frames.push(&packet[offset..offset + len]);
        offset += len;
    }

    if offset > packet_end {
        return Err(Error::InvalidPacket(
            "VBR packet too short for last frame".into(),
        ));
    }

    frames.push(&packet[offset..packet_end]);

    Ok(frames)
}

fn parse_code3(packet: &[u8]) -> Result<Vec<&[u8]>> {
    if packet.len() < 2 {
        return Err(Error::InvalidPacket("Code 3 needs ≥2 bytes".into()));
    }

    let toc = Toc::parse(packet[0]);
    let fc_byte = FrameCountByte::parse(packet[1])?;

    let frame_duration_tenths = toc.frame_duration_tenths_ms();
    let total_duration_tenths = u32::from(fc_byte.count) * u32::from(frame_duration_tenths);

    if total_duration_tenths > 1200 {
        #[allow(clippy::cast_precision_loss)]
        let duration_ms = total_duration_tenths as f32 / 10.0;
        return Err(Error::InvalidPacket(format!(
            "Packet duration {:.1}ms exceeds 120ms limit (R5): {} frames",
            duration_ms, fc_byte.count
        )));
    }

    let mut offset = 2;

    let (len_indicator_bytes, padding_data_bytes) = if fc_byte.padding {
        decode_padding_length(&packet[offset..], packet.len())?
    } else {
        (0, 0)
    };

    offset += len_indicator_bytes;

    if fc_byte.vbr {
        parse_code3_vbr(packet, offset, fc_byte.count, padding_data_bytes)
    } else {
        parse_code3_cbr(packet, offset, fc_byte.count, padding_data_bytes)
    }
}

/// Parses an Opus packet into individual frames according to RFC 6716
///
/// # Errors
///
/// * Returns error if packet violates RFC 6716 requirements (R1-R7)
pub fn parse_frames(packet: &[u8]) -> Result<Vec<&[u8]>> {
    if packet.is_empty() {
        return Err(Error::InvalidPacket("Packet must be ≥1 byte".into()));
    }

    let toc = Toc::parse(packet[0]);

    match toc.frame_count_code() {
        0 => parse_code0(packet),
        1 => parse_code1(packet),
        2 => parse_code2(packet),
        3 => parse_code3(packet),
        _ => unreachable!("frame_count_code is 2 bits, max value 3"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code0_single_frame() {
        let packet = &[0b0000_0000, 0x01, 0x02, 0x03];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_code1_two_equal_frames() {
        let packet = &[0b0000_0001, 0x01, 0x02, 0x03, 0x04];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], &[0x01, 0x02]);
        assert_eq!(frames[1], &[0x03, 0x04]);
    }

    #[test]
    fn test_code1_odd_payload_fails() {
        let packet = &[0b0000_0001, 0x01, 0x02, 0x03];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_code2_two_variable_frames() {
        let packet = &[0b0000_0010, 2, 0x01, 0x02, 0x03, 0x04, 0x05];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], &[0x01, 0x02]);
        assert_eq!(frames[1], &[0x03, 0x04, 0x05]);
    }

    #[test]
    fn test_code2_frame1_too_large() {
        let packet = &[0b0000_0010, 10, 0x01, 0x02];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_code3_cbr_three_frames() {
        let packet = &[0b0000_0011, 0b0000_0011, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], &[0x01, 0x02]);
        assert_eq!(frames[1], &[0x03, 0x04]);
        assert_eq!(frames[2], &[0x05, 0x06]);
    }

    #[test]
    fn test_code3_cbr_non_divisible_fails() {
        let packet = &[0b0000_0011, 0b0000_0011, 0x01, 0x02, 0x03, 0x04, 0x05];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_code3_vbr_three_frames() {
        let packet = &[
            0b0000_0011,
            0b1000_0011,
            2,
            0x01,
            0x02,
            3,
            0x03,
            0x04,
            0x05,
            0x06,
            0x07,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], &[0x01, 0x02]);
        assert_eq!(frames[1], &[0x03, 0x04, 0x05]);
        assert_eq!(frames[2], &[0x06, 0x07]);
    }

    #[test]
    fn test_decode_frame_length_direct() {
        let (len, bytes) = decode_frame_length(&[100]).unwrap();
        assert_eq!(len, 100);
        assert_eq!(bytes, 1);
    }

    #[test]
    fn test_decode_frame_length_two_byte() {
        let (len, bytes) = decode_frame_length(&[252, 10]).unwrap();
        assert_eq!(len, 10 * 4 + 252);
        assert_eq!(bytes, 2);
    }

    #[test]
    fn test_decode_frame_length_max() {
        let (len, bytes) = decode_frame_length(&[255, 255]).unwrap();
        assert_eq!(len, 255 * 4 + 255);
        assert_eq!(bytes, 2);
    }

    #[test]
    fn test_decode_frame_length_dtx() {
        let (len, bytes) = decode_frame_length(&[0]).unwrap();
        assert_eq!(len, 0);
        assert_eq!(bytes, 1);
    }

    #[test]
    fn test_padding_single_byte() {
        let packet = &[
            0b0000_0011,
            0b0100_0001,
            5,
            0x01,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], &[0x01]);
    }

    #[test]
    fn test_padding_chain() {
        let mut packet = vec![0b0000_0011, 0b0100_0001, 255, 255, 2];
        packet.push(0x01);
        let padding_len = 254 + 254 + 2;
        packet.extend(std::iter::repeat_n(0x00, padding_len));

        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], &[0x01]);
    }

    #[test]
    fn test_empty_packet_fails() {
        assert!(parse_frames(&[]).is_err());
    }

    #[test]
    fn test_frame_count_zero_fails() {
        let packet = &[0b0000_0011, 0b0000_0000];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_code1_frame_content_validation() {
        let packet = &[0b0000_0001, 0xAA, 0xBB, 0xCC, 0xDD];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], &[0xAA, 0xBB]);
        assert_eq!(frames[1], &[0xCC, 0xDD]);
    }

    #[test]
    fn test_code3_cbr_with_padding_content() {
        let packet = &[
            0b0000_0011,
            0b0100_0010,
            3,
            0xAA,
            0xBB,
            0xCC,
            0xDD,
            0x00,
            0x00,
            0x00,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], &[0xAA, 0xBB]);
        assert_eq!(frames[1], &[0xCC, 0xDD]);
    }

    #[test]
    fn test_code3_vbr_with_padding_content() {
        let packet = &[
            0b0000_0011,
            0b1100_0011,
            2,
            2,
            0xAA,
            0xBB,
            3,
            0xCC,
            0xDD,
            0xEE,
            0xFF,
            0x00,
            0x00,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], &[0xAA, 0xBB]);
        assert_eq!(frames[1], &[0xCC, 0xDD, 0xEE]);
        assert_eq!(frames[2], &[0xFF]);
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_2_5ms() {
        let mut packet = vec![(16 << 3) | 0b011, 0b0011_0000];
        packet.extend(vec![0x01; 96]);
        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 48);
    }

    #[test]
    fn test_r5_exceeds_120ms_2_5ms() {
        let packet = &[(16 << 3) | 0b011, 0b0011_0001, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_5ms() {
        let mut packet = vec![(17 << 3) | 0b011, 0b0001_1000];
        packet.extend(vec![0x01; 96]);
        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 24);
    }

    #[test]
    fn test_r5_exceeds_120ms_5ms() {
        let packet = &[(17 << 3) | 0b011, 0b0001_1001, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_10ms() {
        let mut packet = vec![0b0000_0011, 0b0000_1100];
        packet.extend(vec![0x01; 48]);
        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 12);
    }

    #[test]
    fn test_r5_exceeds_120ms_10ms() {
        let packet = &[0b0000_0011, 0b0000_1101, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_20ms() {
        let packet = &[
            (1 << 3) | 0b011,
            0b0000_0110,
            0x01,
            0x01,
            0x01,
            0x01,
            0x01,
            0x01,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 6);
    }

    #[test]
    fn test_r5_exceeds_120ms_20ms() {
        let packet = &[(1 << 3) | 0b011, 0b0000_0111, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_40ms() {
        let packet = &[(2 << 3) | 0b011, 0b0000_0011, 0x01, 0x01, 0x01];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn test_r5_exceeds_120ms_40ms() {
        let packet = &[(2 << 3) | 0b011, 0b0000_0100, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_valid_at_120ms_limit_60ms() {
        let packet = &[(3 << 3) | 0b011, 0b0000_0010, 0x01, 0x01];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn test_r5_exceeds_120ms_60ms() {
        let packet = &[(3 << 3) | 0b011, 0b0000_0011, 0x01, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_2_5ms_boundary_47_frames_valid() {
        let mut packet = vec![(16 << 3) | 0b011, 0b0010_1111];
        packet.extend(vec![0x01; 94]);
        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 47);
    }

    #[test]
    fn test_r5_2_5ms_boundary_48_frames_valid() {
        let mut packet = vec![(16 << 3) | 0b011, 0b0011_0000];
        packet.extend(vec![0x01; 96]);
        let frames = parse_frames(&packet).unwrap();
        assert_eq!(frames.len(), 48);
    }

    #[test]
    fn test_r5_2_5ms_boundary_49_frames_invalid() {
        let packet = &[(16 << 3) | 0b011, 0b0011_0001, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }

    #[test]
    fn test_r5_2_5ms_boundary_50_frames_invalid() {
        let packet = &[(16 << 3) | 0b011, 0b0011_0010, 0x01, 0x01];
        assert!(parse_frames(packet).is_err());
    }
}
