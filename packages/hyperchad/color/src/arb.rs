//! Property testing support via QuickCheck.
//!
//! This module provides an [`Arbitrary`] implementation for [`Color`],
//! enabling property-based testing with the [`quickcheck`] crate.

use quickcheck::{Arbitrary, Gen};

use crate::Color;

impl Arbitrary for Color {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            r: u8::arbitrary(g),
            g: u8::arbitrary(g),
            b: u8::arbitrary(g),
            a: Option::arbitrary(g),
        }
    }
}
