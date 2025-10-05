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

fn decode_padding_length(data: &[u8], packet_len: usize) -> Result<usize> {
    let mut offset = 0;
    let mut padding_bytes = 0_usize;

    loop {
        if offset >= data.len() {
            return Err(Error::InvalidPacket("Incomplete padding".into()));
        }

        let byte = data[offset];
        offset += 1;

        if byte == 255 {
            padding_bytes += 254;
        } else {
            padding_bytes += byte as usize;
            break;
        }
    }

    let total_padding_overhead = offset + padding_bytes;
    if total_padding_overhead > packet_len - 2 {
        return Err(Error::InvalidPacket("Padding exceeds packet size".into()));
    }

    Ok(total_padding_overhead)
}

fn parse_code3_cbr(
    packet: &[u8],
    offset: usize,
    count: u8,
    padding_overhead: usize,
) -> Result<Vec<&[u8]>> {
    let r = packet.len() - 2 - padding_overhead;

    if !r.is_multiple_of(count as usize) {
        return Err(Error::InvalidPacket(
            "CBR remainder not divisible by frame count".into(),
        ));
    }

    let frame_len = r / (count as usize);
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
    padding_overhead: usize,
) -> Result<Vec<&[u8]>> {
    let mut frames = Vec::with_capacity(count as usize);

    for _ in 0..(count - 1) {
        let (len, len_bytes) = decode_frame_length(&packet[offset..])?;
        offset += len_bytes;

        if offset + len > packet.len() - padding_overhead {
            return Err(Error::InvalidPacket("VBR frame exceeds packet".into()));
        }

        frames.push(&packet[offset..offset + len]);
        offset += len;
    }

    let end = packet.len() - padding_overhead;
    if offset > end {
        return Err(Error::InvalidPacket(
            "VBR packet too short for last frame".into(),
        ));
    }

    frames.push(&packet[offset..end]);

    Ok(frames)
}

fn parse_code3(packet: &[u8]) -> Result<Vec<&[u8]>> {
    if packet.len() < 2 {
        return Err(Error::InvalidPacket("Code 3 needs ≥2 bytes".into()));
    }

    let fc_byte = FrameCountByte::parse(packet[1])?;
    let mut offset = 2;

    let padding_overhead = if fc_byte.padding {
        let po = decode_padding_length(&packet[offset..], packet.len())?;
        offset += po;
        po
    } else {
        0
    };

    if fc_byte.vbr {
        parse_code3_vbr(packet, offset, fc_byte.count, padding_overhead)
    } else {
        parse_code3_cbr(packet, offset, fc_byte.count, padding_overhead)
    }
}

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
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x01,
        ];
        let frames = parse_frames(packet).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], &[0x01]);
    }

    #[test]
    fn test_padding_chain() {
        let mut packet = vec![0b0000_0011, 0b0100_0001, 255, 255, 2];
        let padding_len = 254 + 254 + 2;
        packet.extend(std::iter::repeat_n(0x00, padding_len));
        packet.push(0x01);

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
}
