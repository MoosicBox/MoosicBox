use quickcheck::{Arbitrary, Gen};
use serde_json::Value;

use crate::xml::XmlString;

#[derive(Clone, Debug)]
pub struct JsonValue(pub Value);

impl Arbitrary for JsonValue {
    fn arbitrary(g: &mut Gen) -> Self {
        Self(Value::String(XmlString::arbitrary(g).0))
    }
}

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
