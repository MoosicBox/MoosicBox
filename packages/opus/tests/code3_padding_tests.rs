#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::packet::OpusPacket;
use pretty_assertions::assert_eq;

#[test_log::test]
fn test_code_3_cbr_with_simple_padding() {
    let packet = vec![
        0x03, 0x83, 5, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33, 0, 0, 0, 0, 0,
    ];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.frames[2].data, vec![0x11, 0x22, 0x33]);
    assert_eq!(result.padding.len(), 5);
}

#[test_log::test]
fn test_code_3_cbr_with_zero_padding() {
    let packet = vec![
        0x03, 0x83, 0, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33,
    ];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.frames[2].data, vec![0x11, 0x22, 0x33]);
    assert_eq!(result.padding.len(), 0);
}

#[test_log::test]
fn test_code_3_vbr_with_simple_padding() {
    let packet = vec![
        0x03, 0xC3, 3, 2, 3, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0, 0, 0,
    ];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD, 0xEE]);
    assert_eq!(result.frames[2].data, vec![0xFF, 0x11]);
    assert_eq!(result.padding.len(), 3);
}

#[test_log::test]
fn test_code_3_cbr_with_two_byte_padding() {
    let mut packet = vec![
        0x03, 0x83, 255, 1, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33,
    ];
    packet.extend(vec![0; 255]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.frames[2].data, vec![0x11, 0x22, 0x33]);
    assert_eq!(result.padding.len(), 255);
}

#[test_log::test]
fn test_code_3_vbr_with_two_byte_padding() {
    let mut packet = vec![0x03, 0xC2, 255, 0, 2];
    packet.extend(vec![0xAA, 0xBB, 0xCC, 0xDD]);
    packet.extend(vec![0; 254]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD]);
    assert_eq!(result.padding.len(), 254);
}

#[test_log::test]
fn test_code_3_cbr_with_254_padding() {
    let mut packet = vec![0x03, 0x82, 254, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    packet.extend(vec![0; 254]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.padding.len(), 254);
}

#[test_log::test]
fn test_code_3_cbr_with_chained_255_padding() {
    let mut packet = vec![0x03, 0x82, 255, 255, 2, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    packet.extend(vec![0; 510]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.padding.len(), 510);
}

#[test_log::test]
fn test_code_3_vbr_with_chained_255_padding() {
    let mut packet = vec![0x03, 0xC2, 255, 255, 255, 10, 2];
    packet.extend(vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
    packet.extend(vec![0; 772]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD, 0xEE]);
    assert_eq!(result.padding.len(), 772);
}

#[test_log::test]
fn test_code_3_cbr_no_padding_flag() {
    let packet = vec![
        0x03, 0x03, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33,
    ];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.padding.len(), 0);
}

#[test_log::test]
fn test_code_3_vbr_no_padding_flag() {
    let packet = vec![0x03, 0x43, 2, 3, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.padding.len(), 0);
}

#[test_log::test]
fn test_code_3_padding_too_short_fails() {
    let packet = vec![0x03, 0x83, 10];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_3_two_byte_padding_truncated_fails() {
    let packet = vec![0x03, 0x82, 255, 0xAA, 0xBB];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_3_cbr_with_max_padding_255() {
    let mut packet = vec![0x03, 0x82, 255, 0, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    packet.extend(vec![0; 254]);
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.padding.len(), 254);
}
