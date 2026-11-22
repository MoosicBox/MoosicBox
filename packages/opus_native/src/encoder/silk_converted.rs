//! Test-only qualification of converted-rate SILK floor candidates.
use super::*;

#[test]
#[ignore = "requires explicit speech WAV and pinned C oracle"]
fn converted_rate_floor_candidates() {
    let speech = completion::completion_tests::read_speech_samples();
    let mut failures = Vec::new();
    for rate in [24_000, 48_000] {
        let speech = convert_speech_fixture(&speech, rate);
        for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
            for milliseconds in [10, 20, 40, 60] {
                for noisy in [false, true] {
                    if std::panic::catch_unwind(|| {
                        qualify_case(rate, channels, milliseconds, noisy, &speech);
                    })
                    .is_err()
                    {
                        failures.push((rate, channels, milliseconds, noisy));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "converted floor candidate failures: {failures:?}"
    );
}

fn convert_speech_fixture(input: &[i16], rate: u32) -> Vec<i16> {
    let mut converter = crate::silk::resampler::SilkResampler::new(8000, rate).unwrap();
    let mut output = Vec::new();
    for chunk in input.chunks(960) {
        // The reference conversion API accepts whole milliseconds only.
        // Pad the last partial millisecond rather than dropping speech.
        let mut padded = chunk.to_vec();
        padded.resize(chunk.len().next_multiple_of(8), 0);
        output.extend(converter.process(&padded).unwrap());
    }
    // Drain the finite conversion filter with 120 ms of silence using the
    // same state; keep this tail in the fixture instead of resetting it.
    output.extend(converter.process(&[0; 960]).unwrap());
    assert_eq!(
        output.len(),
        (input.len().next_multiple_of(8) + 960) * rate as usize / 8000
    );
    output
}

fn qualify_case(
    rate: u32,
    channels: crate::Channels,
    milliseconds: usize,
    noisy: bool,
    speech: &[i16],
) {
    let count = super::super::channel_count(channels);
    let config = EncoderConfig {
        sample_rate: SampleRate::from_hz(rate).unwrap(),
        channels,
        frame_size: rate as usize * milliseconds / 1000,
        bitrate: if count == 1 { 12_000 } else { 24_000 },
    };
    // Exercise the production constructor and conversion/tail machinery, but
    // lower only this test-local candidate's budget after legal construction.
    // This does not relax or bypass the public constructor's acceptance policy.
    let mut candidate = super::super::Encoder::new_silk(EncoderConfig {
        bitrate: if count == 1 { 48_000 } else { 96_000 },
        ..config
    })
    .unwrap();
    candidate.config.bitrate = config.bitrate;
    candidate
        .silk
        .as_mut()
        .unwrap()
        .set_bitrate(config.bitrate)
        .unwrap();
    let length = (rate as usize / 2 + 137).min(speech.len() - 31);
    let input = converted_input(speech, length, count, noisy);
    let delay = candidate.lookahead();
    let mut packets = Vec::new();
    for frame in input.chunks(config.frame_size * count) {
        let mut padded = frame.to_vec();
        padded.resize(config.frame_size * count, 0.0);
        packets.push(candidate.encode(&padded).unwrap());
        converted_packet_metrics(&candidate, &padded, packets.len() - 1);
    }
    let tail = candidate.finish().unwrap();
    assert_eq!(tail.len(), delay.div_ceil(config.frame_size));
    packets.extend(tail);
    assert_eq!(candidate.finish().unwrap(), [] as [Vec<u8>; 0]);
    let mut decoder = crate::Decoder::new(config.sample_rate, channels).unwrap();
    let mut output = Vec::new();
    for packet in &packets {
        assert!(
            packet.len() <= (config.bitrate as usize * config.frame_size / (rate as usize * 8))
        );
        let mut pcm = vec![0_i16; config.frame_size * count];
        let samples_written = decoder.decode(Some(packet), &mut pcm, false).unwrap();
        assert_eq!(samples_written, config.frame_size);
        output.extend(pcm);
    }
    assert_eq!(output.len(), packets.len() * config.frame_size * count);
    super::qualification::compare_reference(config, &packets, &output);
    converted_alignment(&input, &output, count, delay, rate);
    let trimmed = &output[delay * count..(delay + length) * count];
    let signal = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
    let error = input
        .iter()
        .zip(trimmed)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
        .sum::<f64>();
    let snr = 10.0 * (signal / error).log10();
    eprintln!(
        "converted floor rate={rate} channels={count} ms={milliseconds} noisy={noisy} snr={snr} input={length} delay={delay} packets={}",
        packets.len()
    );
    assert!(
        snr >= if noisy { 0.0 } else { 5.0 },
        "candidate quality {config:?} noisy={noisy} snr={snr}"
    );
}

fn converted_packet_metrics(candidate: &super::super::Encoder, input: &[f32], frame: usize) {
    let silk = candidate.silk.as_ref().unwrap();
    let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
    let reconstruction: Vec<i128> = silk
        .pcm_history
        .iter()
        .take(super::super::channel_count(candidate.config.channels))
        .map(|pcm| pcm.iter().map(|&x| i128::from(x).pow(2)).sum())
        .collect();
    eprintln!(
        "converted packet frame={frame} config={:?} energy={energy} gains={:?} indices={:?} reconstruction={reconstruction:?}",
        candidate.config, silk.gain, silk.previous_index
    );
}

// Diagnostic only: never use best-lag SNR to replace fixed-delay acceptance.
fn converted_alignment(input: &[f32], output: &[i16], channels: usize, delay: usize, rate: u32) {
    let length = input.len() / channels;
    let energy = input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
    let mut best = (f64::INFINITY, 0);
    for lag in delay.saturating_sub(rate as usize / 100)..=delay + rate as usize / 100 {
        if (lag + length) * channels > output.len() {
            continue;
        }
        let error = input
            .iter()
            .zip(&output[lag * channels..])
            .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
            .sum::<f64>();
        if error < best.0 {
            best = (error, lag);
        }
    }
    let aligned = &output[delay * channels..(delay + length) * channels];
    let cross = input
        .iter()
        .zip(aligned)
        .map(|(&x, &y)| f64::from(x) * f64::from(y) / 32768.0)
        .sum::<f64>();
    let output_energy = aligned
        .iter()
        .map(|&x| (f64::from(x) / 32768.0).powi(2))
        .sum::<f64>();
    eprintln!(
        "converted amplitude gain={} correlation={} output_energy={output_energy}",
        cross / energy,
        cross / (energy * output_energy).sqrt()
    );
    let clips = output
        .iter()
        .filter(|&&x| x == i16::MIN || x == i16::MAX)
        .count();
    eprintln!(
        "converted alignment declared={delay} best={} diagnostic_snr={} clips={clips} signal={energy} error={}",
        best.1,
        10.0 * (energy / best.0).log10(),
        best.0
    );
}

fn converted_input(speech: &[i16], length: usize, channels: usize, noisy: bool) -> Vec<f32> {
    let mut random = 1_u32;
    (0..length)
        .flat_map(|i| {
            let mut frame = Vec::with_capacity(channels);
            for channel in 0..channels {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let sample = if noisy {
                    (random as i32 as f32) / 2_147_483_648.0 * 0.18
                } else {
                    f32::from(speech[i + channel * 31]) / 32768.0
                };
                frame.push(sample);
            }
            frame
        })
        .collect()
}
