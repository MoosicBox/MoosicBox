#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Exact, uncompensated reference scoreboard. See the package README for its contract.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use moosicbox_opus_native_test_vectors::{
    TestVector,
    frozen::{FROZEN_MANIFEST_SHA256, verify_frozen_corpus},
    test_vectors_dir,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod baseline;
mod corpus;
mod isolation;
#[cfg(test)]
mod negative_tests;
mod required;

type Result<T> = std::result::Result<T, String>;

#[derive(Default)]
struct Options {
    root: Option<PathBuf>,
    case: Option<String>,
    baseline: Option<PathBuf>,
    diagnostic: bool,
    internal_case: Option<String>,
    expected_executable: Option<String>,
}

fn options(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--internal-case" => {
                options.internal_case = Some(args.next().ok_or("missing internal case")?);
            }
            "--expected-executable" => {
                options.expected_executable = Some(args.next().ok_or("missing executable hash")?);
            }
            "--diagnostic" => options.diagnostic = true,
            "--root" => options.root = Some(args.next().ok_or("--root requires a path")?.into()),
            "--case" => options.case = Some(args.next().ok_or("--case requires an ID")?),
            "--baseline" => {
                options.baseline = Some(
                    args.next()
                        .ok_or("--baseline requires a JSONL path")?
                        .into(),
                );
            }
            _ => {
                return Err(format!(
                    "unknown argument {arg}; usage: moosicbox_opus_native_test_runner [--case ID] [--root DIR] [--baseline JSONL] [--diagnostic]"
                ));
            }
        }
    }
    Ok(options)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn packet_mode(packet: &[u8]) -> Result<&'static str> {
    match packet.first().ok_or("empty packet")? >> 3 {
        0..=11 => Ok("silk"),
        12..=15 => Ok("hybrid"),
        _ => Ok("celt"),
    }
}

fn load(root: &Path, id: &str) -> Result<(TestVector, String, String)> {
    let path = root.join(id);
    let packet = read(&path.join("packet.bin"))?;
    let pcm = read(&path.join("expected.pcm"))?;
    let metadata = read(&path.join("meta.json"))?;
    if pcm.is_empty() || pcm.len() % 2 != 0 {
        return Err("empty or odd-byte PCM fixture".into());
    }
    let meta: Value = serde_json::from_slice(&metadata).map_err(|error| error.to_string())?;
    let mode = packet_mode(&packet)?;
    if meta["format_version"] != 2
        || meta["pcm_semantics"] != "raw_decoder_output"
        || meta["decoder_state"] != "reset"
        || meta["packet_index"] != 0
    {
        return Err("single packet must use v2 raw reset-state metadata".into());
    }
    if meta["mode"].as_str() != Some(mode) || id.split('/').next() != Some(mode) {
        return Err(format!(
            "mislabeled mode: TOC={mode}, metadata={}, ID={id}",
            meta["mode"]
        ));
    }
    let vector = TestVector::load(&path).map_err(|error| error.to_string())?;
    if !matches!(vector.channels, 1 | 2)
        || vector.expected_pcm.len() % usize::from(vector.channels) != 0
    {
        return Err("invalid channel count or PCM channel alignment".into());
    }
    if id.ends_with("_stereo") != (vector.channels == 2) {
        return Err("channel label disagrees with metadata".into());
    }
    let duration = meta["frame_size_ms"]
        .as_u64()
        .ok_or("missing frame_size_ms")?;
    if duration == 0
        || (vector.expected_pcm.len() as u64) * 1000
            != u64::from(vector.sample_rate) * duration * u64::from(vector.channels)
    {
        return Err("reference PCM length disagrees with frame metadata".into());
    }
    SampleRate::from_hz(vector.sample_rate).map_err(|error| error.to_string())?;
    let mut identity = Sha256::new();
    for bytes in [&packet, &pcm, &metadata] {
        identity.update((bytes.len() as u64).to_le_bytes());
        identity.update(bytes);
    }
    Ok((vector, hex(&identity.finalize()), mode.into()))
}

