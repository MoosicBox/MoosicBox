use crate::error::Result;

/// TOC byte structure (RFC 6716 Section 3.1).
#[derive(Debug, Clone, Copy)]
pub struct TocByte {
    /// Configuration number (0-31)
    config: u8,
    /// Stereo flag
    stereo: bool,
    /// Frame count code (0-3)
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

    /// Get configuration number.
    #[must_use]
    pub const fn config(&self) -> u8 {
        self.config
    }

    /// Check if stereo.
    #[must_use]
    pub const fn is_stereo(&self) -> bool {
        self.stereo
    }

    /// Get frame count code.
    #[must_use]
    pub const fn frame_code(&self) -> u8 {
        self.frame_code
    }
}

/// Opus mode derived from configuration.
#[derive(Debug, Clone, Copy)]
pub enum OpusMode {
    /// SILK mode for speech
    SilkOnly,
    /// Hybrid mode
    Hybrid,
    /// CELT mode for music
    CeltOnly,
}

/// Audio bandwidth.
#[derive(Debug, Clone, Copy)]
pub enum Bandwidth {
    /// 4 kHz
    Narrowband,
    /// 6 kHz
    Mediumband,
    /// 8 kHz
    Wideband,
    /// 12 kHz
    SuperWideband,
    /// 20 kHz
    Fullband,
}
