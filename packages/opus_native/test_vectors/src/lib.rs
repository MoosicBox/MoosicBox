#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TestVector {
    pub name: String,
    pub packet: Vec<u8>,
    pub expected_pcm: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u8,
}

impl TestVector {
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

    10.0 * (signal_power / noise_power).log10()
}

#[must_use]
pub fn test_vectors_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("generated")
}

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