fn enabled(mode: &str) -> bool {
    (mode == "silk" && cfg!(feature = "silk"))
        || (mode == "celt" && cfg!(feature = "celt"))
        || (mode == "hybrid" && cfg!(feature = "hybrid"))
}

fn compare(expected: &[i16], actual: &[i16]) -> Value {
    let differences = expected
        .iter()
        .zip(actual)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .collect::<Vec<_>>();
    let first = differences.first().map(|(index, (expected, actual))| json!({"index": index, "expected": expected, "actual": actual}));
    let max_error = differences
        .iter()
        .map(|(_, (a, b))| (i32::from(**a) - i32::from(**b)).unsigned_abs())
        .max()
        .unwrap_or(0);
    json!({
        "status": if expected == actual { "pass" } else { "pcm_mismatch" },
        "expected_samples": expected.len(), "actual_samples": actual.len(),
        "length_equal": expected.len() == actual.len(),
        "different_common_samples": differences.len(),
        "unpaired_samples": expected.len().abs_diff(actual.len()),
        "first_difference": first, "max_absolute_error": max_error,
    })
}

fn decoder(vector: &TestVector) -> Result<Decoder> {
    let rate = SampleRate::from_hz(vector.sample_rate).map_err(|error| error.to_string())?;
    let channels = if vector.channels == 1 {
        Channels::Mono
    } else {
        Channels::Stereo
    };
    Decoder::new(rate, channels).map_err(|error| error.to_string())
}

fn decode(decoder: &mut Decoder, vector: &TestVector) -> Result<Vec<i16>> {
    // Maximum legal Opus packet duration: do not size to the expected length and
    // accidentally turn extra decoder output into an output-buffer error.
    let mut output =
        vec![0; vector.sample_rate as usize * 120 / 1000 * usize::from(vector.channels)];
    let count = decoder
        .decode(Some(&vector.packet), &mut output, false)
        .map_err(|error| error.to_string())?;
    let total = count
        .checked_mul(usize::from(vector.channels))
        .ok_or("decoder count overflow")?;
    if total > output.len() {
        return Err("decoder returned count larger than output capacity".into());
    }
    output.truncate(total);
    Ok(output)
}

fn case(root: &Path, id: &str, entry: &corpus::Entry) -> Value {
    let mut record = json!({"type": "case", "id": id, "comparison": "exact_uncompensated_pcm", "reproduce": ["--case", id], "kind": if entry.sequence { "sequence" } else { "single" }});
    let loaded = catch_unwind(AssertUnwindSafe(|| corpus::packets(root, id, entry)))
        .unwrap_or_else(|_| {
            Err("fixture loader panicked; invalid fixture or loader failure".into())
        });
    let (packets, identity) = match loaded {
        Ok(value) => value,
        Err(error) => {
            record["status"] = json!("harness_failure");
            record["error"] = json!(error);
            record["exercised"] = json!(false);
            return record;
        }
    };
    record["fixture_sha256"] = json!(identity);
    let mode = packet_mode(&packets[0].packet).expect("validated packet");
    record["mode"] = json!(mode);
    let exercised = packets
        .iter()
        .all(|packet| packet_mode(&packet.packet).is_ok_and(enabled));
    record["exercised"] = json!(exercised);
    if !exercised {
        record["status"] = json!("unsupported_not_exercised");
        return record;
    }
    let mut state = match decoder(&packets[0]) {
        Ok(state) => state,
        Err(error) => {
            record["status"] = json!("codec_error");
            record["error"] = json!(error);
            return record;
        }
    };
    let mut diagnostics = Vec::new();

    let mut poisoned = false;
    for (index, vector) in packets.iter().enumerate() {
        let mut result = if poisoned {
            json!({"status": "not_exercised_after_codec_failure"})
        } else {
            match catch_unwind(AssertUnwindSafe(|| decode(&mut state, vector))) {
                Ok(Ok(actual)) => {
                    let mut result = compare(&vector.expected_pcm, &actual);
                    let bytes = actual
                        .iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect::<Vec<_>>();
                    result["actual_pcm_sha256"] = json!(hash(&bytes));

                    result
                }
                Ok(Err(error)) => {
                    poisoned = true;
                    json!({"status": "codec_error", "error": error})
                }
                Err(_) => {
                    poisoned = true;
                    json!({"status": "codec_panic"})
                }
            }
        };
        result["packet_index"] = json!(index);
        diagnostics.push(result);
    }
    record["status"] = json!(if poisoned {
        "codec_error"
    } else if diagnostics.iter().all(|packet| packet["status"] == "pass") {
        "pass"
    } else {
        "pcm_mismatch"
    });
    record["actual_pcm_sha256"] = json!(output_identity(&diagnostics));
    record["different_common_samples"] = json!(
        diagnostics
            .iter()
            .filter_map(|packet| packet["different_common_samples"].as_u64())
            .sum::<u64>()
    );
    record["unpaired_samples"] = json!(
        diagnostics
            .iter()
            .filter_map(|packet| packet["unpaired_samples"].as_u64())
            .sum::<u64>()
    );
    record["packets"] = json!(diagnostics);
    record
}

