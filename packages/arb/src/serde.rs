//! JSON-compatible value generators for property-based testing.
//!
//! Provides [`proptest::arbitrary::Arbitrary`] implementations for generating JSON values
//! and JSON-safe floating-point numbers (finite `f32` and `f64` values).

use proptest::prelude::*;
use serde_json::Value;

use crate::xml::XmlString;

/// Arbitrary JSON value for property-based testing.
///
/// Currently generates JSON string values using XML-compatible strings
/// to ensure broad compatibility in testing scenarios.
#[derive(Clone, Debug)]
pub struct JsonValue(pub Value);

impl Arbitrary for JsonValue {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        any::<XmlString>()
            .prop_map(|s| Self(Value::String(s.0)))
            .boxed()
    }
}

/// Strategy that generates finite f64 values directly.
fn finite_f64_strategy() -> impl Strategy<Value = f64> {
    // Generate f64 values and filter for finite ones
    // The filter rate for f64 is very low (NaN/Inf are rare in random generation)
    any::<f64>().prop_filter("must be finite", |x| x.is_finite())
}

/// Arbitrary finite `f64` for JSON serialization in property-based testing.
///
/// Generates only finite floating-point values (excludes NaN and infinity),
/// as these are the only values valid in JSON.
#[derive(Clone, Debug)]
pub struct JsonF64(pub f64);

impl Arbitrary for JsonF64 {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        finite_f64_strategy().prop_map(Self).boxed()
    }
}

/// Strategy that generates finite f32 values directly.
fn finite_f32_strategy() -> impl Strategy<Value = f32> {
    // Generate f32 values and filter for finite ones
    // The filter rate for f32 is very low (NaN/Inf are rare in random generation)
    any::<f32>().prop_filter("must be finite", |x| x.is_finite())
}

/// Arbitrary finite `f32` for JSON serialization in property-based testing.
///
/// Generates only finite floating-point values (excludes NaN and infinity),
/// as these are the only values valid in JSON.
#[derive(Clone, Debug)]
pub struct JsonF32(pub f32);

impl Arbitrary for JsonF32 {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        finite_f32_strategy().prop_map(Self).boxed()
    }
}
