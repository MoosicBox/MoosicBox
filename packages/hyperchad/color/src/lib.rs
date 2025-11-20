//! A lightweight color parsing and manipulation library.
//!
//! This crate provides a simple [`Color`] type representing RGB/RGBA colors with 8-bit channels,
//! along with utilities for parsing hex color strings in various formats.
//!
//! # Features
//!
//! * Parse hex color strings in multiple formats (RGB, RGBA, RRGGBB, RRGGBBAA)
//! * Support for optional alpha channel
//! * Conversion to/from hex strings
//! * Optional integration with egui (via `egui` feature)
//! * Optional property testing support (via `arb` feature)
//! * Optional serialization support (via `serde` feature)
//!
//! # Examples
//!
//! ```rust
//! use hyperchad_color::Color;
//!
//! // Parse a hex color string
//! let color = Color::from_hex("#FF5733");
//! assert_eq!(color.r, 255);
//! assert_eq!(color.g, 87);
//! assert_eq!(color.b, 51);
//!
//! // Use predefined constants
//! let black = Color::BLACK;
//! let white = Color::WHITE;
//!
//! // Convert back to hex string
//! assert_eq!(color.to_string(), "#FF5733");
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Re-export of the `color_from_hex!` macro from the `color-hex` crate.
///
/// This macro provides compile-time hex color parsing. Use it when you need
/// to parse hex colors at compile time rather than runtime.
///
/// # Examples
///
/// ```rust
/// use hyperchad_color::color_from_hex;
///
/// let color = color_from_hex!("#FF5733");
/// ```
pub use color_hex::color_from_hex;
use thiserror::Error;

/// Property testing support via QuickCheck.
///
/// This module provides an [`Arbitrary`] implementation for [`Color`],
/// enabling property-based testing with the [`quickcheck`] crate.
///
/// [`Arbitrary`]: quickcheck::Arbitrary
/// [`quickcheck`]: https://docs.rs/quickcheck/latest/quickcheck/
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    /// Red channel (0-255).
    pub r: u8,
    /// Green channel (0-255).
    pub g: u8,
    /// Blue channel (0-255).
    pub b: u8,
    /// Optional alpha channel (0-255). `None` represents fully opaque.
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

/// Converts [`Color`] to [`egui::Color32`].
///
/// If the color has an alpha channel, it creates an RGBA color; otherwise,
/// it creates an opaque RGB color.
#[cfg(feature = "egui")]
impl From<Color> for egui::Color32 {
    fn from(value: Color) -> Self {
        value.a.map_or_else(
            || Self::from_rgb(value.r, value.g, value.b),
            |a| Self::from_rgba_unmultiplied(value.r, value.g, value.b, a),
        )
    }
}

/// Converts a reference to [`Color`] to [`egui::Color32`].
///
/// If the color has an alpha channel, it creates an RGBA color; otherwise,
/// it creates an opaque RGB color.
#[cfg(feature = "egui")]
impl From<&Color> for egui::Color32 {
    fn from(value: &Color) -> Self {
        value.a.map_or_else(
            || Self::from_rgb(value.r, value.g, value.b),
            |a| Self::from_rgba_unmultiplied(value.r, value.g, value.b, a),
        )
    }
}

/// Converts [`Color`] to a hex string representation.
///
/// Outputs uppercase hex format with '#' prefix:
/// * RGB colors: `#RRGGBB` (6 characters)
/// * RGBA colors: `#RRGGBBAA` (8 characters)
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

/// Converts a hex string to [`Color`].
///
/// # Panics
///
/// * If the string contains invalid hex characters or has an invalid format.
impl From<&str> for Color {
    fn from(s: &str) -> Self {
        Self::from_hex(s)
    }
}

/// Converts an owned [`String`] containing a hex color to [`Color`].
///
/// # Panics
///
/// * If the string contains invalid hex characters or has an invalid format.
impl From<String> for Color {
    fn from(s: String) -> Self {
        Self::from_hex(&s)
    }
}

