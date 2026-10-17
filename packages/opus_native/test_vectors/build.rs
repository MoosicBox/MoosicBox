use moosicbox_opus_native_libopus::safe::{Decoder, Encoder, OpusError};
use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, OPUS_APPLICATION_VOIP};
use std::fs;
#[path = "src/format.rs"]
mod format;
#[path = "src/frozen.rs"]
mod frozen;
use moosicbox_opus_native_libopus::ReferenceMode;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/format.rs");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let generated_dir = out_dir.join("generated");

    generate_all_silk_vectors(&generated_dir).expect("Failed to generate SILK vectors");
    generate_all_celt_vectors(&generated_dir).expect("Failed to generate CELT vectors");
    generate_all_hybrid_vectors(&generated_dir).expect("Failed to generate Hybrid vectors");
    generate_sequences(&generated_dir).expect("Failed to generate stateful sequences");
    write_inventory(&generated_dir);
    verify_frozen_core(&generated_dir);
    println!("cargo:rerun-if-changed=frozen");
    println!("cargo:rerun-if-changed=src/frozen.rs");
    frozen::verify_frozen_corpus(Path::new("frozen/corpus"))
        .expect("Source frozen corpus mismatch");
    frozen::verify_frozen_corpus(&generated_dir)
        .expect("Generated mandatory corpus changed: independent review required");
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
    generate_vector(
        base,
        path,
        sample_rate,
        channels,
        frame_size,
        signal_type,
        ReferenceMode::Silk,
    )
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
    generate_vector(
        base,
        path,
        sample_rate,
        channels,
        frame_size,
        signal_type,
        ReferenceMode::Celt,
    )
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
    generate_vector(
        base,
        path,
        sample_rate,
        channels,
        frame_size,
        signal_type,
        ReferenceMode::Hybrid,
    )
}

// Existing named signals remain unchanged to retain useful historical cases.
// New stateful baselines use only integer arithmetic and deliberately distinct channels.
fn integer_signal(frame_size: usize, channels: usize, packet_index: usize) -> Vec<i16> {
    (0..frame_size * channels)
        .map(|i| {
            let channel = i % channels;
            let n = i / channels + packet_index * frame_size;
            let triangle = ((n * (37 + channel * 16)) % 2048) as i32;
            let triangle = if triangle < 1024 {
                triangle
            } else {
                2047 - triangle
            };
            ((triangle - 512) * 12) as i16
        })
        .collect()
}

fn settings(mode: ReferenceMode, sample_rate: i32) -> (&'static str, &'static str, i32, i32) {
    let (name, bitrate) = match mode {
        ReferenceMode::Silk => ("silk", 16000),
        ReferenceMode::Hybrid => ("hybrid", 32000),
        ReferenceMode::Celt => ("celt", 64000),
    };
    let (bandwidth, ctl) = match sample_rate {
        8000 => ("nb", 1101),
        12000 => ("mb", 1102),
        16000 => ("wb", 1103),
        24000 if matches!(mode, ReferenceMode::Silk) => ("wb", 1103),
        24000 => ("swb", 1104),
        48000 => ("fb", 1105),
        _ => panic!("Unsupported sample rate"),
    };
    (name, bandwidth, ctl, bitrate)
}

#[allow(clippy::too_many_arguments)]
fn write_packet(
    dir: &Path,
    encoder: &mut Encoder,
    decoder: &mut Decoder,
    input: &[i16],
    mode: ReferenceMode,
    sample_rate: i32,
    channels: i32,
    frame_size: usize,
    packet_index: usize,
) -> Result<(), OpusError> {
    fs::create_dir_all(dir).expect("create vector directory");
    let mut packet = vec![0; 4000];
    let length = encoder.encode(input, frame_size, &mut packet)?;
    packet.truncate(length);
    let (name, bandwidth, _, bitrate) = settings(mode, sample_rate);
    let revision = format::REFERENCE_REVISION;
    let reference_config = format::REFERENCE_CONFIG;
    format::validate_toc(
        &packet,
        name,
        bandwidth,
        sample_rate as u32,
        channels as u8,
        frame_size,
    )
    .unwrap_or_else(|error| panic!("{}: {error}, TOC={:02x}", dir.display(), packet[0]));
    let mut pcm = vec![0; frame_size * channels as usize];
    let decoded = decoder.decode(&packet, &mut pcm, frame_size, false)?;
    assert_eq!(decoded, frame_size, "Reference duration mismatch");
    let pcm_bytes: Vec<_> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("packet.bin"), &packet).expect("write packet");
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("write PCM");
    fs::write(
        dir.join("meta.json"),
        format!(
            r#"{{
      "format_version": 2, "sample_rate": {sample_rate}, "channels": {channels},
      "frame_size_ms": {}, "frame_samples": {frame_size},
      "mode": "{name}", "bandwidth": "{bandwidth}",
      "pcm_semantics": "raw_decoder_output", "packet_index": {packet_index},
      "decoder_state": "{}", "reference_revision": "{revision}",
      "reference_config": "{reference_config}",
      "bitrate": {bitrate}, "packet_fnv1a64": "{}", "pcm_fnv1a64": "{}"
    }}"#,
            frame_size * 1000 / sample_rate as usize,
            if packet_index == 0 {
                "reset"
            } else {
                "continued"
            },
            format::checksum(&packet),
            format::checksum(&pcm_bytes)
        ),
    )
    .expect("write metadata");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn generate_vector(
    base: &Path,
    path: &str,
    sample_rate: i32,
    channels: i32,
    frame_size: usize,
    signal_type: SignalType,
    mode: ReferenceMode,
) -> Result<(), OpusError> {
    let application = if matches!(mode, ReferenceMode::Silk) {
        OPUS_APPLICATION_VOIP
    } else {
        OPUS_APPLICATION_AUDIO
    };
    let mut encoder = Encoder::new(sample_rate as u32, channels as u8, application)?;
    let (_, _, bandwidth, bitrate) = settings(mode, sample_rate);
    encoder.configure_reference(mode, bandwidth, bitrate)?;
    let mut decoder = Decoder::new(sample_rate as u32, channels as u8)?;
    let input = generate_signal(signal_type, sample_rate, frame_size, channels);
    write_packet(
        &base.join(path),
        &mut encoder,
        &mut decoder,
        &input,
        mode,
        sample_rate,
        channels,
        frame_size,
        0,
    )
}

