//! JSON-compatible value generators for property-based testing.
//!
//! Provides [`Arbitrary`] implementations for generating JSON values and JSON-safe
//! floating-point numbers (finite `f32` and `f64` values).

use quickcheck::{Arbitrary, Gen};
use serde_json::Value;

use crate::xml::XmlString;

/// Arbitrary JSON value for property-based testing.
///
/// Currently generates JSON string values using XML-compatible strings
/// to ensure broad compatibility in testing scenarios.
#[derive(Clone, Debug)]
pub struct JsonValue(pub Value);

impl Arbitrary for JsonValue {
    fn arbitrary(g: &mut Gen) -> Self {
        Self(Value::String(XmlString::arbitrary(g).0))
    }
}

/// Arbitrary finite `f64` for JSON serialization in property-based testing.
///
/// Generates only finite floating-point values (excludes NaN and infinity),
/// as these are the only values valid in JSON.
#[derive(Clone, Debug)]
pub struct JsonF64(pub f64);

impl Arbitrary for JsonF64 {
    fn arbitrary(g: &mut Gen) -> Self {
        loop {
            let num = f64::arbitrary(g);

            if !num.is_finite() {
                continue;
            }

            return Self(num);
        }
    }
}

/// Arbitrary finite `f32` for JSON serialization in property-based testing.
///
/// Generates only finite floating-point values (excludes NaN and infinity),
/// as these are the only values valid in JSON.
#[derive(Clone, Debug)]
pub struct JsonF32(pub f32);

impl Arbitrary for JsonF32 {
    fn arbitrary(g: &mut Gen) -> Self {
        loop {
            let num = f32::arbitrary(g);

            if !num.is_finite() {
                continue;
            }

            return Self(num);
        }
    }
}
