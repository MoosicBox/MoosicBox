#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Raw SILK regression checks for supported output rates. Full coverage, including
//! known CELT/hybrid failures, is enforced by the strict scoreboard executable.
//! No delay compensation, SNR acceptance, prefix trimming, or ignored codec tests.

use moosicbox_opus_native_test_vectors::{calculate_snr, test_vectors_dir};

#[test]
fn snr_utility_distinguishes_length_failure_from_identical_pcm() {
    assert_eq!(calculate_snr(&[1, 2], &[1, 2]), f64::INFINITY);
    assert_eq!(calculate_snr(&[1, 2], &[1]), f64::NEG_INFINITY);
}

#[cfg(feature = "silk")]
#[test]
fn supported_silk_reference_pcm_is_raw_and_exact() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    use moosicbox_opus_native_test_vectors::TestVector;
    let inventory: serde_json::Value =
        serde_json::from_slice(&std::fs::read(test_vectors_dir().join("inventory.json")).unwrap())
            .unwrap();
    let mut checked = 0;
    let mut failures = Vec::new();
    for case in inventory["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        if !["silk/nb/", "silk/mb/", "silk/wb/"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
        {
            continue;
        }
        let vector = TestVector::load(test_vectors_dir().join(id)).unwrap();
        let channels = if vector.channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let mut decoder =
            Decoder::new(SampleRate::from_hz(vector.sample_rate).unwrap(), channels).unwrap();
        let mut output =
            vec![0; vector.sample_rate as usize * 120 / 1000 * usize::from(vector.channels)];
        let count = decoder
            .decode(Some(&vector.packet), &mut output, false)
            .unwrap();
        output.truncate(count * usize::from(vector.channels));
        if output != vector.expected_pcm {
            let first = output
                .iter()
                .zip(&vector.expected_pcm)
                .position(|(actual, expected)| actual != expected);
            failures.push(format!(
                "{id}: raw length {}/{}, first difference {first:?}",
                output.len(),
                vector.expected_pcm.len()
            ));
        }
        checked += 1;
    }
    assert_eq!(checked, 16, "mandatory supported SILK coverage");
    assert!(
        failures.is_empty(),
        "raw PCM failures: {}",
        failures.join("; ")
    );
}

#[test]
fn vectors_directory_exists() {
    assert!(test_vectors_dir().is_dir());
}
