//! Table of Contents (TOC) byte parsing and Opus configuration types.
//!
//! This module implements the TOC byte parsing logic from RFC 6716 Section 3.1.
//! The TOC byte is the first byte of every Opus packet and encodes the configuration
//! index, stereo flag, and frame count code.
//!
//! The module also provides types for representing Opus modes (SILK/CELT/Hybrid),
//! bandwidths (NB/MB/WB/SWB/FB), and frame sizes, along with a lookup table of
//! all 32 standard Opus configurations defined in RFC 6716 Table 2.

use crate::Channels;

/// Table of Contents (TOC) byte parsed from Opus packet header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toc {
    config: u8,
    stereo: bool,
    frame_count_code: u8,
}

impl Toc {
    /// Parses TOC byte from Opus packet header
    #[must_use]
    pub const fn parse(toc_byte: u8) -> Self {
        Self {
            config: toc_byte >> 3,
            stereo: (toc_byte >> 2) & 0x1 == 1,
            frame_count_code: toc_byte & 0x3,
        }
    }

    /// Returns configuration index (0-31)
    #[must_use]
    pub const fn config(self) -> u8 {
        self.config
    }

    /// Returns channel configuration
    #[must_use]
    pub const fn channels(self) -> Channels {
        if self.stereo {
            Channels::Stereo
        } else {
            Channels::Mono
        }
    }

    /// Returns frame count code (0-3)
    #[must_use]
    pub const fn frame_count_code(self) -> u8 {
        self.frame_count_code
    }

    /// Returns true if packet uses SILK codec (configs 0-15)
    #[must_use]
    pub const fn uses_silk(self) -> bool {
        self.config < 16
    }

    /// Returns true if packet uses Hybrid mode (configs 12-15)
    #[must_use]
    pub const fn is_hybrid(self) -> bool {
        matches!(self.config, 12..=15)
    }

    /// Returns audio bandwidth
    #[must_use]
    pub const fn bandwidth(self) -> Bandwidth {
        match self.config {
            0..=3 | 16..=19 => Bandwidth::Narrowband,
            4..=7 => Bandwidth::Mediumband,
            8..=11 | 20..=23 => Bandwidth::Wideband,
            12..=13 | 24..=27 => Bandwidth::SuperWideband,
            14..=15 | 28..=31 => Bandwidth::Fullband,
            _ => unreachable!(),
        }
    }

    /// Returns frame duration in milliseconds
    #[must_use]
    pub const fn frame_size_ms(self) -> u8 {
        let index = (self.config % 4) as usize;
        match self.config {
            0..=11 => [10, 20, 40, 60][index],
            12..=15 => [10, 20, 10, 20][index],
            16..=31 => [2, 5, 10, 20][index],
            _ => unreachable!(),
        }
    }

    /// Returns frame duration in tenths of milliseconds
    #[must_use]
    pub const fn frame_duration_tenths_ms(self) -> u16 {
        let index = (self.config % 4) as usize;
        match self.config {
            0..=11 => [100, 200, 400, 600][index],
            12..=15 => [100, 200, 100, 200][index],
            16..=31 => [25, 50, 100, 200][index],
            _ => unreachable!(),
        }
    }

    /// Returns complete configuration for this TOC
    #[must_use]
    pub const fn configuration(self) -> Configuration {
        CONFIGURATIONS[self.config as usize]
    }
}

/// Opus encoding mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpusMode {
    /// SILK-only mode (voice-optimized, NB/MB/WB)
    SilkOnly,
    /// Hybrid mode (SILK low frequencies + CELT high frequencies)
    Hybrid,
    /// CELT-only mode (full-spectrum, all bandwidths)
    CeltOnly,
}

/// Audio bandwidth classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// Narrowband (4 kHz, 8 kHz sample rate)
    Narrowband,
    /// Mediumband (6 kHz, 12 kHz sample rate)
    Mediumband,
    /// Wideband (8 kHz, 16 kHz sample rate)
    Wideband,
    /// Super-wideband (12 kHz, 24 kHz sample rate)
    SuperWideband,
    /// Fullband (20 kHz, 48 kHz sample rate)
    Fullband,
}

/// Frame duration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSize {
    /// 2.5 milliseconds
    Ms2_5,
    /// 5 milliseconds
    Ms5,
    /// 10 milliseconds
    Ms10,
    /// 20 milliseconds
    Ms20,
    /// 40 milliseconds
    Ms40,
    /// 60 milliseconds
    Ms60,
}

