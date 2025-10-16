use moosicbox_opus_native_libopus::safe::{Decoder, Encoder, OpusError};
use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, OPUS_APPLICATION_VOIP};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let generated_dir = out_dir.join("generated");

    generate_all_silk_vectors(&generated_dir).expect("Failed to generate SILK vectors");
    generate_all_celt_vectors(&generated_dir).expect("Failed to generate CELT vectors");
    generate_all_hybrid_vectors(&generated_dir).expect("Failed to generate Hybrid vectors");
}

fn generate_all_silk_vectors(base: &Path) -> Result<(), OpusError> {
    generate_silk_nb_vectors(base)?;
    generate_silk_mb_vectors(base)?;
    generate_silk_wb_vectors(base)?;
    generate_silk_swb_vectors(base)?;
    Ok(())
}

fn generate_silk_nb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 8000;
    let frame_size = 160;

    generate_silk_vector(
        base,
        "silk/nb/impulse_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Impulse,
    )?;
    generate_silk_vector(
        base,
        "silk/nb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 400.0 },
    )?;
    generate_silk_vector(
        base,
        "silk/nb/noise_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::WhiteNoise,
    )?;
    generate_silk_vector(
        base,
        "silk/nb/silence_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Silence,
    )?;
    generate_silk_vector(
        base,
        "silk/nb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 300.0 },
    )?;
    generate_silk_vector(
        base,
        "silk/nb/noise_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::WhiteNoise,
    )?;
    generate_silk_vector(
        base,
        "silk/nb/mixed_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Mixed,
    )?;
    generate_silk_vector(
        base,
        "silk/nb/quiet_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::QuietSine,
    )?;

    Ok(())
}

fn generate_silk_mb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 12000;
    let frame_size = 240;

    generate_silk_vector(
        base,
        "silk/mb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 500.0 },
    )?;
    generate_silk_vector(
        base,
        "silk/mb/noise_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::WhiteNoise,
    )?;
    generate_silk_vector(
        base,
        "silk/mb/silence_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Silence,
    )?;
    generate_silk_vector(
        base,
        "silk/mb/mixed_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Mixed,
    )?;

    Ok(())
}

fn generate_silk_wb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 16000;
    let frame_size = 320;

    generate_silk_vector(
        base,
        "silk/wb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 800.0 },
    )?;
    generate_silk_vector(
        base,
        "silk/wb/noise_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::WhiteNoise,
    )?;
    generate_silk_vector(
        base,
        "silk/wb/silence_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Silence,
    )?;
    generate_silk_vector(
        base,
        "silk/wb/mixed_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Mixed,
    )?;

    Ok(())
}

fn generate_silk_swb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 24000;
    let frame_size = 480;

    generate_silk_vector(
        base,
        "silk/swb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 1000.0 },
    )?;
    generate_silk_vector(
        base,
        "silk/swb/mixed_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Mixed,
    )?;

    Ok(())
}

#[derive(Clone, Copy)]
enum SignalType {
    Impulse,
    Sine { freq_hz: f32 },
    WhiteNoise,
    Silence,
    Mixed,
    QuietSine,
}

fn generate_signal(
    signal_type: SignalType,
    sample_rate: i32,
    num_samples: usize,
    channels: i32,
) -> Vec<i16> {
    let total_samples = num_samples * channels as usize;
    match signal_type {
        SignalType::Impulse => generate_impulse(total_samples),
        SignalType::Sine { freq_hz } => {
            generate_sine_wave(freq_hz, sample_rate, num_samples, channels)
        }
        SignalType::WhiteNoise => generate_white_noise(total_samples),
        SignalType::Silence => vec![0i16; total_samples],
        SignalType::Mixed => generate_mixed_signal(sample_rate, num_samples, channels),
        SignalType::QuietSine => generate_quiet_sine(sample_rate, num_samples, channels),
    }
}

fn generate_impulse(num_samples: usize) -> Vec<i16> {
    let mut signal = vec![0i16; num_samples];
    if num_samples > 0 {
        signal[0] = 1000;
    }
    if num_samples > 10 {
        signal[10] = -500;
    }
    if num_samples > 20 {
        signal[20] = 2000;
    }
    signal
}

fn generate_sine_wave(
    freq_hz: f32,
    sample_rate: i32,
    num_samples: usize,
    channels: i32,
) -> Vec<i16> {
    let mut signal = Vec::with_capacity(num_samples * channels as usize);
    let amplitude = 8000.0;

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample_value = (amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin()) as i16;

        for _ in 0..channels {
            signal.push(sample_value);
        }
    }

    signal
}

fn generate_white_noise(num_samples: usize) -> Vec<i16> {
    let mut signal = Vec::with_capacity(num_samples);
    let mut seed: u32 = 12345;
    let amplitude = 2000.0;

    for _ in 0..num_samples {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let normalized = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        signal.push((normalized * amplitude) as i16);
    }

    signal
}

fn generate_mixed_signal(sample_rate: i32, num_samples: usize, channels: i32) -> Vec<i16> {
    let sine_part = generate_sine_wave(440.0, sample_rate, num_samples / 2, channels);
    let noise_part = generate_white_noise((num_samples / 2) * channels as usize);

    let mut signal = Vec::with_capacity(num_samples * channels as usize);
    signal.extend_from_slice(&sine_part);
    signal.extend_from_slice(&noise_part);

    signal
}

fn generate_quiet_sine(sample_rate: i32, num_samples: usize, channels: i32) -> Vec<i16> {
    let mut signal = Vec::with_capacity(num_samples * channels as usize);
    let amplitude = 500.0;
    let freq_hz = 300.0;

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample_value = (amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin()) as i16;

        for _ in 0..channels {
            signal.push(sample_value);
        }
    }

    signal
}

