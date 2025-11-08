//! Table of Contents (TOC) byte parsing and Opus configuration types.
//!
//! This module provides [`TocByte`] for parsing the TOC byte as defined in RFC 6716
//! Section 3.1, along with related types for Opus mode and bandwidth.

use crate::error::Result;

/// TOC (Table of Contents) byte structure from RFC 6716 Section 3.1.
///
/// The TOC byte is the first byte of every Opus packet and encodes the
/// configuration number, stereo flag, and frame packing code.
#[derive(Debug, Clone, Copy)]
pub struct TocByte {
    /// Configuration number (0-31).
    ///
    /// Determines the Opus mode (SILK, Hybrid, or CELT) and bandwidth.
    /// See RFC 6716 Table 2 for the mapping.
    config: u8,

    /// Whether the packet is encoded in stereo.
    ///
    /// When true, the packet contains stereo audio. When false, it's mono.
    stereo: bool,

    /// Frame packing code (0-3).
    ///
    /// Determines how frames are structured within the packet:
    /// * 0: Single frame
    /// * 1: Two equal frames
    /// * 2: Two variable frames
    /// * 3: Arbitrary number of frames
    frame_code: u8,
}

impl TocByte {
    /// Parse a TOC byte.
    ///
    /// # Errors
    ///
    /// Currently never returns an error, but uses Result for future compatibility.
    pub const fn parse(byte: u8) -> Result<Self> {
        let config = (byte >> 3) & 0x1F;
        let stereo = (byte & 0x04) != 0;
        let frame_code = byte & 0x03;

        Ok(Self {
            config,
            stereo,
            frame_code,
        })
    }

    /// Get the configuration number (0-31).
    ///
    /// The configuration number determines the Opus mode and bandwidth.
    /// See RFC 6716 Table 2 for the complete mapping.
    #[must_use]
    pub const fn config(&self) -> u8 {
        self.config
    }

    /// Check if the packet is encoded in stereo.
    ///
    /// Returns `true` for stereo audio, `false` for mono.
    #[must_use]
    pub const fn is_stereo(&self) -> bool {
        self.stereo
    }

    /// Get the frame packing code (0-3).
    ///
    /// Returns the code that determines how frames are structured:
    /// * 0: Single frame
    /// * 1: Two equal frames
    /// * 2: Two variable frames
    /// * 3: Arbitrary number of frames
    #[must_use]
    pub const fn frame_code(&self) -> u8 {
        self.frame_code
    }
}

/// Opus encoding mode derived from the configuration number.
///
/// Opus supports three encoding modes optimized for different audio characteristics.
/// The mode is determined by the configuration number in the TOC byte.
#[derive(Debug, Clone, Copy)]
pub enum OpusMode {
    /// SILK-only mode optimized for speech.
    ///
    /// Uses the SILK codec, which excels at low-bitrate speech encoding.
    /// Typically used for configurations 0-11.
    SilkOnly,

    /// Hybrid mode combining SILK and CELT.
    ///
    /// Uses both SILK (for low frequencies) and CELT (for high frequencies),
    /// providing good quality for mixed speech and music content.
    /// Typically used for configurations 12-15.
    Hybrid,

    /// CELT-only mode optimized for music and general audio.
    ///
    /// Uses the CELT codec, which provides high-quality full-bandwidth encoding
    /// suitable for music. Typically used for configurations 16-31.
    CeltOnly,
}

/// Audio bandwidth modes in Opus.
///
/// Defines the frequency range of the encoded audio. Higher bandwidth modes
/// provide better audio quality but require higher bitrates. The bandwidth
/// is encoded in the configuration number within the TOC byte.
#[derive(Debug, Clone, Copy)]
pub enum Bandwidth {
    /// Narrowband: 4 kHz cutoff frequency.
    ///
    /// Suitable for low-quality speech in bandwidth-constrained scenarios.
    Narrowband,

    /// Mediumband: 6 kHz cutoff frequency.
    ///
    /// Better than narrowband for speech while still conserving bandwidth.
    Mediumband,

    /// Wideband: 8 kHz cutoff frequency.
    ///
    /// Standard telephone quality, suitable for most speech applications.
    Wideband,

    /// Super-wideband: 12 kHz cutoff frequency.
    ///
    /// High-quality speech and good for mixed speech/music content.
    SuperWideband,

    /// Fullband: 20 kHz cutoff frequency.
    ///
    /// Full audio spectrum, suitable for high-quality music encoding.
    Fullband,
}
