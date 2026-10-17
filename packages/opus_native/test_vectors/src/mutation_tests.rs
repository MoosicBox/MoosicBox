use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            ".mutation-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = test_vectors_dir().join("sequences/celt_fb_stereo");
        fs::copy(source.join("sequence.json"), root.join("sequence.json")).unwrap();
        for index in 0..4 {
            let part = if index == 0 {
                PathBuf::new()
            } else {
                PathBuf::from(format!("packet_{index:03}"))
            };
            fs::create_dir_all(root.join(&part)).unwrap();
            for file in ["packet.bin", "expected.pcm", "meta.json"] {
                fs::copy(source.join(&part).join(file), root.join(&part).join(file)).unwrap();
            }
        }
        assert!(TestSequence::load(&root).is_ok());
        Self(root)
    }
    fn mutate(&self, relative: &str, field: &str, value: Value) {
        let path = self.0.join(relative);
        let mut meta: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        meta[field] = value;
        fs::write(path, serde_json::to_vec(&meta).unwrap()).unwrap();
    }
    fn rejected(&self) {
        let result = std::panic::catch_unwind(|| TestSequence::load(&self.0).is_err());
        assert!(
            matches!(result, Ok(true)),
            "Mutation must return an error, not panic or pass"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn valid_packets_reject_metadata_mutations_without_panics() {
    for (field, value) in [
        ("frame_samples", json!(u64::MAX)),
        ("frame_samples", json!(0)),
        ("frame_size_ms", json!(u64::MAX)),
        ("frame_size_ms", json!(999)),
        ("reference_revision", json!("changed")),
        ("reference_revision", Value::Null),
        ("reference_config", json!("floating-point")),
        ("bitrate", json!(1)),
        ("sample_rate", json!(44100)),
        ("sample_rate", json!(u64::MAX)),
        ("channels", json!(0)),
        ("channels", json!(1)),
        ("channels", json!(u64::MAX)),
        ("packet_index", json!(0)),
        ("decoder_state", json!("reset")),
        ("packet_fnv1a64", json!("bad")),
        ("pcm_fnv1a64", json!("bad")),
        ("format_version", json!(99)),
        ("pcm_semantics", json!("trimmed")),
    ] {
        let fixture = Fixture::new();
        fixture.mutate("packet_001/meta.json", field, value);
        fixture.rejected();
    }
    assert!(validate_toc(&[0xf8], "celt", "fb", 48000, 1, usize::MAX).is_err());
}

#[test]
fn valid_sequences_reject_order_state_and_byte_mutations() {
    for packets in [
        json!([]),
        json!([".", "packet_002"]),
        json!([".", "."]),
        json!(["../escape"]),
    ] {
        let fixture = Fixture::new();
        fixture.mutate("sequence.json", "packets", packets);
        fixture.rejected();
    }
    for (field, value) in [
        ("decoder_state", json!("reset")),
        ("format_version", json!(2)),
        ("pcm_semantics", json!("trimmed")),
    ] {
        let fixture = Fixture::new();
        fixture.mutate("sequence.json", field, value);
        fixture.rejected();
    }
    for file in ["expected.pcm", "packet.bin"] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("packet_001").join(file), [0]).unwrap();
        fixture.rejected();
    }
}

#[test]
fn individually_valid_packets_cannot_change_sequence_output_configuration() {
    for id in ["celt_fb_mono", "hybrid_swb_stereo"] {
        let fixture = Fixture::new();
        let source = test_vectors_dir()
            .join("sequences")
            .join(id)
            .join("packet_001");
        assert!(TestVector::load(&source).is_ok());
        for file in ["packet.bin", "expected.pcm", "meta.json"] {
            fs::copy(source.join(file), fixture.0.join("packet_001").join(file)).unwrap();
        }
        fixture.rejected();
    }
}
