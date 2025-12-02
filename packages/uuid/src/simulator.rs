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

/// Generates a deterministic UUID v4 for simulation purposes.
///
/// This function uses a seeded random number generator to produce UUIDs that
/// are reproducible across runs with the same seed. The seed can be configured
/// via the `SIMULATOR_UUID_SEED` environment variable (defaults to 12345).
///
/// The generated UUID is compliant with RFC 4122 version 4 format, with the
/// version and variant bits correctly set.
///
/// # Examples
///
/// ```
/// let uuid = switchy_uuid::simulator::new_v4();
/// assert_eq!(uuid.get_version_num(), 4);
/// ```
#[must_use]
pub fn new_v4() -> Uuid {
    let mut bytes = [0u8; 16];
    RNG.fill_bytes(&mut bytes);

    // Set version (4) and variant bits according to RFC 4122
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 10

    Uuid::from_bytes(bytes)
}

/// Generates a deterministic UUID v4 as a hyphenated string.
///
/// This is a convenience function that generates a UUID using [`new_v4`] and
/// converts it to the standard hyphenated string format (8-4-4-4-12).
///
/// # Examples
///
/// ```
/// let uuid_string = switchy_uuid::simulator::new_v4_string();
/// assert_eq!(uuid_string.len(), 36);
/// assert!(uuid_string.chars().filter(|c| *c == '-').count() == 4);
/// ```
#[must_use]
pub fn new_v4_string() -> String {
    new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
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

    #[test_log::test]
    fn test_multiple_uuids_are_unique() {
        // Generate multiple UUIDs and verify they're all unique
        let mut uuids = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let uuid = new_v4();
            assert!(uuids.insert(uuid), "Generated duplicate UUID: {uuid}");
        }
        assert_eq!(uuids.len(), 100);
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_version_variant_bits_consistently_set_across_many_uuids() {
        // Verify that the bit masking logic correctly sets version/variant bits
        // across many UUIDs, not just one. This tests that the masking operations
        // are consistently applied regardless of the random input.
        for i in 0..100 {
            let uuid = new_v4();
            let bytes = uuid.as_bytes();

            // Version 4: upper nibble of byte[6] must be 0100
            assert_eq!(
                bytes[6] & 0xf0,
                0x40,
                "UUID #{i} version bits incorrect: got {:02x}, expected 0x4x",
                bytes[6]
            );

            // RFC 4122 variant: upper 2 bits of byte[8] must be 10
            assert_eq!(
                bytes[8] & 0xc0,
                0x80,
                "UUID #{i} variant bits incorrect: got {:02x}, expected 0x8x-0xbx",
                bytes[8]
            );
        }
    }

    #[test_log::test]
    fn test_random_bits_preserved_in_version_variant_bytes() {
        // Verify that the masking operations preserve the random bits
        // in bytes[6] and bytes[8]. The lower nibble of bytes[6] (4 bits)
        // and lower 6 bits of bytes[8] should vary across generated UUIDs.
        let mut byte6_lower_nibbles = std::collections::BTreeSet::new();
        let mut byte8_lower_bits = std::collections::BTreeSet::new();

        // Generate enough UUIDs to see variation in the preserved bits
        for _ in 0..50 {
            let uuid = new_v4();
            let bytes = uuid.as_bytes();

            byte6_lower_nibbles.insert(bytes[6] & 0x0f);
            byte8_lower_bits.insert(bytes[8] & 0x3f);
        }

        // With 50 UUIDs, we should see significant variation in the preserved bits.
        // Lower nibble of byte[6] has 4 bits (16 possible values)
        // Lower 6 bits of byte[8] has 6 bits (64 possible values)
        // Even with a deterministic RNG, we expect good distribution.
        assert!(
            byte6_lower_nibbles.len() >= 4,
            "Expected variation in byte[6] lower nibble, but only saw {} unique values: {:?}",
            byte6_lower_nibbles.len(),
            byte6_lower_nibbles
        );
        assert!(
            byte8_lower_bits.len() >= 8,
            "Expected variation in byte[8] lower bits, but only saw {} unique values: {:?}",
            byte8_lower_bits.len(),
            byte8_lower_bits
        );
    }
}