// Hash only evidence that can be recomputed from serialized ordered packets.
fn output_identity(packets: &[Value]) -> String {
    hash(
        serde_json::to_string(
            &packets
                .iter()
                .map(|packet| {
                    json!([
                        packet["packet_index"],
                        packet["status"],
                        packet["actual_samples"],
                        packet["actual_pcm_sha256"]
                    ])
                })
                .collect::<Vec<_>>(),
        )
        .expect("JSON values serialize")
        .as_bytes(),
    )
}

fn delta(current: &Value, old: Option<&Value>) -> Value {
    let Some(old) = old else {
        return json!({"kind": "new_case"});
    };
    if current["fixture_sha256"].is_null() || current["fixture_sha256"] != old["fixture_sha256"] {
        return json!({"kind": "incomparable_fixture", "previous_status": old["status"]});
    }
    if old["actual_pcm_sha256"].as_str().is_none()
        || current["actual_pcm_sha256"].as_str().is_none()
    {
        return json!({"kind":"incomparable_not_exercised_or_no_pcm", "previous_status":old["status"]});
    }
    if current["status"] == "pass"
        && old["status"] == "codec_error"
        && !old["packets"].as_array().is_some_and(|packets| {
            packets
                .iter()
                .any(|packet| matches!(packet["status"].as_str(), Some("pass" | "pcm_mismatch")))
        })
    {
        return json!({"kind":"incomparable_not_exercised_or_no_pcm", "previous_status": old["status"]});
    }
    let kind = match (old["status"].as_str(), current["status"].as_str()) {
        (Some("pass"), Some("pass")) => "unchanged_pass",
        (Some("pass"), _) => "regression",
        (_, Some("pass")) => "resolved",
        _ => "still_failing",
    };
    json!({"kind": kind, "previous_status": old["status"],
        "previous_different_common_samples": old["different_common_samples"],
        "previous_actual_pcm_sha256": old["actual_pcm_sha256"],
        "output_changed": current["actual_pcm_sha256"] != old["actual_pcm_sha256"]})
}

fn source_files(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            source_files(&entry.map_err(|error| error.to_string())?.path(), files)?;
        }
    } else {
        files.insert(path.to_owned());
    }
    Ok(())
}

