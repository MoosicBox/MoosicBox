//! Arbitrary implementations for property-based testing.
//!
//! This module provides additional [`proptest::arbitrary::Arbitrary`] implementations
//! for types that require custom strategies beyond what `#[derive(Arbitrary)]` provides.
//!
//! Most types in this crate use `#[derive(test_strategy::Arbitrary)]` directly on their
//! definitions. This module only contains implementations for types that need special
//! handling, such as those with complex field dependencies.
//!
//! Available when the `arb` feature is enabled.

// Currently, all types use derive(Arbitrary) directly on their definitions.
// This module is kept for potential future custom implementations.
