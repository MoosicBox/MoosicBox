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

    #[test_log::test]
    fn is_invalid_xml_char_identifies_null_as_invalid() {
        assert!(is_invalid_xml_char('\u{0000}'));
    }

    #[test_log::test]
    fn is_invalid_xml_char_identifies_last_control_char_as_invalid() {
        // U+001F is the last control character in the first range
        assert!(is_invalid_xml_char('\u{001F}'));
    }

    #[test_log::test]
    fn is_invalid_xml_char_identifies_space_as_valid() {
        // U+0020 (space) is the first character after the control range
        assert!(!is_invalid_xml_char('\u{0020}'));
    }

    #[test_log::test]
    fn is_invalid_xml_char_identifies_replacement_char_as_valid() {
        // U+FFFD (replacement character) should be valid
        assert!(!is_invalid_xml_char('\u{FFFD}'));
    }

    #[test_log::test]
    fn is_invalid_xml_char_identifies_fffe_as_invalid() {
        // U+FFFE is a non-character
        assert!(is_invalid_xml_char('\u{FFFE}'));
    }

    #[test_log::test]
    fn is_invalid_xml_char_identifies_ffff_as_invalid() {
        // U+FFFF is a non-character
        assert!(is_invalid_xml_char('\u{FFFF}'));
    }

    #[test_log::test]
    fn is_valid_xml_char_is_inverse_of_is_invalid_xml_char() {
        // Test that is_valid_xml_char is the exact inverse for key boundary characters
        let test_chars = [
            '\u{0000}', '\u{001F}', '\u{0020}', 'A', '\u{FFFD}', '\u{FFFE}', '\u{FFFF}',
        ];
        for c in test_chars {
            assert_eq!(
                is_valid_xml_char(c),
                !is_invalid_xml_char(c),
                "is_valid_xml_char and is_invalid_xml_char should be inverses for {c:?}"
            );
        }
    }
}
