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

mod format;
pub use format::{REFERENCE_CONFIG, REFERENCE_REVISION, checksum, validate_toc};

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
    /// * PCM data cannot be read or metadata values cannot be converted to expected numeric types
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use moosicbox_opus_native_test_vectors::TestVector;
    ///
    /// let vector = TestVector::load("/path/to/vector")?;
    /// assert!(!vector.packet.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

        if pcm_bytes.len() % 2 != 0 {
            return Err("Odd PCM byte length".into());
        }
        if !meta["format_version"].is_null() && meta["format_version"] != 2 {
            return Err("Unsupported packet format version".into());
        }
        if meta["format_version"] == 2 {
            let frame_samples = usize::try_from(
                meta["frame_samples"]
                    .as_u64()
                    .ok_or("Missing frame_samples")?,
            )?;
            let expected_bytes = frame_samples
                .checked_mul(usize::from(channels))
                .and_then(|n| n.checked_mul(2))
                .ok_or("PCM size overflow")?;
            if expected_bytes == 0 || pcm_bytes.len() != expected_bytes {
                return Err("PCM length mismatch".into());
            }
            let duration = meta["frame_size_ms"]
                .as_u64()
                .ok_or("Missing frame_size_ms")?;
            if u64::try_from(frame_samples)?
                .checked_mul(1000)
                .ok_or("Duration overflow")?
                != u64::from(sample_rate)
                    .checked_mul(duration)
                    .ok_or("Duration overflow")?
            {
                return Err("Contradictory frame duration".into());
            }
            let mode = meta["mode"].as_str().ok_or("Missing mode")?;
            let bitrate = match mode {
                "silk" => 16000,
                "hybrid" => 32000,
                "celt" => 64000,
                _ => return Err("Invalid mode".into()),
            };
            if meta["reference_revision"] != REFERENCE_REVISION
                || meta["reference_config"] != REFERENCE_CONFIG
                || meta["bitrate"] != bitrate
            {
                return Err("Reference provenance mismatch".into());
            }
            let index = meta["packet_index"]
                .as_u64()
                .ok_or("Missing packet_index")?;
            if meta["decoder_state"] != if index == 0 { "reset" } else { "continued" } {
                return Err("Packet state/index mismatch".into());
            }
            validate_toc(
                &packet,
                mode,
                meta["bandwidth"].as_str().ok_or("Missing bandwidth")?,
                sample_rate,
                channels,
                frame_samples,
            )?;
            if meta["pcm_semantics"] != "raw_decoder_output"
                || meta["packet_fnv1a64"] != checksum(&packet)
                || meta["pcm_fnv1a64"] != checksum(&pcm_bytes)
            {
                return Err("Fixture checksum or PCM semantics mismatch".into());
            }
        }

        let expected_pcm = pcm_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| i16::from_le_bytes(*chunk))
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
    /// Returns error if:
    /// * The provided directory does not exist or cannot be read
    /// * A directory entry cannot be read while scanning for vectors
    /// * Entry type metadata cannot be read for an item in the directory
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use moosicbox_opus_native_test_vectors::{TestVector, test_vectors_dir};
    ///
    /// let vectors = TestVector::load_all(test_vectors_dir().join("silk"))?;
    /// println!("Loaded {} vectors", vectors.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

/// Ordered reference packets, decoded with one persistent decoder.
#[derive(Debug, Clone)]
pub struct TestSequence {
    /// Directory name identifying this sequence.
    pub name: String,
    /// Packets in decode order; PCM includes the reference decoder's raw delay.
    pub packets: Vec<TestVector>,
}

impl TestSequence {
    /// Loads an explicit `sequence.json` v1 manifest and validates every packet.
    ///
    /// # Errors
    /// * Returns an error for missing files, invalid JSON, unsupported semantics,
    ///   unsafe paths, noncontiguous packet indices, invalid TOCs or checksums.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("sequence.json"))?)?;
        if manifest["format_version"] != 1
            || manifest["decoder_state"] != "persistent"
            || manifest["pcm_semantics"] != "raw_decoder_output"
        {
            return Err("Unsupported sequence format".into());
        }
        let entries = manifest["packets"]
            .as_array()
            .ok_or("Missing sequence packets")?;
        if entries.is_empty() {
            return Err("Empty sequence".into());
        }
        let mut packets = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let expected = if index == 0 {
                ".".to_owned()
            } else {
                format!("packet_{index:03}")
            };
            if entry.as_str() != Some(expected.as_str()) {
                return Err("Noncanonical sequence packet path".into());
            }
            let dir = path.join(expected);
            let meta: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("meta.json"))?)?;
            if meta["format_version"] != 2
                || meta["packet_index"].as_u64() != Some(u64::try_from(index)?)
                || meta["decoder_state"] != if index == 0 { "reset" } else { "continued" }
            {
                return Err("Sequence state/index mismatch".into());
            }
            let vector = TestVector::load(dir)?;
            if let Some(first) = packets.first() {
                let first: &TestVector = first;
                if first.sample_rate != vector.sample_rate || first.channels != vector.channels {
                    return Err("Sequence output configuration changed".into());
                }
            }
            packets.push(vector);
        }
        Ok(Self {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("Invalid sequence name")?
                .to_owned(),
            packets,
        })
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
///
/// # Examples
///
/// ```rust
/// use moosicbox_opus_native_test_vectors::calculate_snr;
///
/// let reference = [1000, -1000, 500, -500];
/// let decoded = [995, -1005, 490, -510];
///
/// let snr = calculate_snr(&reference, &decoded);
/// assert!(snr > 30.0);
/// ```
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

        signal_power = ref_f.mul_add(ref_f, signal_power);
        noise_power = error.mul_add(error, noise_power);
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
mod mutation_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mislabeled_tocs() {
        assert!(validate_toc(&[0x08], "celt", "nb", 8000, 1, 160).is_err());
        assert!(validate_toc(&[0xf8], "hybrid", "fb", 48000, 1, 960).is_err());
        assert!(validate_toc(&[0xf8], "celt", "swb", 48000, 1, 960).is_err());
        assert!(validate_toc(&[0xf8], "celt", "fb", 48000, 2, 960).is_err());
        assert!(validate_toc(&[0xf8], "celt", "fb", 48000, 1, 480).is_err());
        assert!(validate_toc(&[], "celt", "fb", 48000, 1, 960).is_err());
        assert!(validate_toc(&[0xf8], "celt", "fb", 48000, 1, 960).is_ok());
        assert_eq!(checksum(b"hello"), "a430d84680aabd0b");
    }

    #[test]
    fn inventory_loads_every_case() {
        let root = test_vectors_dir();
        let inventory: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("inventory.json")).unwrap()).unwrap();
        let mut singles = 0;
        let mut sequences = 0;
        for case in inventory["cases"].as_array().unwrap() {
            let path = root.join(case["path"].as_str().unwrap());
            if case["kind"] == "sequence" {
                let sequence = TestSequence::load(&path).unwrap();
                assert_eq!(sequence.packets.len(), 4);
                assert_eq!(
                    TestVector::load(&path).unwrap().packet,
                    sequence.packets[0].packet
                );
                sequences += 1;
            } else {
                TestVector::load(path).unwrap();
                singles += 1;
            }
        }
        assert_eq!(singles, 38);
        assert_eq!(sequences, 5);
    }

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

/// Complete mandatory corpus identity verification.
pub mod frozen;
