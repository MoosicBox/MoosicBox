//! Serde's human-readable UUID contract uses text; its binary contract uses bytes.
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::Uuid;

impl Serialize for Uuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_bytes(self.as_bytes())
        }
    }
}

struct Bytes;

impl de::Visitor<'_> for Bytes {
    type Value = Uuid;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("16 UUID bytes")
    }

    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Uuid, E> {
        Uuid::from_slice(bytes).map_err(E::custom)
    }
}

struct Readable;

impl<'de> de::Visitor<'de> for Readable {
    type Value = Uuid;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a UUID string")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Uuid, E> {
        Uuid::parse_str(text).map_err(E::custom)
    }

    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Uuid, E> {
        Bytes.visit_bytes(bytes)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Uuid, A::Error> {
        let mut bytes = [0; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(i, &Bytes))?;
        }
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(17, &Bytes));
        }
        Ok(Uuid::from_bytes(bytes))
    }
}

impl<'de> Deserialize<'de> for Uuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(Readable)
        } else {
            deserializer.deserialize_bytes(Bytes)
        }
    }
}
