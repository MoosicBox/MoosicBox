use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use moosicbox_opus_native_test_vectors::{
    TestVector, calculate_snr, test_vectors_dir, vectors_available,
};

#[test]
#[ignore = "Decoder not yet complete enough to handle real libopus packets"]
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

            assert_eq!(
                decoded_samples * usize::from(vector.channels),
                output.len(),
                "Sample count mismatch for {}",
                vector.name
            );

            let snr = calculate_snr(&vector.expected_pcm, &output);
            assert!(
                snr > 40.0,
                "SNR too low for {}: {} dB (expected > 40 dB)",
                vector.name,
                snr
            );
        }
    }
}

#[test]
fn test_decode_celt_vectors() {
    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    for bandwidth in &["nb", "wb", "swb", "fb"] {
        let vectors_dir = test_vectors_dir().join("celt").join(bandwidth);
        if !vectors_dir.exists() {
            continue;
        }

        let vectors = TestVector::load_all(&vectors_dir).unwrap_or_else(|e| {
            panic!("Failed to load CELT test vectors from {vectors_dir:?}: {e}");
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

            assert_eq!(
                decoded_samples * usize::from(vector.channels),
                output.len(),
                "Sample count mismatch for {}",
                vector.name
            );

            let snr = calculate_snr(&vector.expected_pcm, &output);
            assert!(
                snr > 40.0,
                "SNR too low for {}: {} dB (expected > 40 dB)",
                vector.name,
                snr
            );
        }
    }
}

#[test]
fn test_decode_integration_vectors() {
    if !vectors_available() {
        eprintln!("Skipping: test vectors not generated");
        return;
    }

    let vectors_dir = test_vectors_dir().join("integration");
    if !vectors_dir.exists() {
        eprintln!("Skipping test: {vectors_dir:?} does not exist");
        return;
    }

    let vectors = TestVector::load_all(&vectors_dir).unwrap_or_else(|e| {
        panic!("Failed to load integration test vectors: {e}");
    });

    if vectors.is_empty() {
        eprintln!("Skipping test: no test vectors found in {vectors_dir:?}");
        return;
    }

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

        eprintln!(
            "DEBUG {}: decoded_samples={}, expected_len={}, channels={}",
            vector.name,
            decoded_samples,
            vector.expected_pcm.len(),
            vector.channels
        );
        eprintln!(
            "Expected[200..210]: {:?}",
            &vector.expected_pcm
                [200.min(vector.expected_pcm.len())..210.min(vector.expected_pcm.len())]
        );
        eprintln!(
            "Actual[200..210]: {:?}",
            &output[200.min(output.len())..210.min(output.len())]
        );

        assert_eq!(
            decoded_samples * usize::from(vector.channels),
            output.len(),
            "Sample count mismatch for {}",
            vector.name
        );

        let snr = calculate_snr(&vector.expected_pcm, &output);
        assert!(
            snr > 40.0,
            "SNR too low for {}: {} dB (expected > 40 dB)",
            vector.name,
            snr
        );
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
