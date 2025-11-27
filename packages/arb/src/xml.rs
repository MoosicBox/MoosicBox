//! XML-compatible string generators for property-based testing.
//!
//! Provides [`Arbitrary`] implementations for generating XML-safe strings and attribute names,
//! along with utilities for validating XML characters.

#![allow(clippy::module_name_repetitions)]

use std::borrow::Cow;

use proptest::prelude::*;

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

/// Strategy that generates valid XML character directly using ranges.
///
/// This avoids filtering by only generating characters that are valid for XML:
/// - U+0020 through U+D7FF (BMP excluding control chars and surrogates)
/// - U+E000 through U+FFFD (private use through BMP end, excluding FFFE/FFFF)
/// - U+10000 through U+10FFFF (all supplementary planes)
///
/// This provides full Unicode coverage matching the original quickcheck implementation.
fn valid_xml_char_strategy() -> impl Strategy<Value = char> {
    prop::char::ranges(Cow::Owned(vec![
        '\u{0020}'..='\u{D7FF}', // BMP: space through end of basic multilingual plane (excludes surrogates)
        '\u{E000}'..='\u{FFFD}', // Private use area through BMP end (excludes FFFE/FFFF)
        '\u{10000}'..='\u{10FFFF}', // All supplementary planes (non-BMP)
    ]))
}

/// Strategy that generates alphanumeric strings with hyphens and underscores.
///
/// Used when `ALPHANUMERIC_STRINGS` environment variable is set to "1".
fn alphanumeric_string_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::char::ranges(Cow::Owned(vec![
            '0'..='9',
            'A'..='Z',
            'a'..='z',
            '-'..='-',
            '_'..='_',
        ])),
        0..100,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

/// Strategy that generates valid XML strings directly.
///
/// Generates strings containing valid XML characters from the full Unicode range.
/// Can be configured to use alphanumeric-only mode via the `ALPHANUMERIC_STRINGS`
/// compile-time environment variable.
fn xml_string_strategy() -> BoxedStrategy<String> {
    if std::option_env!("ALPHANUMERIC_STRINGS") == Some("1") {
        alphanumeric_string_strategy().boxed()
    } else {
        // Full Unicode XML-valid characters, length 0-100
        prop::collection::vec(valid_xml_char_strategy(), 0..100)
            .prop_map(|chars| chars.into_iter().collect())
            .boxed()
    }
}

/// Arbitrary XML-compatible string for property-based testing.
///
/// Generates strings that are valid for use in XML content by excluding
/// invalid XML characters (control characters and specific Unicode ranges).
/// Can be configured to use alphanumeric-only mode via the `ALPHANUMERIC_STRINGS`
/// compile-time environment variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct XmlString(pub String);

impl Arbitrary for XmlString {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        xml_string_strategy().prop_map(Self).boxed()
    }
}

/// Strategy that generates valid XML attribute names directly.
///
/// Generates non-empty strings containing only alphanumeric characters,
/// hyphens, and underscores, suitable for use as XML attribute names.
/// This matches the original quickcheck behavior which allowed any of these
/// characters in any position.
fn xml_attr_name_strategy() -> impl Strategy<Value = String> {
    // All chars can be alphanumeric, hyphen, or underscore (matching original quickcheck)
    // Length: 1-100 (non-empty)
    prop::collection::vec(
        prop::char::ranges(Cow::Owned(vec![
            '0'..='9',
            'A'..='Z',
            'a'..='z',
            '-'..='-',
            '_'..='_',
        ])),
        1..100,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

/// Arbitrary XML attribute name string for property-based testing.
///
/// Generates non-empty strings containing only alphanumeric characters,
/// hyphens, and underscores, suitable for use as XML attribute names.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct XmlAttrNameString(pub String);

impl Arbitrary for XmlAttrNameString {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        xml_attr_name_strategy().prop_map(Self).boxed()
    }
}
