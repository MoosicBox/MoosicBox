//! Property testing support via proptest.
//!
//! This module provides an [`Arbitrary`] implementation for [`Color`],
//! enabling property-based testing with the [`proptest`] crate.

use proptest::prelude::*;

use crate::Color;

/// Implementation of [`Arbitrary`] for [`Color`] to support property-based testing.
///
/// Generates random colors with arbitrary RGB values and optional alpha channel.
impl Arbitrary for Color {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (any::<u8>(), any::<u8>(), any::<u8>(), any::<Option<u8>>())
            .prop_map(|(r, g, b, a)| Self { r, g, b, a })
            .boxed()
    }
}