fn generate_silk_vector(
    base: &Path,
    path: &str,
    sample_rate: i32,
    channels: i32,
    frame_size: usize,
    signal_type: SignalType,
) -> Result<(), OpusError> {
    let dir = base.join(path);
    fs::create_dir_all(&dir).expect("Failed to create directory");

    let mut encoder = Encoder::new(sample_rate as u32, channels as u8, OPUS_APPLICATION_VOIP)?;
    let mut decoder = Decoder::new(sample_rate as u32, channels as u8)?;

    let input_pcm = generate_signal(signal_type, sample_rate, frame_size, channels);
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size * channels as usize];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples * channels as usize);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let frame_size_ms = (frame_size * 1000) / sample_rate as usize;
    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": {},
  "mode": "silk"
}}"#,
        sample_rate, channels, frame_size_ms
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}

fn generate_all_celt_vectors(base: &Path) -> Result<(), OpusError> {
    generate_celt_nb_vectors(base)?;
    generate_celt_wb_vectors(base)?;
    generate_celt_swb_vectors(base)?;
    generate_celt_fb_vectors(base)?;
    Ok(())
}

fn generate_celt_nb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 8000;
    let frame_size = 80;

    generate_celt_vector(
        base,
        "celt/nb/impulse_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Impulse,
    )?;
    generate_celt_vector(
        base,
        "celt/nb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 300.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/nb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 250.0 },
    )?;

    Ok(())
}

fn generate_celt_wb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 16000;
    let frame_size = 160;

    generate_celt_vector(
        base,
        "celt/wb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 600.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/wb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 500.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/wb/noise_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::WhiteNoise,
    )?;

    Ok(())
}

fn generate_celt_swb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 24000;
    let frame_size = 240;

    generate_celt_vector(
        base,
        "celt/swb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 1000.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/swb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 900.0 },
    )?;

    Ok(())
}

fn generate_celt_fb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 48000;
    let frame_size = 480;

    generate_celt_vector(
        base,
        "celt/fb/silence_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Silence,
    )?;
    generate_celt_vector(
        base,
        "celt/fb/impulse_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Impulse,
    )?;
    generate_celt_vector(
        base,
        "celt/fb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 1200.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/fb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 1100.0 },
    )?;
    generate_celt_vector(
        base,
        "celt/fb/noise_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::WhiteNoise,
    )?;

    Ok(())
}

fn generate_celt_vector(
    base: &Path,
    path: &str,
    sample_rate: i32,
    channels: i32,
    frame_size: usize,
    signal_type: SignalType,
) -> Result<(), OpusError> {
    let dir = base.join(path);
    fs::create_dir_all(&dir).expect("Failed to create directory");

    let mut encoder = Encoder::new(sample_rate as u32, channels as u8, OPUS_APPLICATION_AUDIO)?;
    let mut decoder = Decoder::new(sample_rate as u32, channels as u8)?;

    let input_pcm = generate_signal(signal_type, sample_rate, frame_size, channels);
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size * channels as usize];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples * channels as usize);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let frame_size_ms = (frame_size * 1000) / sample_rate as usize;
    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": {},
  "mode": "celt"
}}"#,
        sample_rate, channels, frame_size_ms
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}

fn generate_all_hybrid_vectors(base: &Path) -> Result<(), OpusError> {
    generate_hybrid_swb_vectors(base)?;
    generate_hybrid_fb_vectors(base)?;
    Ok(())
}

fn generate_hybrid_swb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 24000;
    let frame_size = 480;

    generate_hybrid_vector(
        base,
        "hybrid/swb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 1000.0 },
    )?;
    generate_hybrid_vector(
        base,
        "hybrid/swb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 900.0 },
    )?;

    Ok(())
}

fn generate_hybrid_fb_vectors(base: &Path) -> Result<(), OpusError> {
    let sample_rate = 48000;
    let frame_size = 960;

    generate_hybrid_vector(
        base,
        "hybrid/fb/silence_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Silence,
    )?;
    generate_hybrid_vector(
        base,
        "hybrid/fb/silence_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Silence,
    )?;
    generate_hybrid_vector(
        base,
        "hybrid/fb/sine_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Sine { freq_hz: 1200.0 },
    )?;
    generate_hybrid_vector(
        base,
        "hybrid/fb/sine_stereo",
        sample_rate,
        2,
        frame_size,
        SignalType::Sine { freq_hz: 1100.0 },
    )?;
    generate_hybrid_vector(
        base,
        "hybrid/fb/mixed_mono",
        sample_rate,
        1,
        frame_size,
        SignalType::Mixed,
    )?;

    Ok(())
}

fn generate_hybrid_vector(
    base: &Path,
    path: &str,
    sample_rate: i32,
    channels: i32,
    frame_size: usize,
    signal_type: SignalType,
) -> Result<(), OpusError> {
    let dir = base.join(path);
    fs::create_dir_all(&dir).expect("Failed to create directory");

    let mut encoder = Encoder::new(sample_rate as u32, channels as u8, OPUS_APPLICATION_AUDIO)?;
    let mut decoder = Decoder::new(sample_rate as u32, channels as u8)?;

    let input_pcm = generate_signal(signal_type, sample_rate, frame_size, channels);
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size * channels as usize];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples * channels as usize);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let frame_size_ms = (frame_size * 1000) / sample_rate as usize;
    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": {},
  "mode": "hybrid"
}}"#,
        sample_rate, channels, frame_size_ms
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}
