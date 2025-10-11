use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use moosicbox_opus_native_test_vectors::{
    TestVector, calculate_snr, test_vectors_dir, vectors_available,
};

#[test]
fn test_decode_silk_vectors() {
    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    for bandwidth in &["nb", "mb", "wb", "swb"] {
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
                "  Decoded {} samples, expected {} output samples",
                decoded_samples,
                output.len()
            );

            assert_eq!(
                decoded_samples * usize::from(vector.channels),
                output.len(),
                "Sample count mismatch for {}",
                vector.name
            );

            // Account for 5-sample algorithmic delay in SILK decoder
            const DELAY_SAMPLES: usize = 5;
            let expected_shifted = &vector.expected_pcm[DELAY_SAMPLES..];
            let actual_trimmed = &output[..output.len() - DELAY_SAMPLES];

            eprintln!(
                "  Expected (shifted)[0..20]: {:?}",
                &expected_shifted[..20.min(expected_shifted.len())]
            );
            eprintln!(
                "  Actual[0..20]: {:?}",
                &actual_trimmed[..20.min(actual_trimmed.len())]
            );

            let snr = calculate_snr(expected_shifted, actual_trimmed);
            eprintln!("  SNR (with delay compensation): {} dB", snr);

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
}

#[test]
fn test_decode_silk_vectors_skip_delay() {
    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    const DELAY_SAMPLES: usize = 5;

    for bandwidth in &["nb"] {
        let vectors_dir = test_vectors_dir().join("silk").join(bandwidth);
        if !vectors_dir.exists() {
            continue;
        }

        let vectors = TestVector::load_all(&vectors_dir).unwrap();

        for vector in vectors {
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
