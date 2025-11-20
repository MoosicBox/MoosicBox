use sha2::Sha256;

/// Trait for types that can contribute to a checksum digest
pub trait Digest {
    /// Update the hasher with this value's contribution to the checksum
    ///
    /// This method feeds data into the SHA-256 hasher in a deterministic way,
    /// ensuring that the same value always produces the same checksum.
    fn update_digest(&self, hasher: &mut Sha256);
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest as _;

    // Test implementation of Digest for a simple type
    struct TestData {
        value: String,
    }

    impl Digest for TestData {
        fn update_digest(&self, hasher: &mut Sha256) {
            hasher.update(b"TEST:");
            hasher.update(self.value.as_bytes());
        }
    }

    #[test_log::test]
    fn test_digest_trait_custom_implementation() {
        let data = TestData {
            value: "hello".to_string(),
        };

        let mut hasher = Sha256::new();
        data.update_digest(&mut hasher);
        let result = hasher.finalize();

        // Verify we got a valid 32-byte SHA256 hash
        assert_eq!(result.len(), 32);

        // Verify the hash is not all zeros
        assert_ne!(&result[..], &[0u8; 32]);
    }

    #[test_log::test]
    fn test_digest_deterministic() {
        let data = TestData {
            value: "test_value".to_string(),
        };

        // Hash the same data twice
        let mut hasher1 = Sha256::new();
        data.update_digest(&mut hasher1);
        let result1 = hasher1.finalize();

        let mut hasher2 = Sha256::new();
        data.update_digest(&mut hasher2);
        let result2 = hasher2.finalize();

        // Results should be identical
        assert_eq!(result1, result2);
    }

    #[test_log::test]
    fn test_digest_different_data_different_hash() {
        let data1 = TestData {
            value: "value1".to_string(),
        };
        let data2 = TestData {
            value: "value2".to_string(),
        };

        let mut hasher1 = Sha256::new();
        data1.update_digest(&mut hasher1);
        let result1 = hasher1.finalize();

        let mut hasher2 = Sha256::new();
        data2.update_digest(&mut hasher2);
        let result2 = hasher2.finalize();

        // Different data should produce different hashes
        assert_ne!(result1, result2);
    }

    #[test_log::test]
    fn test_digest_multiple_updates() {
        let data1 = TestData {
            value: "part1".to_string(),
        };
        let data2 = TestData {
            value: "part2".to_string(),
        };

        // Hash with multiple updates
        let mut hasher = Sha256::new();
        data1.update_digest(&mut hasher);
        data2.update_digest(&mut hasher);
        let result = hasher.finalize();

        // Verify we got a valid hash
        assert_eq!(result.len(), 32);
        assert_ne!(&result[..], &[0u8; 32]);
    }

    #[test_log::test]
    fn test_digest_empty_string() {
        let data = TestData {
            value: String::new(),
        };

        let mut hasher = Sha256::new();
        data.update_digest(&mut hasher);
        let result = hasher.finalize();

        // Empty data should still produce a valid hash (hash of "TEST:" prefix)
        assert_eq!(result.len(), 32);
        assert_ne!(&result[..], &[0u8; 32]);
    }

    #[test_log::test]
    fn test_digest_order_matters() {
        let data1 = TestData {
            value: "abc".to_string(),
        };
        let data2 = TestData {
            value: "def".to_string(),
        };

        // Hash in one order
        let mut hasher1 = Sha256::new();
        data1.update_digest(&mut hasher1);
        data2.update_digest(&mut hasher1);
        let result1 = hasher1.finalize();

        // Hash in reverse order
        let mut hasher2 = Sha256::new();
        data2.update_digest(&mut hasher2);
        data1.update_digest(&mut hasher2);
        let result2 = hasher2.finalize();

        // Different order should produce different hashes
        assert_ne!(result1, result2);
    }
}
