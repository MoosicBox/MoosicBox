//! Test vectors for validating Opus decoder implementations.
//!
//! This crate provides a collection of Opus test vectors containing encoded Opus packets
//! and their expected decoded PCM output. These vectors are used to validate the correctness
//! of Opus decoder implementations by comparing actual decoder output against reference results.
//!
//! Test vectors include both SILK and CELT codec modes and are generated at build time.
//!
//! # Usage
//!
//! ```rust,no_run
//! use moosicbox_opus_native_test_vectors::{TestVector, test_vectors_dir, calculate_snr};
//!
//! # fn decode_opus_packet(packet: &[u8], sample_rate: u32, channels: u8) -> Vec<i16> {
//! #     // Placeholder decoder implementation
//! #     vec![0; 1920]
//! # }
//! // Load all test vectors
//! let vectors = TestVector::load_all(test_vectors_dir().join("silk"))?;
//!
//! // Validate decoder with each test vector
//! for vector in &vectors {
//!     let decoded = decode_opus_packet(&vector.packet, vector.sample_rate, vector.channels);
//!     let snr = calculate_snr(&vector.expected_pcm, &decoded);
//!     assert!(snr > 50.0, "SNR too low: {}", snr);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Main Types
//!
//! * [`TestVector`] - Contains an Opus packet and expected PCM output
//! * [`calculate_snr`] - Calculates Signal-to-Noise Ratio for quality measurement
//! * [`test_vectors_dir`] - Returns the path to generated test vectors
//! * [`vectors_available`] - Checks if test vectors are available

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::fs;
use std::path::{Path, PathBuf};

/// Test vector containing an Opus packet and expected decoded PCM output.
///
/// Used for validating Opus decoder implementations against reference outputs.
#[derive(Debug, Clone)]
pub struct TestVector {
    /// Name of the test vector.
    pub name: String,
    /// Encoded Opus packet data.
    pub packet: Vec<u8>,
    /// Expected PCM samples after decoding (16-bit signed integers).
    pub expected_pcm: Vec<i16>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u8,
}

impl TestVector {
    /// Loads a test vector from a directory containing the required test data files.
    ///
    /// Expects the directory to contain `packet.bin` (Opus packet), `expected.pcm` (decoded PCM),
    /// and `meta.json` (metadata with `sample_rate` and channels).
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// * Path does not exist or is not a directory
    /// * Required files (packet.bin, expected.pcm, meta.json) are missing
    /// * JSON metadata is malformed or missing required fields
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid filename")?
            .to_string();

        let packet_path = path.join("packet.bin");
        let pcm_path = path.join("expected.pcm");
        let meta_path = path.join("meta.json");

        let packet = fs::read(packet_path)?;
        let pcm_bytes = fs::read(pcm_path)?;
        let meta_str = fs::read_to_string(meta_path)?;

        let meta: serde_json::Value = serde_json::from_str(&meta_str)?;
        let sample_rate =
            u32::try_from(meta["sample_rate"].as_u64().ok_or("Missing sample_rate")?)?;
        let channels = u8::try_from(meta["channels"].as_u64().ok_or("Missing channels")?)?;

        let expected_pcm = pcm_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        Ok(Self {
            name,
            packet,
            expected_pcm,
            sample_rate,
            channels,
        })
    }

    /// Loads all test vectors from subdirectories within the specified directory.
    ///
    /// Scans the directory for subdirectories and attempts to load a test vector from each.
    /// Subdirectories that don't contain valid test vector data are silently skipped.
    ///
    /// # Errors
    ///
    /// Returns error if directory cannot be read or accessed
    pub fn load_all(dir: impl AsRef<Path>) -> Result<Vec<Self>, Box<dyn std::error::Error>> {
        let dir = dir.as_ref();
        let mut vectors = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Ok(vector) = Self::load(entry.path())
            {
                vectors.push(vector);
            }
        }

        Ok(vectors)
    }
}

/// Calculates Signal-to-Noise Ratio (SNR) in decibels between reference and decoded signals.
///
/// SNR measures the quality of the decoded signal compared to the reference.
/// Higher SNR values indicate better quality (less noise/distortion).
///
/// # Special Return Values
///
/// * `f64::INFINITY` - Signals are identical (no noise)
/// * `f64::NEG_INFINITY` - Signals have different lengths
/// * `0.0` - Reference signal has negligible power
#[must_use]
pub fn calculate_snr(reference: &[i16], decoded: &[i16]) -> f64 {
    if reference.len() != decoded.len() {
        return f64::NEG_INFINITY;
    }

    let mut signal_power = 0.0;
    let mut noise_power = 0.0;

    for (ref_sample, dec_sample) in reference.iter().zip(decoded.iter()) {
        let ref_f = f64::from(*ref_sample);
        let dec_f = f64::from(*dec_sample);
        let error = ref_f - dec_f;

        signal_power += ref_f * ref_f;
        noise_power += error * error;
    }

    if noise_power < 1e-10 {
        return f64::INFINITY;
    }

    if signal_power < 1e-10 {
        return 0.0;
    }

    10.0 * (signal_power / noise_power).log10()
}

