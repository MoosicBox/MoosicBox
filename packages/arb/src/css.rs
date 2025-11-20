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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_identifier_string_never_empty() {
        // Generate multiple CSS identifiers and verify none are empty
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let identifier = CssIdentifierString::arbitrary(&mut g);
            assert!(
                !identifier.0.is_empty(),
                "CssIdentifierString should never be empty"
            );
        }
    }

    #[test]
    fn test_css_identifier_string_valid_characters() {
        // Verify that generated identifiers only contain valid characters
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let identifier = CssIdentifierString::arbitrary(&mut g);
            assert!(
                identifier
                    .0
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_')),
                "CssIdentifierString contains invalid character: {}",
                identifier.0
            );
        }
    }

    #[test]
    fn test_css_identifier_string_not_only_hyphens_underscores() {
        // Critical constraint: identifiers must contain at least one alphanumeric character
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let identifier = CssIdentifierString::arbitrary(&mut g);
            assert!(
                !identifier.0.chars().all(|c| matches!(c, '-' | '_')),
                "CssIdentifierString should not be only hyphens and underscores: {}",
                identifier.0
            );
        }
    }

    #[test]
    fn test_css_identifier_string_has_alphanumeric() {
        // Verify that generated identifiers contain at least one alphanumeric character
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let identifier = CssIdentifierString::arbitrary(&mut g);
            assert!(
                identifier
                    .0
                    .chars()
                    .any(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z')),
                "CssIdentifierString should contain at least one alphanumeric character: {}",
                identifier.0
            );
        }
    }
}