fn source_identity() -> Result<String> {
    let native = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing native directory")?;
    let mut files = BTreeSet::new();
    for relative in [
        "src",
        "Cargo.toml",
        "test_runner/src",
        "test_runner/Cargo.toml",
        "test_vectors/src",
        "test_vectors/build.rs",
        "test_vectors/Cargo.toml",
        "libopus/src",
        "libopus/build.rs",
        "libopus/Cargo.toml",
        "libopus/opus/celt",
        "libopus/opus/silk",
        "libopus/opus/src",
        "libopus/opus/include",
        "libopus/opus/CMakeLists.txt",
    ] {
        source_files(&native.join(relative), &mut files)?;
    }
    let workspace = native
        .parent()
        .and_then(Path::parent)
        .ok_or("missing workspace")?;
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        ".cargo",
        "rust-toolchain.toml",
        "rust-toolchain",
    ] {
        let path = workspace.join(name);
        if path.exists() {
            source_files(&path, &mut files)?;
        }
    }
    let mut digest = Sha256::new();
    for path in files {
        let name = path
            .strip_prefix(workspace)
            .map_err(|error| error.to_string())?
            .to_str()
            .ok_or("non-UTF8 source path")?;
        let bytes = read(&path)?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(hex(&digest.finalize()))
}

fn runtime_context() -> Result<Value> {
    let rustc = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|error| error.to_string())?;
    if !rustc.status.success() {
        return Err("cannot observe runtime rustc".into());
    }
    let environment = [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_PROFILE_RELEASE_LTO",
        "CARGO_PROFILE_RELEASE_OPT_LEVEL",
    ]
    .into_iter()
    .map(|key| {
        (
            key,
            std::env::var_os(key).map(|value| hash(value.to_string_lossy().as_bytes())),
        )
    })
    .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"scope":"runtime observations only; not build provenance", "rustc_vv_sha256":hash(&rustc.stdout), "configuration_environment_sha256":environment}),
    )
}

fn successful(statuses: &BTreeMap<String, usize>, diagnostic: bool) -> bool {
    !statuses.contains_key("harness_failure")
        && (diagnostic || statuses.keys().all(|status| status == "pass"))
}

