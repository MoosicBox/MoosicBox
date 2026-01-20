//! SILK frame types and metadata structures.
//!
//! This module defines frame-level types used during SILK decoding, including
//! frame type classification (Inactive/Unvoiced/Voiced) and quantization parameters.

/// SILK frame metadata decoded from bitstream
///
/// Contains frame-level parameters decoded from SILK header that control
/// the decoding process for the frame's subframes.
pub struct SilkFrame {
    /// Frame classification (Inactive/Unvoiced/Voiced)
    pub frame_type: FrameType,
    /// Voice Activity Detection flag
    pub vad_flag: bool,
    /// Number of subframes in this frame (1-4)
    pub subframe_count: usize,
    /// Quantized gain indices per subframe
    pub subframe_gains: Vec<u8>,
}

/// SILK frame type classification per RFC 6716 Section 4.2.7.1
///
/// Determines which probability distributions to use for gain and frame type decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Inactive frame (silence/background noise)
    Inactive,
    /// Unvoiced frame (consonants, noise-like sounds)
    Unvoiced,
    /// Voiced frame (vowels, periodic sounds)
    Voiced,
}

/// Quantization offset type for LSF indices
///
/// Controls the offset applied during Line Spectral Frequency quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum QuantizationOffsetType {
    /// Low offset quantization
    Low,
    /// High offset quantization
    High,
}
