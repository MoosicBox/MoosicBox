use crate::Channels;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toc {
    config: u8,
    stereo: bool,
    frame_count_code: u8,
}

impl Toc {
    #[must_use]
    pub const fn parse(toc_byte: u8) -> Self {
        Self {
            config: toc_byte >> 3,
            stereo: (toc_byte >> 2) & 0x1 == 1,
            frame_count_code: toc_byte & 0x3,
        }
    }

    #[must_use]
    pub const fn config(self) -> u8 {
        self.config
    }

    #[must_use]
    pub const fn channels(self) -> Channels {
        if self.stereo {
            Channels::Stereo
        } else {
            Channels::Mono
        }
    }

    #[must_use]
    pub const fn frame_count_code(self) -> u8 {
        self.frame_count_code
    }

    #[must_use]
    pub const fn uses_silk(self) -> bool {
        self.config < 16
    }

    #[must_use]
    pub const fn is_hybrid(self) -> bool {
        matches!(self.config, 12..=15)
    }

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

    #[must_use]
    pub const fn configuration(self) -> Configuration {
        CONFIGURATIONS[self.config as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpusMode {
    SilkOnly,
    Hybrid,
    CeltOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    Narrowband,
    Mediumband,
    Wideband,
    SuperWideband,
    Fullband,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSize {
    Ms2_5,
    Ms5,
    Ms10,
    Ms20,
    Ms40,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Configuration {
    pub mode: OpusMode,
    pub bandwidth: Bandwidth,
    pub frame_size: FrameSize,
}

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
}
