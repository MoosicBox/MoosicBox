//! Common-platform native regression baselines from the deterministic qualification probe.
//!
//! Native baselines intentionally reviewed on 2026-10-24 after stereo allocation,
//! SILK prediction and framing fixes. Source+probe SHA-256:
//! fc2c214180eff4536a4cacce4eaf14c8df934d7b8502713f22d47daa15b8a678.
//! Capture executable SHA-256:
//! 8c3f5c8e96e1a01459cfe314fa09747403d7191823ab42460cbc6829ff494f7e.
//! All 55 recorded files matched the independently reviewed 74c8831e capture:
//! 13 streams/7213 packets decoded to bit-exact native=C PCM with actual TOCs
//! and durations checked, SNR 19.028..34.160 dB and no clipping. Corpus outcomes
//! classify corrupt inputs, not valid-stream conformance (155 C/native rejects differ).
//! These are native regression baselines, NOT C-generated encoding goldens or
//! perceptual quality certification. Tests never regenerate expectations.
//! All platforms compare the same length-delimited packets and little-endian PCM.
#![cfg(feature = "celt")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[path = "../examples/qualification_probe.rs"]
mod probe;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "moosicbox-opus-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn check(directory: &Path, name: &str, extension: &str, expected: (usize, u64)) {
    assert_eq!(
        probe::fingerprint(&directory.join(format!("{name}.{extension}"))).unwrap(),
        expected,
        "native regression baseline changed: {name}.{extension}; investigate, do not regenerate to hide failures"
    );
}

#[test]
fn deterministic_malformed_corpus_has_common_packet_structure() {
    let directory = Directory::new();
    probe::malformed(&directory.0).unwrap();
    check(
        &directory.0,
        "corpus",
        "packets",
        (475_181, 0x7a23_9f98_6a5e_57e2),
    );
}

// Decoder acceptance and errors depend on which codec implementations are enabled.
#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn deterministic_malformed_corpus_has_common_expected_outcomes() {
    let directory = Directory::new();
    probe::malformed(&directory.0).unwrap();
    check(
        &directory.0,
        "corpus",
        "outcomes",
        // Approved 2026-10-24: 128 one-byte-constituent records now follow
        // upstream PLC/DTX. Independent C replacement of precisely those
        // records reproduces this complete fingerprint (+298 bytes).
        (19_809, 0x07bc_8c07_bd43_40e1),
    );
}

#[test]
fn celt_all_rates_and_channels_match_common_packet_and_pcm_baselines() {
    let cases = [
        (8000, 1, 32320, 0xa91a_81e0_97b3_72d0, 0x116c_465b_d306_2160),
        (8000, 2, 64640, 0xeaee_de88_5aa1_7a5c, 0x6c2e_1424_abe5_cd17),
        (
            12000,
            1,
            48480,
            0x4e30_62aa_a8dd_e8be,
            0x487d_ad24_4fb7_358e,
        ),
        (
            12000,
            2,
            96960,
            0xe092_6062_856c_be08,
            0x64e5_6419_d7e2_d62d,
        ),
        (
            16000,
            1,
            64640,
            0x7535_e87d_a8cd_3802,
            0x5c6e_0566_6abd_1dbf,
        ),
        (
            16000,
            2,
            129_280,
            0x773d_1e5d_87a1_1149,
            0x8ee1_4ede_b9eb_1701,
        ),
        (
            24000,
            1,
            96960,
            0x2271_6f7d_5ad7_a1d6,
            0xc5cd_88ee_6f14_62ec,
        ),
        (
            24000,
            2,
            193_920,
            0x8e0a_c871_1789_a28a,
            0x2cba_816a_d063_daf4,
        ),
        (
            48000,
            1,
            193_920,
            0x50dc_fdc3_330f_61ae,
            0x60fa_b675_3951_bccf,
        ),
        (
            48000,
            2,
            387_840,
            0x505a_12b0_71e5_28b0,
            0x01b0_e6ec_1755_bf83,
        ),
    ];
    let directory = Directory::new();
    for (rate, channels, pcm_bytes, packets, pcm) in cases {
        probe::stream(&directory.0, "celt", rate, channels, 100).unwrap();
        let name = format!("celt-{rate}-{channels}-100");
        check(&directory.0, &name, "packets", (24644, packets));
        check(&directory.0, &name, "pcm", (pcm_bytes, pcm));
    }
}

#[test]
#[cfg(feature = "silk")]
fn silk_stream_matches_common_native_regression_baseline() {
    let directory = Directory::new();
    probe::stream(&directory.0, "silk", 16000, 1, 100).unwrap();
    check(
        &directory.0,
        "silk-16000-1-100",
        "packets",
        // Approved flush-only correction: packets 0..100 are unchanged; the
        // final zero-input packet now uses synthesis-aware silence LPC scoring.
        // Debug/release agree and the complete stream decodes exactly as pinned C.
        (15_925, 0x8a06_e315_1837_b9c8),
    );
    check(
        &directory.0,
        "silk-16000-1-100",
        "pcm",
        (64_640, 0xb14d_6f89_9afa_3b68),
    );
}

#[test]
#[cfg(feature = "hybrid")]
fn hybrid_stream_matches_common_native_regression_baseline() {
    let directory = Directory::new();
    probe::stream(&directory.0, "hybrid", 48000, 2, 100).unwrap();
    check(
        &directory.0,
        "hybrid-48000-2-100",
        "packets",
        (48884, 0xa940_1071_980a_272d),
    );
    check(
        &directory.0,
        "hybrid-48000-2-100",
        "pcm",
        (387_840, 0xc6dd_981a_260f_7c78),
    );
}

#[test]
fn two_minute_stream_matches_common_native_packet_and_pcm_baselines() {
    let directory = Directory::new();
    probe::stream(&directory.0, "celt", 48000, 2, 6000).unwrap();
    check(
        &directory.0,
        "celt-48000-2-6000",
        "packets",
        (1_464_244, 0xc334_dd59_c773_5444),
    );
    check(
        &directory.0,
        "celt-48000-2-6000",
        "pcm",
        (23_043_840, 0xc13e_ea0e_1885_2a25),
    );
}