/// Converts a reference to [`String`] containing a hex color to [`Color`].
///
/// # Panics
///
/// * If the string contains invalid hex characters or has an invalid format.
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

    // Short hex format tests (3 and 4 character formats)
    #[test_log::test]
    fn can_parse_short_rgb_hex_string() {
        // #ABC should expand to #AABBCC
        assert_eq!(
            Color::from_hex("#ABC"),
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_short_rgba_hex_string() {
        // #ABCD should expand to #AABBCCDD
        assert_eq!(
            Color::from_hex("#ABCD"),
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
                a: Some(0xDD)
            }
        );
    }

    #[test_log::test]
    fn can_parse_short_rgb_with_zero_values() {
        // #000 should expand to #000000
        assert_eq!(
            Color::from_hex("#000"),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_short_rgb_with_max_values() {
        // #FFF should expand to #FFFFFF
        assert_eq!(
            Color::from_hex("#FFF"),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_short_rgba_with_zero_alpha() {
        // #ABC0 should expand to #AABBCC00
        assert_eq!(
            Color::from_hex("#ABC0"),
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
                a: Some(0)
            }
        );
    }

    // Error handling tests
    #[test_log::test]
    fn invalid_character_returns_error() {
        let result = Color::try_from_hex("#GGHHII");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::InvalidCharacter(idx, ch) => {
                assert_eq!(idx, 0);
                assert_eq!(ch, 'G');
            }
            _ => panic!("Expected InvalidCharacter error"),
        }
    }

    #[test_log::test]
    fn non_ascii_character_returns_error() {
        let result = Color::try_from_hex("#日本語");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::InvalidNonAsciiCharacter(idx) => {
                assert_eq!(idx, 0);
            }
            _ => panic!("Expected InvalidNonAsciiCharacter error"),
        }
    }

    #[test_log::test]
    fn string_too_long_returns_error() {
        let result = Color::try_from_hex("#123456789");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::StringTooLong => {}
            _ => panic!("Expected StringTooLong error"),
        }
    }

    #[test_log::test]
    fn invalid_length_returns_error() {
        // 7 characters is invalid (incomplete alpha channel)
        let result = Color::try_from_hex("#1234567");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::InvalidLength => {}
            _ => panic!("Expected InvalidLength error"),
        }
    }

    // Edge cases in parsing
    #[test_log::test]
    fn can_parse_hex_without_hash_prefix() {
        assert_eq!(
            Color::from_hex("FF5733"),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_hex_with_trailing_whitespace() {
        assert_eq!(
            Color::from_hex("#FF5733  "),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_hex_with_trailing_whitespace_no_prefix() {
        assert_eq!(
            Color::from_hex("FF5733  "),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_lowercase_hex() {
        assert_eq!(
            Color::from_hex("#ff5733"),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_mixed_case_hex() {
        assert_eq!(
            Color::from_hex("#Ff5733"),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_uppercase_hex() {
        assert_eq!(
            Color::from_hex("#FF5733"),
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    // From trait implementations
    #[test_log::test]
    fn can_convert_from_str_ref() {
        let color = Color::from("#FF5733");
        assert_eq!(
            color,
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_convert_from_string() {
        let hex_string = String::from("#FF5733");
        let color = Color::from(hex_string);
        assert_eq!(
            color,
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_convert_from_string_ref() {
        let hex_string = String::from("#FF5733");
        let color = Color::from(&hex_string);
        assert_eq!(
            color,
            Color {
                r: 255,
                g: 87,
                b: 51,
                a: None
            }
        );
    }

    // Color constants tests
    #[test_log::test]
    fn black_constant_has_correct_values() {
        assert_eq!(Color::BLACK.r, 0);
        assert_eq!(Color::BLACK.g, 0);
        assert_eq!(Color::BLACK.b, 0);
        assert_eq!(Color::BLACK.a, None);
    }

    #[test_log::test]
    fn white_constant_has_correct_values() {
        assert_eq!(Color::WHITE.r, 255);
        assert_eq!(Color::WHITE.g, 255);
        assert_eq!(Color::WHITE.b, 255);
        assert_eq!(Color::WHITE.a, None);
    }

    #[test_log::test]
    fn black_constant_displays_as_hex() {
        assert_eq!(Color::BLACK.to_string(), "#000000");
    }

    #[test_log::test]
    fn white_constant_displays_as_hex() {
        assert_eq!(Color::WHITE.to_string(), "#FFFFFF");
    }

    // Additional edge cases for robustness
    #[test_log::test]
    fn can_parse_all_zeros_rgba() {
        assert_eq!(
            Color::from_hex("#00000000"),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: Some(0)
            }
        );
    }

    #[test_log::test]
    fn can_parse_all_max_rgba() {
        assert_eq!(
            Color::from_hex("#FFFFFFFF"),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: Some(255)
            }
        );
    }

    #[test_log::test]
    fn invalid_character_in_middle_returns_error() {
        let result = Color::try_from_hex("#FF5G33");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::InvalidCharacter(idx, ch) => {
                assert_eq!(idx, 3);
                assert_eq!(ch, 'G');
            }
            _ => panic!("Expected InvalidCharacter error"),
        }
    }

    #[test_log::test]
    fn special_ascii_character_returns_error() {
        let result = Color::try_from_hex("#FF5@33");
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::ParseHexError::InvalidCharacter(idx, ch) => {
                assert_eq!(idx, 3);
                assert_eq!(ch, '@');
            }
            _ => panic!("Expected InvalidCharacter error"),
        }
    }

    #[test_log::test]
    fn empty_string_parses_as_black() {
        // Empty strings result in all zeros (black)
        assert_eq!(
            Color::from_hex(""),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: None
            }
        );
    }

    #[test_log::test]
    fn only_hash_parses_as_black() {
        // Just a hash results in all zeros (black)
        assert_eq!(
            Color::from_hex("#"),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: None
            }
        );
    }

    #[test_log::test]
    fn single_character_parses_as_color() {
        // Single character is treated as incomplete short format
        // Based on the logic, this would set short_r and r
        let result = Color::try_from_hex("#A");
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn two_characters_parses_as_color() {
        // Two characters would set r and g values
        let result = Color::try_from_hex("#AB");
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn five_characters_parses_as_color() {
        // Five characters would parse successfully
        let result = Color::try_from_hex("#ABCDE");
        assert!(result.is_ok());
    }
}
