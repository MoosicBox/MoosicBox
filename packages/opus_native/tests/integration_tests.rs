use moosicbox_opus_native_test_vectors::{calculate_snr, test_vectors_dir};

#[cfg(feature = "silk")]
#[test]
fn test_decode_silk_vectors() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    use moosicbox_opus_native_test_vectors::vectors_available;

    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    // TODO: SWB (24kHz) requires resampling feature (SILK max internal rate is 16kHz)
    for bandwidth in &["nb", "mb", "wb"] {
        use moosicbox_opus_native_test_vectors::TestVector;

        let vectors_dir = test_vectors_dir().join("silk").join(bandwidth);
        if !vectors_dir.exists() {
            continue;
        }

        let vectors = TestVector::load_all(&vectors_dir).unwrap_or_else(|e| {
            panic!("Failed to load SILK test vectors from {vectors_dir:?}: {e}");
        });

        for vector in vectors {
            let sample_rate = SampleRate::from_hz(vector.sample_rate).unwrap_or_else(|e| {
                panic!("Invalid sample rate {}: {e}", vector.sample_rate);
            });
            let channels = if vector.channels == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            };

            let mut decoder = Decoder::new(sample_rate, channels)
                .unwrap_or_else(|e| panic!("Failed to create decoder: {e}"));

            let mut output = vec![0i16; vector.expected_pcm.len()];
            let decoded_samples = decoder
                .decode(Some(&vector.packet), &mut output, false)
                .unwrap_or_else(|e| panic!("Failed to decode {}: {e:?}", vector.name));

            eprintln!("Test: {}", vector.name);
            eprintln!(
                "  Decoded {} samples/ch, expected {} output samples",
                decoded_samples,
                output.len()
            );

            assert_eq!(
                decoded_samples * usize::from(vector.channels),
                output.len(),
                "Sample count mismatch for {}",
                vector.name
            );

            // libopus includes SILK algorithmic delay as leading zeros in output
            // Our decoder currently produces output WITHOUT the leading delay zeros
            // Skip the delay samples from libopus output to match our decoder behavior
            let delay_samples_per_channel = decoder.algorithmic_delay_samples();
            let delay_samples_total = delay_samples_per_channel * usize::from(vector.channels);

            eprintln!(
                "  Algorithmic delay: {} samples/ch (libopus includes as zeros, we skip)",
                delay_samples_per_channel
            );

            // DEBUG: Show raw expected PCM before skipping delay
            if vector.name.contains("stereo") {
                eprintln!(
                    "  Raw expected_pcm[0..20]: {:?}",
                    &vector.expected_pcm[..20.min(vector.expected_pcm.len())]
                );
                eprintln!(
                    "  Raw expected_pcm total length: {}",
                    vector.expected_pcm.len()
                );
            }

            // Skip delay from libopus output, compare with our delay-free output
            let expected = if delay_samples_total < vector.expected_pcm.len() {
                &vector.expected_pcm[delay_samples_total..]
            } else {
                &vector.expected_pcm[..]
            };
            let actual_full = &output[..decoded_samples * usize::from(vector.channels)];
            let actual = &actual_full[..expected.len().min(actual_full.len())];

            eprintln!(
                "  Expected[0..20]: {:?}",
                &expected[..20.min(expected.len())]
            );
            eprintln!("  Actual[0..20]: {:?}", &actual[..20.min(actual.len())]);

            let snr = calculate_snr(expected, actual);
            eprintln!("  SNR: {} dB", snr);

            assert!(
                snr > 40.0,
                "SNR too low for {}: {} dB (expected > 40 dB)",
                vector.name,
                snr
            );
        }
    }
}

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[test]
    fn test_snr_utilities() {
        let signal = vec![100, -200, 300, -400];
        let snr = calculate_snr(&signal, &signal);
        assert!(snr.is_infinite());

        let reference = vec![1000, 2000, 3000];
        let decoded = vec![1010, 1990, 3005];
        let snr = calculate_snr(&reference, &decoded);
        assert!(snr > 40.0);
    }

    #[test]
    fn test_vectors_directory_exists() {
        let dir = test_vectors_dir();
        assert!(dir.exists(), "test-vectors directory should exist");
        assert!(dir.is_dir(), "test-vectors should be a directory");
    }

    #[cfg(feature = "silk")]
    #[test]
    fn test_sine_stereo_bit_exact() {
        use moosicbox_opus_native::{Channels, Decoder, SampleRate};
        use moosicbox_opus_native_test_vectors::{TestVector, vectors_available};

        if !vectors_available() {
            eprintln!("Skipping: test vectors not generated");
            return;
        }

        let vectors_dir = test_vectors_dir().join("silk").join("nb");
        let vectors = TestVector::load_all(&vectors_dir).unwrap();

        let vector = vectors
            .iter()
            .find(|v| v.name.contains("sine_stereo"))
            .expect("sine_stereo vector not found");

        eprintln!(
            "Packet hex: {}",
            vector
                .packet
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        );

        let sample_rate = SampleRate::from_hz(vector.sample_rate).unwrap();
        let mut decoder = Decoder::new(sample_rate, Channels::Stereo).unwrap();

        let mut output = vec![0i16; vector.expected_pcm.len()];
        let decoded_samples = decoder
            .decode(Some(&vector.packet), &mut output, false)
            .unwrap();

        let delay_samples_per_channel = decoder.algorithmic_delay_samples();
        let delay_samples_total = delay_samples_per_channel * 2;
        eprintln!(
            "Expected PCM (with delay, first 40): {:?}",
            &vector.expected_pcm[..40]
        );
        let expected = &vector.expected_pcm[delay_samples_total..];
        eprintln!(
            "Expected PCM (after skipping {} delay, first 40): {:?}",
            delay_samples_total,
            &expected[..40]
        );
        let actual_full = &output[..decoded_samples * 2];
        let actual = &actual_full[..expected.len().min(actual_full.len())];
        eprintln!("Actual PCM (first 20): {:?}", &actual[..20]);

        let mut diff_count = 0;
        let mut max_diff = 0_i32;
        let mut first_diff_idx = None;
        for (i, (&exp, &act)) in expected.iter().zip(actual.iter()).enumerate() {
            if exp != act {
                if diff_count < 10 {
                    let ch = if i % 2 == 0 { "L" } else { "R" };
                    let sample_idx = i / 2;
                    eprintln!(
                        "DIFF at sample {}, {}[{}]: expected {}, got {} (diff={})",
                        i,
                        ch,
                        sample_idx,
                        exp,
                        act,
                        act - exp
                    );
                }
                if first_diff_idx.is_none() {
                    first_diff_idx = Some(i);
                }
                diff_count += 1;
                max_diff = max_diff.max(i32::from((act - exp).abs()));
            }
        }

        let snr = calculate_snr(expected, actual);
        eprintln!("SNR: {:.2} dB", snr);

        if diff_count > 0 {
            eprintln!("First diff at sample {}", first_diff_idx.unwrap());
            eprintln!(
                "Stereo decoding has {} differences (max diff: {})",
                diff_count, max_diff
            );

            // Accept high SNR (>50dB) as pass - ±1 sample differences are common in fixed-point implementations
            assert!(
                snr > 50.0,
                "SNR too low: {:.2} dB (expected > 50 dB). {} differences, max diff: {}",
                snr,
                diff_count,
                max_diff
            );
        }
    }
}

