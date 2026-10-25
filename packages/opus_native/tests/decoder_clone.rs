//! Snapshot contracts that compile with no decoder features as well as each codec alone.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_opus_native::{Channels, Decoder, SampleRate};

#[test]
fn fresh_snapshot_has_independent_gain_and_loss_state() {
    let mut original = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    original.set_gain(1536);
    let mut snapshot = original.clone();
    original.set_gain(-1536);
    let mut output = [123; 120];
    assert_eq!(snapshot.decode_loss(&mut output, 120).unwrap(), 120);
    assert_eq!(output, [0; 120]);
    // Cloning is a value snapshot, not shared mutable configuration.
    assert_eq!(snapshot.gain(), 1536);
    assert_eq!(original.gain(), -1536);
}

#[cfg(any(feature = "silk", feature = "celt"))]
#[test]
fn codec_only_snapshot_retains_independent_persistent_history() {
    let packets: &[&[u8]] = &[
        #[cfg(feature = "silk")]
        include_bytes!("../src/silk/reference/nb_sine_mono.packet"),
        #[cfg(feature = "celt")]
        include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin"),
    ];
    for packet in packets {
        let mut original = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut control = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        for decoder in [&mut original, &mut control] {
            for _ in 0..4 {
                assert_eq!(
                    decoder.decode(Some(packet), &mut [0; 960], false).unwrap(),
                    960
                );
            }
        }
        let mut snapshot = original.clone();
        let mut loss_control = control.clone();
        let mut actual = [123; 960];
        let mut expected = [0; 960];
        for _ in 0..3 {
            snapshot.decode_loss(&mut actual, 960).unwrap();
            loss_control.decode_loss(&mut expected, 960).unwrap();
            assert_eq!(actual, expected);
        }
        for _ in 0..3 {
            original.decode(Some(packet), &mut actual, false).unwrap();
            control.decode(Some(packet), &mut expected, false).unwrap();
            assert_eq!(
                actual, expected,
                "speculation must not modify committed history"
            );
            snapshot.decode(Some(packet), &mut actual, false).unwrap();
            loss_control
                .decode(Some(packet), &mut expected, false)
                .unwrap();
            assert_eq!(
                actual, expected,
                "snapshot recovery must match its own loss history"
            );
        }
    }
}
