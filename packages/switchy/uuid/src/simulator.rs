//! Deterministic UUID generation for testing and simulation.
//!
//! This module provides UUID v4 generation from the current simulation run's
//! `switchy_uuid` RNG stream (see [`switchy_simulation::stream`]). The stream is derived from
//! the run's seed (`SIMULATOR_SEED`, or whatever seed the harness gives the run), so:
//!
//! * the same run seed yields the same UUID sequence,
//! * every run restarts the sequence (setting the seed or clearing the context resets it),
//! * parallel runs, each with its own [`switchy_simulation::Context`], never share or perturb
//!   each other's sequences, and
//! * drawing UUIDs never shifts the draws of other simulators (time, random, threads).
//!
//! Earlier releases used a process-global generator seeded by `SIMULATOR_UUID_SEED`; that
//! variable is no longer read. Choose a different run seed to get a different sequence.

use std::sync::PoisonError;

use crate::Uuid;

/// The name of the run's RNG stream UUIDs are drawn from.
pub const STREAM: &str = "switchy_uuid";

/// Generates a deterministic UUID v4 for simulation purposes.
///
/// The bytes come from the current simulation run's [`STREAM`] stream, so the sequence is
/// reproducible for a given run seed and independent between runs.
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
    switchy_simulation::stream(STREAM)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .fill_bytes(&mut bytes);

    Uuid::from_random_bytes(bytes)
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

    use std::sync::Arc;

    use switchy_simulation::{Context, with_context};

    fn draw(context: &Arc<Context>, count: usize) -> Vec<Uuid> {
        with_context(Arc::clone(context), || {
            (0..count).map(|_| new_v4()).collect()
        })
    }

    #[test_log::test]
    fn test_deterministic_generation() {
        let uuids = draw(&Context::new(7), 3);

        // UUIDs should be different from each other
        assert_ne!(uuids[0], uuids[1]);
        assert_ne!(uuids[1], uuids[2]);
        assert_ne!(uuids[0], uuids[2]);
    }

    #[test]
    fn historical_seed_42() {
        // Captured from the pre-migration implementation at 57cf1627f42d.
        let sequence: Vec<_> = draw(&Context::new(42), 3)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            sequence,
            [
                "d379557a-e383-49bb-be17-0d6206ce7c98",
                "e09072b2-7722-43b8-b09e-b88e0867e335",
                "e52850ba-3c02-41e6-8689-f619681fba9a"
            ]
        );
    }

    #[test_log::test]
    fn same_run_seed_yields_same_sequence() {
        assert_eq!(draw(&Context::new(42), 8), draw(&Context::new(42), 8));
    }

    #[test_log::test]
    fn different_run_seeds_yield_different_sequences() {
        assert_ne!(draw(&Context::new(1), 4), draw(&Context::new(2), 4));
    }

    #[test_log::test]
    fn sequence_restarts_when_the_run_is_reseeded() {
        let context = Context::new(5);
        let first = draw(&context, 4);
        assert_ne!(draw(&context, 4), first, "the stream advances within a run");
        context.set_seed(5);
        assert_eq!(draw(&context, 4), first);
        let _ = draw(&context, 2);
        context.clear();
        assert_eq!(draw(&context, 4), first);
    }

    #[test_log::test]
    fn runs_are_isolated_from_each_other() {
        let a = Context::new(9);
        let b = Context::new(9);
        let expected = draw(&Context::new(9), 6);
        // Interleave draws of two runs with the same seed: neither perturbs the other.
        let mut from_a = Vec::new();
        let mut from_b = Vec::new();
        for _ in 0..6 {
            from_a.extend(draw(&a, 1));
            from_b.extend(draw(&b, 1));
        }
        assert_eq!(from_a, expected);
        assert_eq!(from_b, expected);
    }

    #[test_log::test]
    fn parallel_runs_on_threads_are_isolated() {
        let expected = draw(&Context::new(11), 20);
        let results: Vec<Vec<Uuid>> = std::thread::scope(|s| {
            // Start every thread before joining any, so the runs overlap.
            let handles: [_; 4] = std::array::from_fn(|_| s.spawn(|| draw(&Context::new(11), 20)));
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for result in results {
            assert_eq!(result, expected);
        }
    }

    #[test_log::test]
    fn uuid_draws_do_not_shift_other_streams() {
        let a = Context::new(3);
        let b = Context::new(3);
        let _ = draw(&a, 5);
        let x = a.stream("other").lock().unwrap().next_u64();
        let y = b.stream("other").lock().unwrap().next_u64();
        assert_eq!(x, y);
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

    #[test_log::test]
    fn test_concurrent_uuid_generation_produces_unique_uuids() {
        {
            use std::collections::BTreeSet;

            // Generate UUIDs from multiple threads concurrently and verify uniqueness
            let num_threads = 4;
            let uuids_per_thread = 50;

            let all_uuids: Arc<std::sync::Mutex<BTreeSet<Uuid>>> =
                Arc::new(std::sync::Mutex::new(BTreeSet::new()));

            // Threads of one run share its context (as simulated threads do), so they draw
            // from one stream and never repeat each other.
            let context = Context::new(13);
            std::thread::scope(|s| {
                for _ in 0..num_threads {
                    let uuids_clone = Arc::clone(&all_uuids);
                    let context = Arc::clone(&context);
                    s.spawn(move || {
                        switchy_simulation::set_current(Some(context));
                        for _ in 0..uuids_per_thread {
                            let uuid = new_v4();
                            let was_inserted = uuids_clone.lock().unwrap().insert(uuid);
                            assert!(
                                was_inserted,
                                "Concurrent UUID generation produced duplicate: {uuid}"
                            );
                        }
                    });
                }
            });

            let final_count = all_uuids.lock().unwrap().len();
            assert_eq!(
                final_count,
                num_threads * uuids_per_thread,
                "Expected {} unique UUIDs from concurrent generation, got {}",
                num_threads * uuids_per_thread,
                final_count
            );
        }
    }

    #[test_log::test]
    fn test_all_uuid_bytes_are_populated() {
        {
            use std::collections::BTreeSet;

            // Verify that all 16 bytes of the generated UUID contain meaningful data
            // from the RNG, not just the version/variant bytes.
            // Generate multiple UUIDs and collect unique values seen at each byte position.
            let mut byte_variations: [BTreeSet<u8>; 16] = Default::default();

            for _ in 0..100 {
                let uuid = new_v4();
                let bytes = uuid.as_bytes();
                for (i, &byte) in bytes.iter().enumerate() {
                    byte_variations[i].insert(byte);
                }
            }

            // Each byte position should show variation across 100 UUIDs.
            // Bytes 6 and 8 have some bits fixed, but should still show variation
            // in their non-fixed bits.
            for (i, variations) in byte_variations.iter().enumerate() {
                assert!(
                    variations.len() >= 2,
                    "Byte position {i} shows no variation across 100 UUIDs: {variations:?}"
                );
            }
        }
    }
}