impl FrameSize {
    /// Convert to milliseconds (for SILK decoder configuration)
    ///
    /// # Note
    ///
    /// 2.5ms truncates to 2ms since u8 cannot represent 2.5.
    /// This is acceptable since SILK doesn't support 2.5ms frames.
    #[must_use]
    pub const fn to_ms(self) -> u8 {
        match self {
            Self::Ms2_5 => 2,
            Self::Ms5 => 5,
            Self::Ms10 => 10,
            Self::Ms20 => 20,
            Self::Ms40 => 40,
            Self::Ms60 => 60,
        }
    }
}

/// Opus configuration combining mode, bandwidth, and frame size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Configuration {
    /// Encoding mode (SILK/CELT/Hybrid)
    pub mode: OpusMode,
    /// Audio bandwidth
    pub bandwidth: Bandwidth,
    /// Frame duration
    pub frame_size: FrameSize,
}

/// Lookup table of all 32 Opus configurations per RFC 6716 Table 2
pub const CONFIGURATIONS: [Configuration; 32] = [
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms40,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms60,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Mediumband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Mediumband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Mediumband,
        frame_size: FrameSize::Ms40,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Mediumband,
        frame_size: FrameSize::Ms60,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms40,
    },
    Configuration {
        mode: OpusMode::SilkOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms60,
    },
    Configuration {
        mode: OpusMode::Hybrid,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::Hybrid,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::Hybrid,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::Hybrid,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms2_5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Narrowband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms2_5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Wideband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms2_5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::SuperWideband,
        frame_size: FrameSize::Ms20,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms2_5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms5,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms10,
    },
    Configuration {
        mode: OpusMode::CeltOnly,
        bandwidth: Bandwidth::Fullband,
        frame_size: FrameSize::Ms20,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toc_parsing_silk_nb() {
        let toc = Toc::parse(0b0000_0000);
        assert_eq!(toc.config(), 0);
        assert_eq!(toc.channels(), Channels::Mono);
        assert_eq!(toc.frame_count_code(), 0);
        assert!(toc.uses_silk());
        assert!(!toc.is_hybrid());
        assert_eq!(toc.bandwidth(), Bandwidth::Narrowband);
        assert_eq!(toc.frame_size_ms(), 10);
    }

    #[test]
    fn test_toc_parsing_hybrid_swb() {
        let toc = Toc::parse(0b0110_0101);
        assert_eq!(toc.config(), 12);
        assert_eq!(toc.channels(), Channels::Stereo);
        assert!(toc.uses_silk());
        assert!(toc.is_hybrid());
        assert_eq!(toc.bandwidth(), Bandwidth::SuperWideband);
    }

    #[test]
    fn test_configuration_silk_nb() {
        let toc = Toc::parse(0b0000_0000);
        let config = toc.configuration();
        assert_eq!(config.mode, OpusMode::SilkOnly);
        assert_eq!(config.bandwidth, Bandwidth::Narrowband);
        assert_eq!(config.frame_size, FrameSize::Ms10);
    }

    #[test]
    fn test_configuration_hybrid_fb() {
        let toc = Toc::parse(0b0111_0100);
        let config = toc.configuration();
        assert_eq!(config.mode, OpusMode::Hybrid);
        assert_eq!(config.bandwidth, Bandwidth::Fullband);
        assert_eq!(config.frame_size, FrameSize::Ms10);
    }

    #[test]
    fn test_configuration_celt_swb() {
        let toc = Toc::parse(0b1100_0000);
        let config = toc.configuration();
        assert_eq!(config.mode, OpusMode::CeltOnly);
        assert_eq!(config.bandwidth, Bandwidth::SuperWideband);
        assert_eq!(config.frame_size, FrameSize::Ms2_5);
    }

    #[test]
    fn test_channels_mono() {
        let toc = Toc::parse(0b0000_0000);
        assert_eq!(toc.channels(), Channels::Mono);
    }

    #[test]
    fn test_channels_stereo() {
        let toc = Toc::parse(0b0000_0100);
        assert_eq!(toc.channels(), Channels::Stereo);
    }

    #[test]
    fn test_all_configurations_match_rfc_table_2() {
        for config in 0..32_u8 {
            let toc = Toc::parse(config << 3);
            let conf = toc.configuration();

            match config {
                0..=11 => assert_eq!(conf.mode, OpusMode::SilkOnly),
                12..=15 => assert_eq!(conf.mode, OpusMode::Hybrid),
                16..=31 => assert_eq!(conf.mode, OpusMode::CeltOnly),
                _ => unreachable!(),
            }

            match config {
                0..=3 | 16..=19 => assert_eq!(conf.bandwidth, Bandwidth::Narrowband),
                4..=7 => assert_eq!(conf.bandwidth, Bandwidth::Mediumband),
                8..=11 | 20..=23 => assert_eq!(conf.bandwidth, Bandwidth::Wideband),
                12..=13 | 24..=27 => assert_eq!(conf.bandwidth, Bandwidth::SuperWideband),
                14..=15 | 28..=31 => assert_eq!(conf.bandwidth, Bandwidth::Fullband),
                _ => unreachable!(),
            }

            let expected_frame_size = match config {
                0..=11 => match config % 4 {
                    0 => FrameSize::Ms10,
                    1 => FrameSize::Ms20,
                    2 => FrameSize::Ms40,
                    3 => FrameSize::Ms60,
                    _ => unreachable!(),
                },
                12..=15 => match config % 4 {
                    0 | 2 => FrameSize::Ms10,
                    1 | 3 => FrameSize::Ms20,
                    _ => unreachable!(),
                },
                16..=31 => match config % 4 {
                    0 => FrameSize::Ms2_5,
                    1 => FrameSize::Ms5,
                    2 => FrameSize::Ms10,
                    3 => FrameSize::Ms20,
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            };
            assert_eq!(conf.frame_size, expected_frame_size);
        }
    }

    #[test]
    fn test_frame_size_to_ms_all_variants() {
        assert_eq!(FrameSize::Ms2_5.to_ms(), 2); // Truncates to 2 (acceptable for SILK)
        assert_eq!(FrameSize::Ms5.to_ms(), 5);
        assert_eq!(FrameSize::Ms10.to_ms(), 10);
        assert_eq!(FrameSize::Ms20.to_ms(), 20);
        assert_eq!(FrameSize::Ms40.to_ms(), 40);
        assert_eq!(FrameSize::Ms60.to_ms(), 60);
    }

    #[test]
    fn test_toc_frame_duration_tenths_ms_all_configs() {
        // Test SILK configs (0-11): 10, 20, 40, 60 ms
        assert_eq!(Toc::parse(0 << 3).frame_duration_tenths_ms(), 100);  // Config 0: 10ms
        assert_eq!(Toc::parse(1 << 3).frame_duration_tenths_ms(), 200);  // Config 1: 20ms
        assert_eq!(Toc::parse(2 << 3).frame_duration_tenths_ms(), 400);  // Config 2: 40ms
        assert_eq!(Toc::parse(3 << 3).frame_duration_tenths_ms(), 600);  // Config 3: 60ms

        // Test Hybrid configs (12-15): 10, 20, 10, 20 ms
        assert_eq!(Toc::parse(12 << 3).frame_duration_tenths_ms(), 100); // Config 12: 10ms
        assert_eq!(Toc::parse(13 << 3).frame_duration_tenths_ms(), 200); // Config 13: 20ms

        // Test CELT configs (16-31): 2.5, 5, 10, 20 ms
        assert_eq!(Toc::parse(16 << 3).frame_duration_tenths_ms(), 25);  // Config 16: 2.5ms
        assert_eq!(Toc::parse(17 << 3).frame_duration_tenths_ms(), 50);  // Config 17: 5ms
        assert_eq!(Toc::parse(18 << 3).frame_duration_tenths_ms(), 100); // Config 18: 10ms
        assert_eq!(Toc::parse(19 << 3).frame_duration_tenths_ms(), 200); // Config 19: 20ms
    }

    #[test]
    fn test_toc_frame_count_code_all_values() {
        assert_eq!(Toc::parse(0b0000_0000).frame_count_code(), 0);
        assert_eq!(Toc::parse(0b0000_0001).frame_count_code(), 1);
        assert_eq!(Toc::parse(0b0000_0010).frame_count_code(), 2);
        assert_eq!(Toc::parse(0b0000_0011).frame_count_code(), 3);
    }

    #[test]
    fn test_bandwidth_debug_display() {
        // Verify all bandwidth variants can be formatted
        let _ = format!("{:?}", Bandwidth::Narrowband);
        let _ = format!("{:?}", Bandwidth::Mediumband);
        let _ = format!("{:?}", Bandwidth::Wideband);
        let _ = format!("{:?}", Bandwidth::SuperWideband);
        let _ = format!("{:?}", Bandwidth::Fullband);
    }

    #[test]
    fn test_opus_mode_debug_display() {
        // Verify all mode variants can be formatted
        let _ = format!("{:?}", OpusMode::SilkOnly);
        let _ = format!("{:?}", OpusMode::CeltOnly);
        let _ = format!("{:?}", OpusMode::Hybrid);
    }
}
