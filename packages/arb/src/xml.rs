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
