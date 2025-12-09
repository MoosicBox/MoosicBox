//! CSS identifier generators for property-based testing.
//!
//! Provides [`proptest::arbitrary::Arbitrary`] implementations for generating valid CSS
//! identifier strings.

#![allow(clippy::module_name_repetitions)]

use std::borrow::Cow;

use proptest::prelude::*;

/// Strategy that generates valid CSS identifiers directly.
///
/// Generates valid CSS identifier strings containing only alphanumeric characters,
/// hyphens, and underscores. The generated strings are non-empty and contain at
/// least one alphanumeric character (not just hyphens or underscores).
///
/// This matches the original quickcheck behavior which allowed any valid character
/// in any position, as long as at least one alphanumeric was present.
fn css_identifier_strategy() -> impl Strategy<Value = String> {
    // Strategy: Generate one guaranteed alphanumeric char, plus additional chars
    // that can be any valid CSS identifier char. Then shuffle/combine them.
    // This guarantees at least one alphanumeric without filtering.
    (
        // At least one alphanumeric character (guaranteed)
        prop::char::ranges(Cow::Owned(vec!['0'..='9', 'A'..='Z', 'a'..='z'])),
        // Rest can be any valid CSS identifier chars (0-99 more)
        prop::collection::vec(
            prop::char::ranges(Cow::Owned(vec![
                '0'..='9',
                'A'..='Z',
                'a'..='z',
                '-'..='-',
                '_'..='_',
            ])),
            0..99,
        ),
        // Position to insert the guaranteed alphanumeric (will be clamped to valid range)
        any::<prop::sample::Index>(),
    )
        .prop_map(|(guaranteed_alnum, mut rest, insert_pos)| {
            // Insert the guaranteed alphanumeric at a random position
            let pos = insert_pos.index(rest.len() + 1);
            rest.insert(pos, guaranteed_alnum);
            rest.into_iter().collect()
        })
}

/// Arbitrary CSS identifier string for property-based testing.
///
/// Generates valid CSS identifier strings containing only alphanumeric characters,
/// hyphens, and underscores. The generated strings are non-empty and contain at
/// least one alphanumeric character (not just hyphens or underscores).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CssIdentifierString(pub String);

impl Arbitrary for CssIdentifierString {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        css_identifier_strategy().prop_map(Self).boxed()
    }
}
