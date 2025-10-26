#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use color_hex::color_from_hex;
use thiserror::Error;

#[cfg(feature = "arb")]
pub mod arb;

/// Errors that can occur when parsing a hex color string.
#[derive(Debug, Error)]
pub enum ParseHexError {
    /// An invalid hex character was encountered at the specified index.
    #[error("Invalid character at index {0} '{1}'")]
    InvalidCharacter(usize, char),
    /// A non-ASCII character was encountered at the specified index.
    #[error("Invalid non-ASCII character at index {0}")]
    InvalidNonAsciiCharacter(usize),
    /// The hex string is longer than 8 characters (excluding '#' prefix and whitespace).
    #[error("Hex string too long")]
    StringTooLong,
    /// The hex string has an invalid length (e.g., incomplete alpha channel).
    #[error("Hex string invalid length")]
    InvalidLength,
}

/// Represents an RGB or RGBA color with 8-bit channels.
///
/// # Fields
///
/// * `r` - Red channel (0-255)
/// * `g` - Green channel (0-255)
/// * `b` - Blue channel (0-255)
/// * `a` - Optional alpha channel (0-255). `None` represents fully opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: Option<u8>,
}

impl Color {
    /// Black color constant (RGB: 0, 0, 0).
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: None,
    };

    /// White color constant (RGB: 255, 255, 255).
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: None,
    };

    /// Parses a hex string (a-f/A-F/0-9) as a `Color` from the &str,
    /// ignoring surrounding whitespace.
    ///
    /// Accepts hex strings in formats: RGB (3 chars), RGBA (4 chars),
    /// RRGGBB (6 chars), or RRGGBBAA (8 chars). The '#' prefix is optional.
    ///
    /// # Errors
    ///
    /// * `ParseHexError::InvalidCharacter` - If a non-hex, non-whitespace ASCII character is encountered.
    /// * `ParseHexError::InvalidNonAsciiCharacter` - If a non-ASCII character is encountered.
    /// * `ParseHexError::StringTooLong` - If the hex string is longer than 8 characters (excluding '#' and whitespace).
    /// * `ParseHexError::InvalidLength` - If the hex string has an incomplete alpha channel (7 characters).
    #[allow(clippy::many_single_char_names)]
    pub fn try_from_hex(hex: &str) -> Result<Self, ParseHexError> {
        let mut short_r = 0;
        let mut short_g = 0;
        let mut short_b = 0;
        let mut short_a = 0;
        let mut three_chars = false;
        let mut four_chars = false;

        let mut r = 0;
        let mut g = 0;
        let mut b = 0;
        let mut maybe_a = None;
        let mut a = None;

        let hex = hex.strip_prefix('#').unwrap_or(hex);

        for (i, value) in hex.trim().chars().enumerate().map(|(i, x)| {
            (
                i,
                match x {
                    '0'..='9' => Ok(x as u8 - 48),
                    'A'..='F' => Ok(x as u8 - 55),
                    'a'..='f' => Ok(x as u8 - 87),
                    c if c.is_ascii() => Err(ParseHexError::InvalidCharacter(i, x)),
                    _ => Err(ParseHexError::InvalidNonAsciiCharacter(i)),
                },
            )
        }) {
            let value = value?;
            match i {
                0 => {
                    short_r = value;
                    r = value << 4;
                }
                1 => {
                    short_g = value;
                    r += value;
                }
                2 => {
                    three_chars = true;
                    short_b = value;
                    g = value << 4;
                }
                3 => {
                    three_chars = false;
                    four_chars = true;
                    short_a = value;
                    g += value;
                }
                4 => {
                    four_chars = false;
                    b = value << 4;
                }
                5 => {
                    b += value;
                }
                6 => {
                    maybe_a = Some(value << 4);
                }
                7 => {
                    a = maybe_a.map(|a| a + value);
                }
                _ => {
                    return Err(ParseHexError::StringTooLong);
                }
            }
        }

        moosicbox_assert::assert_or_err!(
            maybe_a.is_none() || a.is_some(),
            ParseHexError::InvalidLength,
        );

        if three_chars {
            r = (short_r << 4) + short_r;
            g = (short_g << 4) + short_g;
            b = (short_b << 4) + short_b;
        }
        if four_chars {
            r = (short_r << 4) + short_r;
            g = (short_g << 4) + short_g;
            b = (short_b << 4) + short_b;
            a = Some((short_a << 4) + short_a);
        }

        Ok(Self { r, g, b, a })
    }

    /// Parses a hex string (a-f/A-F/0-9) as a `Color` from the &str,
    /// ignoring surrounding whitespace.
    ///
    /// # Panics
    ///
    /// * If a non-hex, non-whitespace character is encountered.
    #[must_use]
    pub fn from_hex(hex: &str) -> Self {
        Self::try_from_hex(hex).unwrap()
    }
}

#[cfg(feature = "egui")]
impl From<Color> for egui::Color32 {
    fn from(value: Color) -> Self {
        value.a.map_or_else(
            || Self::from_rgb(value.r, value.g, value.b),
            |a| Self::from_rgba_unmultiplied(value.r, value.g, value.b, a),
        )
    }
}

#[cfg(feature = "egui")]
impl From<&Color> for egui::Color32 {
    fn from(value: &Color) -> Self {
        value.a.map_or_else(
            || Self::from_rgb(value.r, value.g, value.b),
            |a| Self::from_rgba_unmultiplied(value.r, value.g, value.b, a),
        )
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(a) = self.a {
            f.write_fmt(format_args!(
                "#{:02X}{:02X}{:02X}{:02X}",
                self.r, self.g, self.b, a
            ))
        } else {
            f.write_fmt(format_args!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b))
        }
    }
}

impl From<&str> for Color {
    fn from(s: &str) -> Self {
        Self::from_hex(s)
    }
}

impl From<String> for Color {
    fn from(s: String) -> Self {
        Self::from_hex(&s)
    }
}

impl From<&String> for Color {
    fn from(s: &String) -> Self {
        Self::from_hex(s)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::Color;

    #[test_log::test]
    fn can_parse_rgb_hex_string_to_color() {
        assert_eq!(
            Color::from_hex("#010203"),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_rgba_hex_string_to_color() {
        assert_eq!(
            Color::from_hex("#01020304"),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: Some(4)
            }
        );
    }

    #[test_log::test]
    fn can_display_small_rgb_as_hex_string() {
        assert_eq!(
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: None
            }
            .to_string(),
            "#010203".to_string(),
        );
    }

    #[test_log::test]
    fn can_display_large_rgb_as_hex_string() {
        assert_eq!(
            Color {
                r: 255,
                g: 2,
                b: 254,
                a: None
            }
            .to_string(),
            "#FF02FE".to_string(),
        );
    }

    #[test_log::test]
    fn can_display_small_rgba_as_hex_string() {
        assert_eq!(
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: Some(4)
            }
            .to_string(),
            "#01020304".to_string(),
        );
    }

    #[test_log::test]
    fn can_display_large_rgba_as_hex_string() {
        assert_eq!(
            Color {
                r: 255,
                g: 2,
                b: 254,
                a: Some(4)
            }
            .to_string(),
            "#FF02FE04".to_string(),
        );
    }
}
