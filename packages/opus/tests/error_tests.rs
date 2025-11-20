#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::error::Error;

#[test]
fn test_error_display_invalid_packet() {
    let err = Error::InvalidPacket;
    assert_eq!(err.to_string(), "Invalid packet format");
}

#[test]
fn test_error_display_decoding_failed() {
    let err = Error::DecodingFailed;
    assert_eq!(err.to_string(), "Decoding failed");
}

#[test]
fn test_error_display_invalid_frame_length() {
    let err = Error::InvalidFrameLength(1500);
    assert_eq!(
        err.to_string(),
        "Invalid frame length: 1500 bytes (max 1275)"
    );
}

#[test]
fn test_error_display_packet_too_short() {
    let err = Error::PacketTooShort(5);
    assert_eq!(err.to_string(), "Packet too short: 5 bytes");
}

#[test]
fn test_error_debug_format() {
    let err = Error::InvalidPacket;
    let debug_str = format!("{err:?}");
    assert!(debug_str.contains("InvalidPacket"));
}
