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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_json_f64_always_finite() {
        // Critical constraint: JsonF64 should never generate NaN or infinity
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f64 = JsonF64::arbitrary(&mut g);
            assert!(
                json_f64.0.is_finite(),
                "JsonF64 generated non-finite value: {}",
                json_f64.0
            );
        }
    }

    #[test_log::test]
    fn test_json_f64_not_nan() {
        // Verify that JsonF64 never generates NaN
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f64 = JsonF64::arbitrary(&mut g);
            assert!(!json_f64.0.is_nan(), "JsonF64 generated NaN");
        }
    }

    #[test_log::test]
    fn test_json_f64_not_infinite() {
        // Verify that JsonF64 never generates positive or negative infinity
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f64 = JsonF64::arbitrary(&mut g);
            assert!(
                !json_f64.0.is_infinite(),
                "JsonF64 generated infinity: {}",
                json_f64.0
            );
        }
    }

    #[test_log::test]
    fn test_json_f32_always_finite() {
        // Critical constraint: JsonF32 should never generate NaN or infinity
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f32 = JsonF32::arbitrary(&mut g);
            assert!(
                json_f32.0.is_finite(),
                "JsonF32 generated non-finite value: {}",
                json_f32.0
            );
        }
    }

    #[test_log::test]
    fn test_json_f32_not_nan() {
        // Verify that JsonF32 never generates NaN
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f32 = JsonF32::arbitrary(&mut g);
            assert!(!json_f32.0.is_nan(), "JsonF32 generated NaN");
        }
    }

    #[test_log::test]
    fn test_json_f32_not_infinite() {
        // Verify that JsonF32 never generates positive or negative infinity
        let mut g = Gen::new(100);
        for _ in 0..50 {
            let json_f32 = JsonF32::arbitrary(&mut g);
            assert!(
                !json_f32.0.is_infinite(),
                "JsonF32 generated infinity: {}",
                json_f32.0
            );
        }
    }

    #[test_log::test]
    fn test_json_value_is_string() {
        // Verify that JsonValue currently generates string values
        let mut g = Gen::new(100);
        for _ in 0..20 {
            let json_value = JsonValue::arbitrary(&mut g);
            assert!(
                json_value.0.is_string(),
                "JsonValue generated non-string value: {:?}",
                json_value.0
            );
        }
    }
}