fn generate_sequences(base: &Path) -> Result<(), OpusError> {
    for (path, mode, sample_rate) in [
        ("sequences/celt_fb_mono", ReferenceMode::Celt, 48000),
        ("sequences/celt_fb_stereo", ReferenceMode::Celt, 48000),
        ("sequences/hybrid_fb_mono", ReferenceMode::Hybrid, 48000),
        ("sequences/hybrid_swb_stereo", ReferenceMode::Hybrid, 24000),
        ("sequences/silk_wb_mono", ReferenceMode::Silk, 16000),
    ] {
        let channels = if path.ends_with("stereo") { 2 } else { 1 };
        let frame_size = sample_rate as usize / 50;
        let mut encoder = Encoder::new(sample_rate as u32, channels as u8, OPUS_APPLICATION_AUDIO)?;
        let (_, _, bandwidth, bitrate) = settings(mode, sample_rate);
        encoder.configure_reference(mode, bandwidth, bitrate)?;
        let mut decoder = Decoder::new(sample_rate as u32, channels as u8)?;
        let dir = base.join(path);
        for index in 0..4 {
            let input = integer_signal(frame_size, channels as usize, index);
            let packet_dir = if index == 0 {
                dir.clone()
            } else {
                dir.join(format!("packet_{index:03}"))
            };
            write_packet(
                &packet_dir,
                &mut encoder,
                &mut decoder,
                &input,
                mode,
                sample_rate,
                channels,
                frame_size,
                index,
            )?;
        }
        fs::write(
            dir.join("sequence.json"),
            r#"{
          "format_version": 1, "decoder_state": "persistent", "pcm_semantics": "raw_decoder_output",
          "packets": [".", "packet_001", "packet_002", "packet_003"],
          "signal": "integer_triangle_v1"
        }"#,
        )
        .expect("write sequence manifest");
    }
    Ok(())
}

fn write_inventory(base: &Path) {
    fn visit(base: &Path, dir: &Path, cases: &mut Vec<String>) {
        if dir.join("sequence.json").exists() || dir.join("meta.json").exists() {
            let path = dir.strip_prefix(base).unwrap().to_str().unwrap();
            let sequence = dir.join("sequence.json").exists();
            cases.push(format!(
                r#"{{"id":"{path}","path":"{path}","kind":"{}","packet_count":{}}}"#,
                if sequence { "sequence" } else { "single" },
                if sequence { 4 } else { 1 }
            ));
            return;
        }
        let mut dirs: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for child in dirs {
            visit(base, &child, cases);
        }
    }
    let mut cases = Vec::new();
    visit(base, base, &mut cases);
    fs::write(
        base.join("inventory.json"),
        format!(r#"{{"format_version":1,"cases":[{}]}}"#, cases.join(",")),
    )
    .unwrap();
    fs::write(
        base.join("provenance.json"),
        r#"{
      "format_version":1,"reference_revision":"34bba701ae97c913de719b1f7c10686f62cddb15",
      "generator":"moosicbox_opus_native_test_vectors/build.rs:v2",
      "fixed_point":true,"intrinsics":false,"float_api":false,
      "pcm_semantics":"raw_decoder_output","checksum_algorithm":"fnv1a64"
    }"#,
    )
    .unwrap();
}

// Regeneration must agree with reviewed, source-controlled oracle bytes.
fn verify_frozen_core(generated: &Path) {
    fn compare(source: &Path, output: &Path) {
        for index in 0..4 {
            let relative = if index == 0 {
                PathBuf::new()
            } else {
                PathBuf::from(format!("packet_{index:03}"))
            };
            for file in ["packet.bin", "expected.pcm", "meta.json"] {
                let path = relative.join(file);
                assert!(
                    fs::read(source.join(&path)).unwrap() == fs::read(output.join(&path)).unwrap(),
                    "Frozen reference changed: {}. Explicit reviewed regeneration required; see frozen/README.md",
                    output.join(path).display()
                );
            }
        }
        assert_eq!(
            fs::read(source.join("sequence.json")).unwrap(),
            fs::read(output.join("sequence.json")).unwrap()
        );
    }
    let frozen = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("frozen");
    println!("cargo:rerun-if-changed=frozen/");
    for name in ["celt_fb_mono", "celt_fb_stereo", "hybrid_fb_mono"] {
        compare(&frozen.join(name), &generated.join("sequences").join(name));
    }
}
