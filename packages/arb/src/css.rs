//! CSS identifier generators for property-based testing.
//!
//! Provides [`Arbitrary`] implementations for generating valid CSS identifier strings.

#![allow(clippy::module_name_repetitions)]

use quickcheck::{Arbitrary, Gen};

/// Arbitrary CSS identifier string for property-based testing.
///
/// Generates valid CSS identifier strings containing only alphanumeric characters,
/// hyphens, and underscores. The generated strings are non-empty and contain at
/// least one alphanumeric character (not just hyphens or underscores).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CssIdentifierString(pub String);

impl Arbitrary for CssIdentifierString {
    fn arbitrary(g: &mut Gen) -> Self {
        let string = loop {
            let string = String::arbitrary(g);
            if !string.is_empty()
                && string
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_'))
                && !string.chars().all(|c| matches!(c, '-' | '_'))
            {
                break string;
            }
        };

        Self(string)
    }
}
