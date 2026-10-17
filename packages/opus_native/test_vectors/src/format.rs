//! Shared wire checks, independent of either codec implementation.

/// Pinned independent C reference revision.
pub const REFERENCE_REVISION: &str = "34bba701ae97c913de719b1f7c10686f62cddb15";
/// Pinned reference encoder and build configuration.
pub const REFERENCE_CONFIG: &str = "fixed-point;scalar;float-api=off;force-mode;CBR;complexity=10";

/// FNV-1a 64-bit identity (not a cryptographic authenticity guarantee).
#[must_use]
pub fn checksum(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Checks the TOC against explicit fixture expectations. Includes packet frame-count framing.
///
/// # Errors
/// * Rejects empty packets, invalid frame count, or mismatched mode, bandwidth,
///   channel count, frame duration, or invalid output sample rate.
pub fn validate_toc(
    packet: &[u8],
    mode: &str,
    bandwidth: &str,
    sample_rate: u32,
    channels: u8,
    frame_samples: usize,
) -> Result<(), &'static str> {
    if ![8000, 12000, 16000, 24000, 48000].contains(&sample_rate) {
        return Err("Invalid output sample rate");
    }
    let toc = *packet.first().ok_or("Empty Opus packet")?;
    let frame_count = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u64::from(*packet.get(1).ok_or("Missing frame count")? & 63),
    };
    if frame_count == 0 || frame_count > 48 {
        return Err("Invalid frame count");
    }
    let config = toc >> 3;
    let (actual_mode, actual_bandwidth, duration_us) = match config {
        0..=11 => (
            "silk",
            ["nb", "mb", "wb"][usize::from(config / 4)],
            [10000, 20000, 40000, 60000][usize::from(config % 4)],
        ),
        12..=15 => (
            "hybrid",
            ["swb", "fb"][usize::from((config - 12) / 2)],
            [10000, 20000][usize::from(config % 2)],
        ),
        _ => (
            "celt",
            ["nb", "wb", "swb", "fb"][usize::from((config - 16) / 4)],
            [2500, 5000, 10000, 20000][usize::from(config % 4)],
        ),
    };
    if mode != actual_mode {
        return Err("TOC mode mismatch");
    }
    if bandwidth != actual_bandwidth {
        return Err("TOC bandwidth mismatch");
    }
    if channels != 1 + ((toc >> 2) & 1) {
        return Err("TOC channels mismatch");
    }
    let samples_us = u64::try_from(frame_samples)
        .map_err(|_| "Frame too large")?
        .checked_mul(1_000_000)
        .ok_or("Frame duration overflow")?;
    if frame_samples == 0 || duration_us * frame_count > 120_000 {
        return Err("Invalid packet duration");
    }
    if samples_us != u64::from(sample_rate) * duration_us * frame_count {
        return Err("TOC frame duration mismatch");
    }
    Ok(())
}