fn run(options: Options) -> Result<bool> {
    let root = options
        .root
        .unwrap_or_else(test_vectors_dir)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    verify_frozen_corpus(&root)
        .map_err(|error| format!("frozen corpus identity failure: {error}"))?;
    let (mut entries, provenance) = corpus::inventory(&root)?;
    if let Some(id) = &options.case {
        if !entries.contains_key(id) {
            return Err(format!("unknown case ID {id}"));
        }
        entries.retain(|candidate, _| candidate == id);
    }
    let previous = options
        .baseline
        .as_deref()
        .map(|path| baseline::load_for_root(path, &root))
        .transpose()?;
    let snapshot = isolation::ExecutableSnapshot::new()?;
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let header = json!({"type": "run", "schema": 3,
        "frozen_manifest_sha256": FROZEN_MANIFEST_SHA256, "comparison_contract": "raw_exact_pcm_packet_identity_v2", "build_profile": if cfg!(debug_assertions) {"debug_assertions"} else {"release_no_debug_assertions"}, "mode": if options.diagnostic { "diagnostic" } else { "strict" },
        "root": root, "source_sha256": source_identity()?, "source_identity_scope": "runtime working-tree snapshot: native/runner/vector/wrapper, pinned C source, workspace manifest/lock/config; NOT compiled-artifact build provenance",
        "runtime_context": runtime_context()?, "executable_sha256": snapshot.sha256, "git_head": revision,
        "reference_provenance": provenance,
        "legacy_compensated_checks": "not_exercised_not_full_pcm_parity",
        "coverage": "single_packet_and_persistent_sequence_decode; encode, PLC, FEC not exercised",
        "features": {"silk": cfg!(feature="silk"), "celt": cfg!(feature="celt"), "hybrid": cfg!(feature="hybrid"), "resampling": cfg!(feature="resampling")}});
    println!("{header}");
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut corpus = Sha256::new();
    corpus.update(read(&root.join("inventory.json"))?);
    corpus.update(read(&root.join("provenance.json"))?);
    let missing_from_current = previous
        .as_ref()
        .map(|previous| {
            previous
                .records
                .keys()
                .filter(|id| !entries.contains_key(*id))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (id, entry) in entries {
        let mut record = isolation::run(&root, &id, &entry, &snapshot);
        if let Some(previous) = &previous {
            record["baseline_delta"] = if baseline::compatible(&previous.header, &header) {
                delta(&record, previous.records.get(&id))
            } else {
                json!({"kind":"incomparable_configuration"})
            };
        }
        corpus.update((id.len() as u64).to_le_bytes());
        corpus.update(id.as_bytes());
        corpus.update(
            record["fixture_sha256"]
                .as_str()
                .unwrap_or("INVALID")
                .as_bytes(),
        );
        *statuses
            .entry(record["status"].as_str().expect("case status").into())
            .or_default() += 1;
        println!("{record}");
    }
    let success = successful(&statuses, options.diagnostic);
    println!(
        "{}",
        json!({"type": "summary", "statuses": statuses, "success": success,
        "corpus_sha256": hex(&corpus.finalize()), "baseline_cases_not_selected": missing_from_current, "scope": if options.case.is_some() { "focused" } else { "all_cases" },
        "selected_cases_pass": !options.diagnostic && success, "whole_corpus_pass": options.case.is_none() && !options.diagnostic && success})
    );
    Ok(success)
}

fn main() -> ExitCode {
    let parsed = match options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    if let Some(id) = &parsed.internal_case {
        if parsed.case.is_some() || parsed.baseline.is_some() || parsed.diagnostic {
            return ExitCode::from(2);
        }
        let result = parsed
            .root
            .as_deref()
            .zip(parsed.expected_executable.as_deref())
            .ok_or_else(|| "incomplete child protocol".to_owned())
            .and_then(|(root, hash)| isolation::child(root, id, hash));
        return match result {
            Ok(record) => {
                println!("{record}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        };
    }
    if parsed.expected_executable.is_some()
        || std::env::var_os("MOOSICBOX_ENTROPY_CASE_CHILD").is_some()
    {
        return ExitCode::from(2);
    }
    match run(parsed) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            println!("{}", json!({"type": "harness_failure", "error": error}));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_comparison_rejects_delay_and_length_mismatch() {
        assert_eq!(compare(&[0, 1, 2], &[1, 2])["status"], "pcm_mismatch");
        assert_eq!(compare(&[1, 2], &[1, 2, 3])["unpaired_samples"], 1);
        assert_eq!(compare(&[1, 2], &[1, 2])["status"], "pass");
    }

    #[test]
    fn diagnostic_cannot_hide_harness_failure() {
        let mut statuses = BTreeMap::from([("codec_error".into(), 2)]);
        assert!(!successful(&statuses, false));
        assert!(successful(&statuses, true));
        statuses.insert("harness_failure".into(), 1);
        assert!(!successful(&statuses, true));
    }

    #[test]
    fn baseline_changes_are_not_silently_accepted() {
        let old = json!({"status": "pass", "fixture_sha256": "a", "actual_pcm_sha256":"x"});
        let current =
            json!({"status": "pcm_mismatch", "fixture_sha256": "a", "actual_pcm_sha256":"y"});
        assert_eq!(delta(&current, Some(&old))["kind"], "regression");
        assert_eq!(
            delta(&json!({"fixture_sha256": "b"}), Some(&old))["kind"],
            "incomparable_fixture"
        );
    }

    #[test]
    fn empty_packet_is_not_a_case() {
        assert!(packet_mode(&[]).is_err());
        assert_eq!(packet_mode(&[0]).unwrap(), "silk");
        assert_eq!(packet_mode(&[12 << 3]).unwrap(), "hybrid");
        assert_eq!(packet_mode(&[16 << 3]).unwrap(), "celt");
    }
}
