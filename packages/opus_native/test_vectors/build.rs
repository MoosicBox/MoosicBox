use moosicbox_opus_native_libopus::safe::{Decoder, Encoder, OpusError};
use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, OPUS_APPLICATION_VOIP};
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

    generate_silk_nb_mono(&generated_dir).expect("Failed to generate SILK NB mono vector");
    generate_celt_fb_mono(&generated_dir).expect("Failed to generate CELT FB mono vector");
    generate_integration_stereo(&generated_dir)
        .expect("Failed to generate integration stereo vector");
}

fn generate_silk_nb_mono(base: &Path) -> Result<(), OpusError> {
    let dir = base.join("silk/nb/basic_mono");
    let sample_rate = 8000;
    let channels = 1;
    let frame_size = 160;

    let mut encoder = Encoder::new(sample_rate, channels, OPUS_APPLICATION_VOIP)?;
    let mut decoder = Decoder::new(sample_rate, channels)?;

    // Use simple impulse for testing
    let mut input_pcm = vec![0i16; frame_size];
    input_pcm[0] = 1000;
    input_pcm[10] = -500;
    input_pcm[20] = 2000;
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": 20,
  "mode": "silk"
}}"#,
        sample_rate, channels
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}

fn generate_celt_fb_mono(base: &Path) -> Result<(), OpusError> {
    let dir = base.join("celt/fb/basic_mono");
    let sample_rate = 48000;
    let channels = 1;
    let frame_size = 480;

    let mut encoder = Encoder::new(sample_rate, channels, OPUS_APPLICATION_AUDIO)?;
    let mut decoder = Decoder::new(sample_rate, channels)?;

    let input_pcm = vec![0i16; frame_size];
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": 10,
  "mode": "celt"
}}"#,
        sample_rate, channels
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}

fn generate_integration_stereo(base: &Path) -> Result<(), OpusError> {
    let dir = base.join("integration/basic_stereo");
    let sample_rate = 48000;
    let channels = 2;
    let frame_size = 960;

    let mut encoder = Encoder::new(sample_rate, channels, OPUS_APPLICATION_AUDIO)?;
    let mut decoder = Decoder::new(sample_rate, channels)?;

    let input_pcm = vec![0i16; frame_size * 2];
    let mut packet = vec![0u8; 4000];

    let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet)?;
    packet.truncate(packet_len);

    fs::write(dir.join("packet.bin"), &packet).expect("Failed to write packet.bin");

    let mut output_pcm = vec![0i16; frame_size * 2];
    let decoded_samples = decoder.decode(&packet, &mut output_pcm, frame_size, false)?;

    output_pcm.truncate(decoded_samples * 2);
    let pcm_bytes: Vec<u8> = output_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join("expected.pcm"), &pcm_bytes).expect("Failed to write expected.pcm");

    let meta = format!(
        r#"{{
  "sample_rate": {},
  "channels": {},
  "frame_size_ms": 20,
  "mode": "hybrid"
}}"#,
        sample_rate, channels
    );
    fs::write(dir.join("meta.json"), meta).expect("Failed to write meta.json");

    Ok(())
}
