//! Deterministic UUID generation for testing and simulation.
//!
//! This module provides UUID v4 generation using a seeded random number generator,
//! allowing for reproducible UUIDs in test and simulation environments.
//!
//! The seed can be configured via the `SIMULATOR_UUID_SEED` environment variable.
//! If not set, defaults to 12345.

use switchy_env::var_parse_or;
use switchy_random::{GenericRng, Rng};
use uuid::Uuid;

static RNG: std::sync::LazyLock<Rng> = std::sync::LazyLock::new(|| {
    let seed = var_parse_or("SIMULATOR_UUID_SEED", 12345u64);

    log::debug!("Using UUID seed: {seed}");
    Rng::from_seed(seed)
});

/// Generate a deterministic UUID v4 for simulation
#[must_use]
pub fn new_v4() -> Uuid {
    let mut bytes = [0u8; 16];
    RNG.fill_bytes(&mut bytes);

    // Set version (4) and variant bits according to RFC 4122
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 10

    Uuid::from_bytes(bytes)
}

/// Generate a deterministic UUID v4 as a string for simulation
#[must_use]
pub fn new_v4_string() -> String {
    new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_generation() {
        // The RNG is static, so we can't easily reset it between tests.
        // Instead, we verify that consecutive calls produce different UUIDs
        // but the sequence is deterministic within a single test run.
        let uuid1 = new_v4();
        let uuid2 = new_v4();
        let uuid3 = new_v4();

        // UUIDs should be different from each other
        assert_ne!(uuid1, uuid2);
        assert_ne!(uuid2, uuid3);
        assert_ne!(uuid1, uuid3);
    }

    #[test]
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

    #[test]
    fn test_string_conversion_consistency() {
        let uuid = new_v4();
        let string_from_uuid = uuid.to_string();
        let uuid_from_string = new_v4_string();

        // Both should produce valid UUID strings with the same format
        assert_eq!(string_from_uuid.len(), 36);
        assert_eq!(uuid_from_string.len(), 36);

        // Verify hyphen positions (8-4-4-4-12 format)
        assert_eq!(string_from_uuid.chars().nth(8).unwrap(), '-');
        assert_eq!(string_from_uuid.chars().nth(13).unwrap(), '-');
        assert_eq!(string_from_uuid.chars().nth(18).unwrap(), '-');
        assert_eq!(string_from_uuid.chars().nth(23).unwrap(), '-');
    }

    #[test]
    fn test_multiple_uuids_are_unique() {
        // Generate multiple UUIDs and verify they're all unique
        let mut uuids = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let uuid = new_v4();
            assert!(uuids.insert(uuid), "Generated duplicate UUID: {uuid}");
        }
        assert_eq!(uuids.len(), 100);
    }

    #[test]
    fn test_new_v4_string_produces_valid_uuid() {
        let uuid_string = new_v4_string();

        // Should be parseable as a UUID
        let parsed = Uuid::parse_str(&uuid_string);
        assert!(
            parsed.is_ok(),
            "new_v4_string should produce valid UUID string"
        );

        // Verify it's a v4 UUID
        let uuid = parsed.unwrap();
        let bytes = uuid.as_bytes();
        assert_eq!(bytes[6] & 0xf0, 0x40, "Parsed UUID should be version 4");
        assert_eq!(
            bytes[8] & 0xc0,
            0x80,
            "Parsed UUID should have RFC 4122 variant"
        );
    }
}