/// Returns the path to the directory containing generated test vectors.
///
/// Test vectors are generated at build time and stored in the build output directory.
#[must_use]
pub fn test_vectors_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("generated")
}

/// Returns whether test vectors are available.
///
/// Checks if the test vectors directory exists and contains either SILK or CELT test data.
#[must_use]
pub fn vectors_available() -> bool {
    let dir = test_vectors_dir();
    dir.exists() && (dir.join("silk").exists() || dir.join("celt").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snr_identical_signals() {
        let signal = vec![100, -200, 300, -400];
        let snr = calculate_snr(&signal, &signal);
        assert!(snr.is_infinite());
    }

    #[test]
    fn test_snr_different_lengths() {
        let signal1 = vec![100, -200];
        let signal2 = vec![100];
        let snr = calculate_snr(&signal1, &signal2);
        assert!(snr.is_infinite() && snr.is_sign_negative());
    }

    #[test]
    fn test_snr_calculation() {
        let reference = vec![1000, 2000, 3000];
        let decoded = vec![1010, 1990, 3005];
        let snr = calculate_snr(&reference, &decoded);
        assert!(snr > 40.0);
    }

    #[test]
    fn test_snr_zero_reference_signal() {
        let reference = vec![0, 0, 0, 0];
        let decoded = vec![100, -50, 200, -100];
        let snr = calculate_snr(&reference, &decoded);
        assert!(
            (snr - 0.0).abs() < f64::EPSILON,
            "SNR should be 0.0 for zero-power reference"
        );
    }

    #[test]
    fn test_snr_empty_signals() {
        let reference: Vec<i16> = vec![];
        let decoded: Vec<i16> = vec![];
        let snr = calculate_snr(&reference, &decoded);
        // Empty signals have no power difference
        assert!(
            snr.is_infinite(),
            "Empty signals should result in infinite SNR"
        );
    }

    #[test]
    fn test_snr_near_zero_noise() {
        let reference = vec![1000, 2000, 3000];
        let decoded = vec![1001, 2001, 3001];
        let snr = calculate_snr(&reference, &decoded);
        // Very small noise should result in very high SNR
        assert!(snr > 60.0, "SNR should be very high for minimal noise");
    }

    #[test]
    fn test_load_missing_directory() {
        let result = TestVector::load("/nonexistent/path/to/vector");
        assert!(result.is_err(), "Loading from nonexistent path should fail");
    }

    #[test]
    fn test_load_file_instead_of_directory() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let result = TestVector::load(temp_file.path());
        assert!(
            result.is_err(),
            "Loading from file instead of directory should fail"
        );
    }

    #[test]
    fn test_load_missing_packet_bin() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        // Create expected.pcm and meta.json but not packet.bin
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01\x00").unwrap();
        fs::write(
            vector_dir.join("meta.json"),
            r#"{"sample_rate": 48000, "channels": 1}"#,
        )
        .unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(result.is_err(), "Loading without packet.bin should fail");
    }

    #[test]
    fn test_load_missing_expected_pcm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        // Create packet.bin and meta.json but not expected.pcm
        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(
            vector_dir.join("meta.json"),
            r#"{"sample_rate": 48000, "channels": 1}"#,
        )
        .unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(result.is_err(), "Loading without expected.pcm should fail");
    }

    #[test]
    fn test_load_missing_meta_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        // Create packet.bin and expected.pcm but not meta.json
        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01\x00").unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(result.is_err(), "Loading without meta.json should fail");
    }

    #[test]
    fn test_load_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01\x00").unwrap();
        fs::write(vector_dir.join("meta.json"), "not valid json {{{").unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(result.is_err(), "Loading with invalid JSON should fail");
    }

    #[test]
    fn test_load_missing_sample_rate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01\x00").unwrap();
        fs::write(vector_dir.join("meta.json"), r#"{"channels": 1}"#).unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(
            result.is_err(),
            "Loading without sample_rate in meta should fail"
        );
    }

    #[test]
    fn test_load_missing_channels() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01\x00").unwrap();
        fs::write(vector_dir.join("meta.json"), r#"{"sample_rate": 48000}"#).unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(
            result.is_err(),
            "Loading without channels in meta should fail"
        );
    }

    #[test]
    fn test_load_odd_pcm_bytes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        fs::write(vector_dir.join("packet.bin"), b"\x00\x01\x02\x03").unwrap();
        // Odd number of bytes (3 bytes instead of 2 or 4)
        fs::write(vector_dir.join("expected.pcm"), b"\x00\x00\x01").unwrap();
        fs::write(
            vector_dir.join("meta.json"),
            r#"{"sample_rate": 48000, "channels": 1}"#,
        )
        .unwrap();

        let result = TestVector::load(&vector_dir);
        // Should succeed but last byte is dropped by chunks_exact
        assert!(result.is_ok(), "Loading with odd PCM bytes should succeed");
        let vector = result.unwrap();
        assert_eq!(
            vector.expected_pcm.len(),
            1,
            "Should only parse complete 2-byte chunks"
        );
    }

    #[test]
    fn test_load_valid_vector() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vector_dir = temp_dir.path().join("test_vector");
        fs::create_dir(&vector_dir).unwrap();

        // Create valid test vector files
        let packet_data = vec![0xFC, 0x12, 0x34, 0x56];
        fs::write(vector_dir.join("packet.bin"), &packet_data).unwrap();

        // Create PCM data: two 16-bit samples (little-endian)
        let pcm_data = vec![0x00, 0x10, 0xFF, 0x0F]; // [4096, 4095]
        fs::write(vector_dir.join("expected.pcm"), &pcm_data).unwrap();

        fs::write(
            vector_dir.join("meta.json"),
            r#"{"sample_rate": 48000, "channels": 2}"#,
        )
        .unwrap();

        let result = TestVector::load(&vector_dir);
        assert!(result.is_ok(), "Loading valid vector should succeed");

        let vector = result.unwrap();
        assert_eq!(vector.name, "test_vector");
        assert_eq!(vector.packet, packet_data);
        assert_eq!(vector.expected_pcm, vec![4096, 4095]);
        assert_eq!(vector.sample_rate, 48000);
        assert_eq!(vector.channels, 2);
    }

    #[test]
    fn test_load_all_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = TestVector::load_all(temp_dir.path());
        assert!(result.is_ok(), "load_all on empty directory should succeed");
        assert_eq!(
            result.unwrap().len(),
            0,
            "Empty directory should return no vectors"
        );
    }

    #[test]
    fn test_load_all_nonexistent_directory() {
        let result = TestVector::load_all("/nonexistent/path");
        assert!(
            result.is_err(),
            "load_all on nonexistent directory should fail"
        );
    }

    #[test]
    fn test_load_all_with_valid_vectors() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create two valid test vectors
        for i in 1..=2 {
            let vector_dir = temp_dir.path().join(format!("vector_{i}"));
            fs::create_dir(&vector_dir).unwrap();

            fs::write(vector_dir.join("packet.bin"), [0x00, 0x01, 0x02]).unwrap();
            fs::write(vector_dir.join("expected.pcm"), [0x00, 0x00, 0x01, 0x00]).unwrap();
            fs::write(
                vector_dir.join("meta.json"),
                format!(r#"{{"sample_rate": 48000, "channels": {i}}}"#),
            )
            .unwrap();
        }

        let result = TestVector::load_all(temp_dir.path());
        assert!(result.is_ok(), "load_all with valid vectors should succeed");

        let vectors = result.unwrap();
        assert_eq!(vectors.len(), 2, "Should load both vectors");
        assert!(vectors.iter().any(|v| v.name == "vector_1"));
        assert!(vectors.iter().any(|v| v.name == "vector_2"));
    }

    #[test]
    fn test_load_all_skips_invalid_subdirectories() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create one valid vector
        let valid_dir = temp_dir.path().join("valid_vector");
        fs::create_dir(&valid_dir).unwrap();
        fs::write(valid_dir.join("packet.bin"), [0x00, 0x01]).unwrap();
        fs::write(valid_dir.join("expected.pcm"), [0x00, 0x00]).unwrap();
        fs::write(
            valid_dir.join("meta.json"),
            r#"{"sample_rate": 48000, "channels": 1}"#,
        )
        .unwrap();

        // Create an invalid vector (missing files)
        let invalid_dir = temp_dir.path().join("invalid_vector");
        fs::create_dir(&invalid_dir).unwrap();

        // Create a regular file (not a directory)
        fs::write(temp_dir.path().join("not_a_dir.txt"), b"test").unwrap();

        let result = TestVector::load_all(temp_dir.path());
        assert!(
            result.is_ok(),
            "load_all should succeed even with invalid subdirectories"
        );

        let vectors = result.unwrap();
        assert_eq!(
            vectors.len(),
            1,
            "Should only load valid vector, skipping invalid ones"
        );
        assert_eq!(vectors[0].name, "valid_vector");
    }

    #[test]
    fn test_test_vectors_dir_returns_path() {
        let dir = test_vectors_dir();
        assert!(
            dir.ends_with("generated"),
            "test_vectors_dir should end with 'generated'"
        );
    }

    #[test]
    fn test_vectors_available_checks_existence() {
        // This test depends on build-time vector generation
        // We just verify the function runs without panicking
        let _available = vectors_available();
        // The actual value depends on whether vectors were generated
        // We can't reliably assert true or false here
    }
}
