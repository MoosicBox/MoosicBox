//! Random UUID generation using the standard `uuid` crate.
//!
//! This module provides truly random UUID v4 generation suitable for production use.

use uuid::Uuid;

/// Generate a new random UUID v4
#[must_use]
pub fn new_v4() -> Uuid {
    Uuid::new_v4()
}

/// Generate a new random UUID v4 as a string
#[must_use]
pub fn new_v4_string() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_uuid_v4_format_compliance() {
        let uuid = new_v4();
        let bytes = uuid.as_bytes();

        // Verify version 4 (bits 12-15 of time_hi_and_version should be 0100)
        let version_byte = bytes[6];
        assert_eq!(
            version_byte & 0xf0,
            0x40,
            "UUID version bits should be 0100 (v4)"
        );

        // Verify variant (bits 6-7 of clock_seq_hi_and_reserved should be 10)
        let variant_byte = bytes[8];
        assert_eq!(
            variant_byte & 0xc0,
            0x80,
            "UUID variant bits should be 10 (RFC 4122)"
        );
    }

    #[test_log::test]
    fn test_string_format() {
        let uuid_string = new_v4_string();

        // Should be 36 characters in 8-4-4-4-12 format
        assert_eq!(uuid_string.len(), 36);

        // Verify hyphen positions
        assert_eq!(uuid_string.chars().nth(8).unwrap(), '-');
        assert_eq!(uuid_string.chars().nth(13).unwrap(), '-');
        assert_eq!(uuid_string.chars().nth(18).unwrap(), '-');
        assert_eq!(uuid_string.chars().nth(23).unwrap(), '-');

        // Should be parseable back to UUID
        let parsed = Uuid::parse_str(&uuid_string);
        assert!(
            parsed.is_ok(),
            "new_v4_string should produce valid UUID string"
        );
    }

    #[test_log::test]
    fn test_randomness() {
        // Generate multiple UUIDs and verify they're all unique
        let mut uuids = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let uuid = new_v4();
            assert!(
                uuids.insert(uuid),
                "Generated duplicate UUID (extremely unlikely with random generation): {uuid}"
            );
        }
        assert_eq!(uuids.len(), 100);
    }

    #[test_log::test]
    fn test_consistency_between_methods() {
        let uuid = new_v4();
        let direct_string = uuid.to_string();
        let helper_string = new_v4_string();

        // Both should produce strings of the same length and format
        assert_eq!(direct_string.len(), helper_string.len());
        assert_eq!(direct_string.len(), 36);
    }
}
