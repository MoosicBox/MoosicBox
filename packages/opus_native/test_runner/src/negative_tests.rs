//! Harness rejection tests use synthetic fixtures, never codec acceptance expectations.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            ".feedback-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("celt/fb/silence_mono")).unwrap();
        let fixture = Self(path);
        for name in ["packet.bin", "expected.pcm", "meta.json"] {
            fixture.write(
                name,
                &read(&test_vectors_dir().join("celt/fb/silence_mono").join(name)).unwrap(),
            );
        }
        fixture
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        fs::write(self.0.join("celt/fb/silence_mono").join(name), bytes).unwrap();
    }
    fn entry() -> corpus::Entry {
        corpus::Entry {
            path: "celt/fb/silence_mono".into(),
            sequence: false,
            count: 1,
        }
    }
    fn record(&self) -> Value {
        case(&self.0, "celt/fb/silence_mono", &Self::entry())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn missing_empty_odd_and_mislabeled_fixtures_are_harness_failures() {
    for (name, bytes) in [
        ("packet.bin", vec![]),
        ("expected.pcm", vec![]),
        ("expected.pcm", vec![0]),
        ("packet.bin", vec![0]),
    ] {
        let fixture = Fixture::new();
        // Prove the seed fixture passes validation before introducing the fault.
        assert!(load(&fixture.0, "celt/fb/silence_mono").is_ok());
        fixture.write(name, &bytes);
        assert_eq!(fixture.record()["status"], "harness_failure");
    }
    let fixture = Fixture::new();
    fs::remove_file(fixture.0.join("celt/fb/silence_mono/packet.bin")).unwrap();
    assert_eq!(fixture.record()["status"], "harness_failure");
}

#[test]
fn missing_or_empty_inventory_and_provenance_are_rejected() {
    let fixture = Fixture::new();
    assert!(corpus::inventory(&fixture.0).is_err());
    fs::write(
        fixture.0.join("inventory.json"),
        r#"{"format_version":1,"cases":[]}"#,
    )
    .unwrap();
    fs::write(fixture.0.join("provenance.json"), r#"{"format_version":1}"#).unwrap();
    assert!(corpus::inventory(&fixture.0).is_err());
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn valid_nonempty_inventory_cannot_drop_or_reclassify_mandatory_cases() {
    let fixture = Fixture::new();
    let original: Value =
        serde_json::from_slice(&read(&test_vectors_dir().join("inventory.json")).unwrap()).unwrap();
    fs::copy(
        test_vectors_dir().join("provenance.json"),
        fixture.0.join("provenance.json"),
    )
    .unwrap();
    let write =
        |value: &Value| fs::write(fixture.0.join("inventory.json"), value.to_string()).unwrap();
    write(&original);
    assert!(corpus::inventory(&fixture.0).is_ok());
    for key in ["reference_revision", "generator"] {
        let file = fixture.0.join("provenance.json");
        let original_provenance = read(&test_vectors_dir().join("provenance.json")).unwrap();
        let mut changed: Value = serde_json::from_slice(&original_provenance).unwrap();
        changed[key] = json!("forged");
        fs::write(&file, changed.to_string()).unwrap();
        write(&original);
        assert!(corpus::inventory(&fixture.0).is_err());
        fs::write(&file, original_provenance).unwrap();
    }
    for mutation in 0..4 {
        let mut changed = original.clone();
        let entries = changed["cases"].as_array_mut().unwrap();
        match mutation {
            0 => entries.retain(|entry| entry["id"] == "celt/fb/silence_mono"),
            1 => entries.retain(|entry| entry["kind"] != "sequence"),
            2 => {
                let entry = entries
                    .iter_mut()
                    .find(|entry| entry["kind"] == "sequence")
                    .unwrap();
                entry["kind"] = json!("single");
                entry["packet_count"] = json!(1);
            }
            _ => {
                entries
                    .iter_mut()
                    .find(|entry| entry["kind"] == "sequence")
                    .unwrap()["packet_count"] = json!(3);
            }
        }
        write(&changed);
        assert!(
            corpus::inventory(&fixture.0).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn sequence_corruptions_cannot_be_codec_passes() {
    let id = "sequences/celt_fb_mono";
    let entry = corpus::Entry {
        path: id.into(),
        sequence: true,
        count: 4,
    };
    for mutation in 0..7 {
        let fixture = Fixture::new();
        let path = fixture.0.join(id);
        copy_tree(&test_vectors_dir().join(id), &path);
        assert!(corpus::packets(&fixture.0, id, &entry).is_ok());
        match mutation {
            0 => fs::remove_file(path.join("packet_002/packet.bin")).unwrap(),
            1 => fs::write(path.join("packet_001/expected.pcm"), []).unwrap(),
            2 => {
                let file = path.join("packet_003/meta.json");
                let mut meta: Value = serde_json::from_slice(&read(&file).unwrap()).unwrap();
                meta["frame_samples"] = json!(u64::MAX);
                fs::write(file, meta.to_string()).unwrap();
            }
            3 => {
                let file = path.join("sequence.json");
                let mut manifest: Value = serde_json::from_slice(&read(&file).unwrap()).unwrap();
                manifest["packets"].as_array_mut().unwrap().pop();
                fs::write(file, manifest.to_string()).unwrap();
            }
            4 => {
                let file = path.join("packet_001/meta.json");
                let mut meta: Value = serde_json::from_slice(&read(&file).unwrap()).unwrap();
                meta["decoder_state"] = json!("reset");
                fs::write(file, meta.to_string()).unwrap();
            }
            5 | 6 => {
                let file = path.join("packet_001/meta.json");
                let mut meta: Value = serde_json::from_slice(&read(&file).unwrap()).unwrap();
                if mutation == 5 {
                    meta["frame_size_ms"] = json!(1);
                } else {
                    meta["reference_revision"] = json!("0".repeat(40));
                }
                fs::write(file, meta.to_string()).unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            case(&fixture.0, id, &entry)["status"],
            "harness_failure",
            "mutation {mutation}"
        );
    }
}

fn baseline_document() -> Vec<Value> {
    let id = "celt/fb/silence_mono";
    let packet = {
        let mut packet = compare(&[0, 0], &[0, 0]);
        packet["actual_pcm_sha256"] = json!("a".repeat(64));
        packet["packet_index"] = json!(0);
        packet
    };
    let identity = output_identity(std::slice::from_ref(&packet));
    vec![
        json!({"type":"run","frozen_manifest_sha256":FROZEN_MANIFEST_SHA256,"git_head":"e".repeat(40),"schema":3,"mode":"diagnostic","source_sha256":"a".repeat(64),"executable_sha256":"b".repeat(64),"build_profile":"debug_assertions","comparison_contract":"raw_exact_pcm_packet_identity_v2","root":"fixture","source_identity_scope":"fixture","coverage":"decode","legacy_compensated_checks":"not_exercised","features":{"silk":true,"celt":true,"hybrid":true,"resampling":false},"reference_provenance":serde_json::from_slice::<Value>(&read(&test_vectors_dir().join("provenance.json")).unwrap()).unwrap()}),
        json!({"type":"case","id":id,"kind":"single","status":"pass","comparison":"exact_uncompensated_pcm","reproduce":["--case",id],"fixture_sha256":"c".repeat(64),"actual_pcm_sha256":identity,"exercised":true,"mode":"celt","packets":[packet],"different_common_samples":0,"unpaired_samples":0}),
        json!({"type":"summary","statuses":{"pass":1},"scope":"focused","success":true,"selected_cases_pass":false,"whole_corpus_pass":false,"corpus_sha256":"d".repeat(64),"baseline_cases_not_selected":[]}),
    ]
}
fn parse_document(document: &[Value]) -> Result<baseline::Baseline> {
    baseline::parse(
        &document
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[test]
fn forged_baseline_schema_status_pcm_and_scope_are_rejected() {
    let original = baseline_document();
    assert!(parse_document(&original).is_ok());
    for mutation in 0..8 {
        let mut document = original.clone();
        match mutation {
            0 => document[0] = json!({"type":"run","schema":1}),
            1 => {
                document[1]["status"] = json!("invented_status");
                document[2]["statuses"] = json!({"invented_status":1});
            }
            2 => document[1]["actual_pcm_sha256"] = Value::Null,
            3 => document[1]["packets"][0]["actual_pcm_sha256"] = Value::Null,
            4 => document[1]["exercised"] = json!(false),
            5 => document[0]["features"] = Value::Null,
            6 => document[2]["scope"] = json!("all_cases"),
            _ => document[1]["packets"][0]["actual_samples"] = json!(1),
        }
        assert!(parse_document(&document).is_err(), "mutation {mutation}");
    }
}

#[test]
fn configuration_changes_and_missing_pcm_are_incomparable() {
    let header = baseline_document().remove(0);
    for key in [
        "features",
        "build_profile",
        "reference_provenance",
        "comparison_contract",
    ] {
        let mut changed = header.clone();
        changed[key] = Value::Null;
        assert!(!baseline::compatible(&header, &changed));
    }
    let before = json!({"status":"codec_error","fixture_sha256":"x"});
    let after = json!({"status":"pass","fixture_sha256":"x","actual_pcm_sha256":"y"});
    assert_eq!(
        delta(&after, Some(&before))["kind"],
        "incomparable_not_exercised_or_no_pcm"
    );
}

#[test]
fn baseline_rejects_self_consistent_forged_fixture_lengths() {
    let mut document = baseline_document();
    let root = test_vectors_dir();
    let id = "celt/fb/silence_mono";
    let (entries, _) = corpus::inventory(&root).unwrap();
    let (vectors, identity) = corpus::packets(&root, id, &entries[id]).unwrap();
    assert_ne!(vectors[0].expected_pcm.len(), 1);
    document[1]["fixture_sha256"] = json!(identity);
    let mut packet = compare(&[0], &[0]);
    packet["packet_index"] = json!(0);
    packet["actual_pcm_sha256"] = json!(hash(&[0, 0]));
    document[1]["actual_pcm_sha256"] = json!(output_identity(std::slice::from_ref(&packet)));
    document[1]["packets"] = json!([packet]);
    let fixture = Fixture::new();
    let path = fixture.0.join("baseline.jsonl");
    fs::write(
        &path,
        document
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    assert!(
        baseline::load_for_root(&path, &root)
            .err()
            .unwrap()
            .contains("expected count")
    );
}

#[test]
fn baseline_rejects_forged_aggregate_and_contradictory_max_error() {
    let mut document = baseline_document();
    document[1]["actual_pcm_sha256"] = json!("0".repeat(64));
    assert!(parse_document(&document).is_err());
    let mut packet = compare(&[i16::MIN], &[i16::MAX]);
    packet["actual_pcm_sha256"] = json!("a".repeat(64));
    packet["max_absolute_error"] = json!(1);
    assert!(baseline::validate_comparison(&packet).is_err());
    for field in [
        "max_absolute_error",
        "different_common_samples",
        "actual_samples",
    ] {
        packet[field] = json!(1.5);
        assert!(baseline::validate_comparison(&packet).is_err());
    }
    let mut old = baseline_document();
    old[0]["schema"] = json!(2);
    assert!(parse_document(&old).is_err());
}

#[test]
fn sequence_aggregate_cannot_change_without_ordered_packet_evidence() {
    let root = test_vectors_dir();
    let (entries, _) = corpus::inventory(&root).unwrap();
    let id = "sequences/celt_fb_mono";
    let record = case(&root, id, &entries[id]);
    let mut document = baseline_document();
    document[1] = record;
    let status = document[1]["status"].as_str().unwrap().to_owned();
    document[2]["statuses"] = json!({status:1});
    let original_packets = document[1]["packets"].clone();
    assert!(parse_document(&document).is_ok());
    document[1]["actual_pcm_sha256"] = json!("0".repeat(64));
    assert_eq!(document[1]["packets"], original_packets);
    assert!(parse_document(&document).is_err());
}

#[test]
fn startup_rejects_custom_tampered_corpus_before_children() {
    let fixture = Fixture::new();
    copy_tree(&test_vectors_dir(), &fixture.0);
    assert!(verify_frozen_corpus(&fixture.0).is_ok());
    let path = fixture.0.join("celt/fb/silence_mono/expected.pcm");
    let mut bytes = read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(path, bytes).unwrap();
    let result = run(Options {
        root: Some(fixture.0.clone()),
        diagnostic: true,
        ..Options::default()
    });
    assert!(
        result
            .err()
            .unwrap()
            .contains("frozen corpus identity failure")
    );
}

#[test]
fn frozen_manifest_identity_is_required_in_schema3() {
    let mut document = baseline_document();
    document[0]["frozen_manifest_sha256"] = json!("0".repeat(64));
    assert!(parse_document(&document).is_err());
}
