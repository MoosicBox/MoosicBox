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
