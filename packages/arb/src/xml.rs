//! XML-compatible string generators for property-based testing.
//!
//! Provides [`Arbitrary`] implementations for generating XML-safe strings and attribute names,
//! along with utilities for validating XML characters.

#![allow(clippy::module_name_repetitions)]

use quickcheck::{Arbitrary, Gen};

/// Arbitrary XML-compatible string for property-based testing.
///
/// Generates strings that are valid for use in XML content by excluding
/// invalid XML characters (control characters and specific Unicode ranges).
/// Can be configured to use alphanumeric-only mode via the `ALPHANUMERIC_STRINGS`
/// environment variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct XmlString(pub String);

impl Arbitrary for XmlString {
    fn arbitrary(g: &mut Gen) -> Self {
        let string = loop {
            let string = String::arbitrary(g);
            if std::option_env!("ALPHANUMERIC_STRINGS") == Some("1") {
                if string
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_'))
                {
                    break string;
                }
            } else if string.chars().all(is_valid_xml_char) {
                break string;
            }
        };

        Self(string)
    }
}

/// Checks if a character is invalid for use in XML content.
///
/// Returns `true` for control characters (U+0000 through U+001F) and
/// the Unicode non-characters U+FFFE and U+FFFF.
#[must_use]
pub const fn is_invalid_xml_char(c: char) -> bool {
    matches!(c, '\u{0000}'..='\u{001F}' | '\u{FFFE}'..='\u{FFFF}')
}

/// Checks if a character is valid for use in XML content.
///
/// Returns `true` for all characters except control characters and specific
/// Unicode non-characters. This is the inverse of [`is_invalid_xml_char`].
#[must_use]
pub const fn is_valid_xml_char(c: char) -> bool {
    !is_invalid_xml_char(c)
}

/// Arbitrary XML attribute name string for property-based testing.
///
/// Generates non-empty strings containing only alphanumeric characters,
/// hyphens, and underscores, suitable for use as XML attribute names.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct XmlAttrNameString(pub String);

impl Arbitrary for XmlAttrNameString {
    fn arbitrary(g: &mut Gen) -> Self {
        let string = loop {
            let string = String::arbitrary(g);
            if !string.is_empty()
                && string
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_'))
            {
                break string;
            }
        };

        Self(string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_invalid_xml_char_control_characters() {
        // Test control characters (U+0000 through U+001F)
        assert!(is_invalid_xml_char('\u{0000}'));
        assert!(is_invalid_xml_char('\u{0001}'));
        assert!(is_invalid_xml_char('\u{000F}'));
        assert!(is_invalid_xml_char('\u{001F}'));
    }

    #[test]
    fn test_is_invalid_xml_char_non_characters() {
        // Test Unicode non-characters
        assert!(is_invalid_xml_char('\u{FFFE}'));
        assert!(is_invalid_xml_char('\u{FFFF}'));
    }

    #[test]
    fn test_is_invalid_xml_char_valid_characters() {
        // Test valid characters that should NOT be considered invalid
        assert!(!is_invalid_xml_char('\u{0020}')); // Space (just after control range)
        assert!(!is_invalid_xml_char('a'));
        assert!(!is_invalid_xml_char('Z'));
        assert!(!is_invalid_xml_char('0'));
        assert!(!is_invalid_xml_char('!'));
        assert!(!is_invalid_xml_char('\u{FFFD}')); // Replacement character (just before non-chars)
    }

    #[test]
    fn test_is_valid_xml_char_is_inverse_of_invalid() {
        // Test that is_valid_xml_char is the logical inverse of is_invalid_xml_char
        let test_chars = [
            '\u{0000}', '\u{0010}', '\u{001F}', '\u{0020}', 'a', 'Z', '0', '!', '\u{FFFD}',
            '\u{FFFE}', '\u{FFFF}',
        ];

        for &c in &test_chars {
            assert_eq!(is_valid_xml_char(c), !is_invalid_xml_char(c));
        }
    }

    #[test]
    fn test_xml_attr_name_string_never_empty() {
        // Generate multiple attribute names and verify none are empty
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let attr = XmlAttrNameString::arbitrary(&mut g);
            assert!(
                !attr.0.is_empty(),
                "XmlAttrNameString should never be empty"
            );
        }
    }

    #[test]
    fn test_xml_attr_name_string_valid_characters() {
        // Verify that generated attribute names only contain valid characters
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let attr = XmlAttrNameString::arbitrary(&mut g);
            assert!(
                attr.0
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_')),
                "XmlAttrNameString contains invalid character: {}",
                attr.0
            );
        }
    }

    #[test]
    fn test_xml_string_no_invalid_characters() {
        // Verify that generated XML strings contain no invalid XML characters
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let xml_str = XmlString::arbitrary(&mut g);
            for c in xml_str.0.chars() {
                assert!(
                    is_valid_xml_char(c),
                    "XmlString contains invalid XML character: {:?} (U+{:04X})",
                    c,
                    c as u32
                );
            }
        }
    }
}
