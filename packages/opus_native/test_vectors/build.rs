use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let generated_dir = out_dir.join("generated");

    fs::create_dir_all(generated_dir.join("silk/nb/basic_mono"))
        .expect("Failed to create silk/nb/basic_mono directory");
    fs::create_dir_all(generated_dir.join("celt/fb/basic_mono"))
        .expect("Failed to create celt/fb/basic_mono directory");
    fs::create_dir_all(generated_dir.join("integration/basic_stereo"))
        .expect("Failed to create integration/basic_stereo directory");

    create_silk_nb_mono(&generated_dir);
    create_celt_fb_mono(&generated_dir);
    create_integration_stereo(&generated_dir);
}

fn create_silk_nb_mono(base: &Path) {
    let dir = base.join("silk/nb/basic_mono");

    let toc = 0x00_u8;
    let frame_length = 20;
    let mut packet = vec![toc];
    packet.extend(vec![0x00; frame_length]);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let sample_rate = 8000_u32;
    let frame_duration_ms = 20;
    let samples_per_frame = (sample_rate * frame_duration_ms) / 1000;

    let samples: Vec<i16> = vec![0; samples_per_frame as usize];
    let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": 1,
  "frame_size_ms": {},
  "mode": "silk"
}}"#,
        sample_rate, frame_duration_ms
    );

    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");
}

fn create_celt_fb_mono(base: &Path) {
    let dir = base.join("celt/fb/basic_mono");

    let toc = 0xF8_u8;
    let frame_length = 40;
    let mut packet = vec![toc];
    packet.extend(vec![0x00; frame_length]);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let sample_rate = 48000_u32;
    let frame_duration_ms = 10;
    let samples_per_frame = (sample_rate * frame_duration_ms) / 1000;

    let samples: Vec<i16> = vec![0; samples_per_frame as usize];
    let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": 1,
  "frame_size_ms": {},
  "mode": "celt"
}}"#,
        sample_rate, frame_duration_ms
    );

    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");
}

fn create_integration_stereo(base: &Path) {
    let dir = base.join("integration/basic_stereo");

    let toc = 0xFC_u8;
    let frame_length = 60;
    let mut packet = vec![toc];
    packet.extend(vec![0x00; frame_length]);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let sample_rate = 48000_u32;
    let channels = 2;
    let frame_duration_ms = 20;
    let samples_per_frame = (sample_rate * frame_duration_ms) / 1000;
    let total_samples = samples_per_frame * channels;

    let samples: Vec<i16> = vec![0; total_samples as usize];
    let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": {},
  "mode": "celt"
}}"#,
        sample_rate, channels, frame_duration_ms
    );

    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");
}