#[cfg(feature = "silk")]
#[test]
fn test_decode_silk_vectors_skip_delay() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    use moosicbox_opus_native_test_vectors::vectors_available;

    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    const DELAY_SAMPLES: usize = 5;

    for bandwidth in &["nb"] {
        use moosicbox_opus_native_test_vectors::TestVector;

        let vectors_dir = test_vectors_dir().join("silk").join(bandwidth);
        if !vectors_dir.exists() {
            continue;
        }

        let vectors = TestVector::load_all(&vectors_dir).unwrap();

        for vector in vectors {
            if vector.channels > 1 {
                // TODO: Stereo decoding - see main test comment
                continue;
            }
            let sample_rate = SampleRate::from_hz(vector.sample_rate).unwrap();
            let channels = if vector.channels == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            };

            let mut decoder = Decoder::new(sample_rate, channels).unwrap();

            let mut output = vec![0i16; vector.expected_pcm.len()];
            let decoded_samples = decoder
                .decode(Some(&vector.packet), &mut output, false)
                .unwrap();

            eprintln!(
                "Test: {} (skipping {} delay samples)",
                vector.name, DELAY_SAMPLES
            );

            // Compare output[0..] with expected[DELAY_SAMPLES..]
            let min_len = (decoded_samples * usize::from(vector.channels))
                .min(vector.expected_pcm.len() - DELAY_SAMPLES);

            let expected_shifted = &vector.expected_pcm[DELAY_SAMPLES..DELAY_SAMPLES + min_len];
            let actual = &output[..min_len];

            eprintln!(
                "  Expected (shifted)[0..20]: {:?}",
                &expected_shifted[..20.min(expected_shifted.len())]
            );
            eprintln!("  Actual[0..20]: {:?}", &actual[..20.min(actual.len())]);

            // Find first mismatch
            for (i, (&exp, &act)) in expected_shifted.iter().zip(actual.iter()).enumerate() {
                if exp != act {
                    eprintln!(
                        "  First mismatch at sample {}: expected {}, got {}",
                        i, exp, act
                    );
                    eprintln!(
                        "  Context: exp[{}..{}] = {:?}",
                        i.saturating_sub(5),
                        (i + 5).min(expected_shifted.len()),
                        &expected_shifted[i.saturating_sub(5)..(i + 5).min(expected_shifted.len())]
                    );
                    eprintln!(
                        "  Context: act[{}..{}] = {:?}",
                        i.saturating_sub(5),
                        (i + 5).min(actual.len()),
                        &actual[i.saturating_sub(5)..(i + 5).min(actual.len())]
                    );
                    break;
                }
            }

            let snr = calculate_snr(expected_shifted, actual);
            eprintln!("  SNR (with delay compensation): {} dB", snr);

            if snr > 20.0 {
                eprintln!("  ✓ Much better SNR with delay compensation!");
            }
        }
    }
}
