//! Independent pinned-C fixtures, decoded exactly by ordinary Cargo tests.
//!
//! The unchanged frozen corpus was audited against fixed-point scalar libopus
//! revision 34bba701ae97c913de719b1f7c10686f62cddb15; see frozen/audit-proof.json.
//! Unlike native regression fingerprints, these PCM bytes are independent expected output.
#![cfg(any(feature = "celt", feature = "silk", feature = "hybrid"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use std::fs;
use std::path::Path;

fn exact_sequence(case: &str, hz: u32, channels: usize, packets: usize) -> usize {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_vectors/frozen/corpus")
        .join(case);
    let layout = if channels == 1 {
        Channels::Mono
    } else {
        Channels::Stereo
    };
    let mut decoder = Decoder::new(SampleRate::from_hz(hz).unwrap(), layout).unwrap();
    let mut total = 0;
    for index in 0..packets {
        let directory = if index == 0 {
            base.clone()
        } else {
            base.join(format!("packet_{index:03}"))
        };
        let packet = fs::read(directory.join("packet.bin")).unwrap();
        let bytes = fs::read(directory.join("expected.pcm")).unwrap();
        assert!(bytes.len().is_multiple_of(2 * channels));
        let expected: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| i16::from_le_bytes(*s))
            .collect();
        let mut actual = vec![1234; expected.len()];
        let samples = decoder.decode(Some(&packet), &mut actual, false).unwrap();
        assert_eq!(
            samples * channels,
            expected.len(),
            "duration {case}/{index}"
        );
        assert_eq!(actual, expected, "raw PCM {case}/{index}");
        total += actual.len();
    }
    total
}

#[test]
#[cfg(feature = "celt")]
fn frozen_celt_raw_pcm_is_exact() {
    let cases = [
        ("celt/fb/impulse_mono", 48000, 1, 1),
        ("celt/fb/noise_mono", 48000, 1, 1),
        ("celt/fb/silence_mono", 48000, 1, 1),
        ("celt/fb/sine_mono", 48000, 1, 1),
        ("celt/fb/sine_stereo", 48000, 2, 1),
        ("celt/nb/impulse_mono", 8000, 1, 1),
        ("celt/nb/sine_mono", 8000, 1, 1),
        ("celt/nb/sine_stereo", 8000, 2, 1),
        ("celt/swb/sine_mono", 24000, 1, 1),
        ("celt/swb/sine_stereo", 24000, 2, 1),
        ("celt/wb/noise_mono", 16000, 1, 1),
        ("celt/wb/sine_mono", 16000, 1, 1),
        ("celt/wb/sine_stereo", 16000, 2, 1),
        ("sequences/celt_fb_mono", 48000, 1, 4),
        ("sequences/celt_fb_stereo", 48000, 2, 4),
    ];
    let samples: usize = cases
        .into_iter()
        .map(|(name, hz, ch, n)| exact_sequence(name, hz, ch, n))
        .sum();
    assert_eq!(samples, 16080);
}

#[test]
#[cfg(feature = "hybrid")]
fn frozen_hybrid_raw_pcm_is_exact() {
    let cases = [
        ("hybrid/fb/mixed_mono", 48000, 1, 1),
        ("hybrid/fb/silence_mono", 48000, 1, 1),
        ("hybrid/fb/silence_stereo", 48000, 2, 1),
        ("hybrid/fb/sine_mono", 48000, 1, 1),
        ("hybrid/fb/sine_stereo", 48000, 2, 1),
        ("hybrid/swb/sine_mono", 24000, 1, 1),
        ("hybrid/swb/sine_stereo", 24000, 2, 1),
        ("sequences/hybrid_fb_mono", 48000, 1, 4),
        ("sequences/hybrid_swb_stereo", 24000, 2, 4),
    ];
    let samples: usize = cases
        .into_iter()
        .map(|(name, hz, ch, n)| exact_sequence(name, hz, ch, n))
        .sum();
    assert_eq!(samples, 15840);
}

#[test]
#[cfg(feature = "silk")]
fn frozen_silk_raw_pcm_is_exact() {
    let cases = [
        ("sequences/silk_wb_mono", 16000, 1, 4),
        ("silk/mb/mixed_stereo", 12000, 2, 1),
        ("silk/mb/noise_mono", 12000, 1, 1),
        ("silk/mb/silence_mono", 12000, 1, 1),
        ("silk/mb/sine_mono", 12000, 1, 1),
        ("silk/nb/impulse_mono", 8000, 1, 1),
        ("silk/nb/mixed_mono", 8000, 1, 1),
        ("silk/nb/noise_mono", 8000, 1, 1),
        ("silk/nb/noise_stereo", 8000, 2, 1),
        ("silk/nb/quiet_mono", 8000, 1, 1),
        ("silk/nb/silence_mono", 8000, 1, 1),
        ("silk/nb/sine_mono", 8000, 1, 1),
        ("silk/nb/sine_stereo", 8000, 2, 1),
        ("silk/swb/mixed_stereo", 24000, 2, 1),
        ("silk/swb/sine_mono", 24000, 1, 1),
        ("silk/wb/mixed_stereo", 16000, 2, 1),
        ("silk/wb/noise_mono", 16000, 1, 1),
        ("silk/wb/silence_mono", 16000, 1, 1),
        ("silk/wb/sine_mono", 16000, 1, 1),
    ];
    let samples: usize = cases
        .into_iter()
        .map(|(name, hz, ch, n)| exact_sequence(name, hz, ch, n))
        .sum();
    assert_eq!(samples, 7120);
}
