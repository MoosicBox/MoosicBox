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
    #[must_use]
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
    #[must_use]
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
}
