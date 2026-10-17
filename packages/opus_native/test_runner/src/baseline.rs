//! Scoreboard v3 baseline validation. Baselines are data, not trusted claims.
use super::{Result, json, read, required, successful};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub struct Baseline {
    pub header: Value,
    pub records: BTreeMap<String, Value>,
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(format!("invalid baseline: {message}"))
    }
}

fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub fn compatible(old: &Value, current: &Value) -> bool {
    [
        "features",
        "frozen_manifest_sha256",
        "build_profile",
        "reference_provenance",
        "comparison_contract",
    ]
    .iter()
    .all(|key| old[key] == current[key])
}

fn header(value: &Value) -> Result<()> {
    require(
        value["schema"] == 3 && value["type"] == "run",
        "unsupported schema",
    )?;
    require(
        value["frozen_manifest_sha256"] == super::FROZEN_MANIFEST_SHA256,
        "missing or incompatible frozen manifest identity",
    )?;
    for key in ["source_sha256", "executable_sha256"] {
        require(digest(&value[key]), "missing header identity")?;
    }
    require(
        matches!(value["mode"].as_str(), Some("strict" | "diagnostic")),
        "invalid run mode",
    )?;
    require(
        matches!(
            value["build_profile"].as_str(),
            Some("debug_assertions" | "release_no_debug_assertions")
        ),
        "invalid build profile",
    )?;
    require(
        value["comparison_contract"] == "raw_exact_pcm_packet_identity_v2",
        "invalid comparison contract",
    )?;
    require(
        value["root"].as_str().is_some()
            && value["source_identity_scope"].as_str().is_some()
            && value["coverage"].as_str().is_some()
            && value["legacy_compensated_checks"].as_str().is_some(),
        "incomplete header",
    )?;
    let features = value["features"]
        .as_object()
        .ok_or("baseline features missing")?;
    require(
        features.len() == 4
            && ["silk", "celt", "hybrid", "resampling"]
                .iter()
                .all(|key| features.get(*key).is_some_and(Value::is_boolean)),
        "invalid features",
    )?;
    require(
        value["git_head"].as_str().is_some_and(|text| {
            text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
        }),
        "missing Git identity",
    )?;
    let provenance = &value["reference_provenance"];
    require(
        provenance["format_version"] == 1
            && provenance["pcm_semantics"] == "raw_decoder_output"
            && provenance["fixed_point"] == true
            && provenance["intrinsics"] == false
            && provenance["float_api"] == false
            && provenance["checksum_algorithm"] == "fnv1a64"
            && provenance["reference_revision"]
                .as_str()
                .is_some_and(|text| {
                    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
            && provenance["generator"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
        "invalid reference provenance",
    )
}

pub fn validate_comparison(value: &Value) -> Result<()> {
    let number = |key: &str| {
        value[key]
            .as_u64()
            .ok_or_else(|| format!("baseline comparison missing {key}"))
    };
    let expected = number("expected_samples")?;
    let actual = number("actual_samples")?;
    let different = number("different_common_samples")?;
    let unpaired = number("unpaired_samples")?;
    let max = number("max_absolute_error")?;
    require(
        expected > 0
            && different <= expected.min(actual)
            && unpaired == expected.abs_diff(actual)
            && value["length_equal"] == (expected == actual)
            && max <= 65535,
        "inconsistent PCM comparison",
    )?;
    require(
        digest(&value["actual_pcm_sha256"]),
        "missing packet PCM identity",
    )?;
    if different == 0 {
        require(
            value["first_difference"].is_null() && max == 0,
            "unexpected first difference",
        )?;
    } else {
        let first = &value["first_difference"];
        require(
            first["index"]
                .as_u64()
                .is_some_and(|index| index < expected.min(actual))
                && first["expected"]
                    .as_i64()
                    .is_some_and(|n| i16::try_from(n).is_ok())
                && first["actual"]
                    .as_i64()
                    .is_some_and(|n| i16::try_from(n).is_ok())
                && first["expected"] != first["actual"]
                && first["expected"]
                    .as_i64()
                    .zip(first["actual"].as_i64())
                    .is_some_and(|(expected, actual)| expected.abs_diff(actual) <= max)
                && max > 0,
            "invalid first difference",
        )?;
    }
    require(
        (value["status"] == "pass") == (different == 0 && unpaired == 0),
        "false PCM status",
    )
}

fn record(value: &Value, header: &Value) -> Result<()> {
    let id = value["id"].as_str().ok_or("baseline missing case ID")?;
    let (_, sequence, count) = required::CASES
        .iter()
        .find(|(candidate, _, _)| *candidate == id)
        .ok_or("baseline unknown case ID")?;
    require(
        value["kind"] == if *sequence { "sequence" } else { "single" },
        "case kind mismatch",
    )?;
    require(
        value["comparison"] == "exact_uncompensated_pcm"
            && value["reproduce"] == json!(["--case", id]),
        "invalid case comparison/reproduction",
    )?;
    let status = value["status"].as_str().ok_or("baseline missing status")?;
    require(
        matches!(
            status,
            "pass"
                | "pcm_mismatch"
                | "codec_error"
                | "codec_crash"
                | "codec_timeout"
                | "harness_failure"
                | "unsupported_not_exercised"
        ),
        "unknown case status",
    )?;
    if status == "harness_failure" {
        return require(
            value["exercised"] == false
                && value["error"].as_str().is_some_and(|text| !text.is_empty())
                && value["fixture_sha256"].is_null()
                && value["actual_pcm_sha256"].is_null(),
            "invalid harness failure",
        );
    }
    require(digest(&value["fixture_sha256"]), "missing fixture identity")?;
    let mode = value["mode"].as_str().ok_or("missing codec mode")?;
    require(
        matches!(mode, "silk" | "celt" | "hybrid"),
        "invalid codec mode",
    )?;
    require(
        matches!(mode, "silk" | "celt" | "hybrid")
            && if *sequence {
                id.strip_prefix("sequences/")
                    .is_some_and(|name| name.starts_with(&format!("{mode}_")))
            } else {
                id.starts_with(&format!("{mode}/"))
            },
        "codec mode disagrees with case ID",
    )?;
    let enabled = header["features"][mode] == true;
    if status == "unsupported_not_exercised" {
        return require(
            !enabled
                && value["exercised"] == false
                && value["packets"].is_null()
                && value["actual_pcm_sha256"].is_null(),
            "invalid unsupported case",
        );
    }
    require(
        enabled && value["exercised"] == true,
        "unexercised case claims codec result",
    )?;
    if matches!(status, "codec_crash" | "codec_timeout") {
        return require(
            value["packets"].is_null()
                && value["actual_pcm_sha256"].is_null()
                && value["error"].as_str().is_some_and(|text| !text.is_empty())
                && value["isolation"]["outcome"] == status,
            "invalid isolated failure",
        );
    }
    if value["packets"].is_null() {
        return require(
            status == "codec_error"
                && value["error"].as_str().is_some_and(|text| !text.is_empty())
                && value["actual_pcm_sha256"].is_null(),
            "invalid decoder initialization failure",
        );
    }
    packet_records(value, *count, status)
}

fn packet_records(value: &Value, count: usize, status: &str) -> Result<()> {
    let packets = value["packets"]
        .as_array()
        .ok_or("baseline missing packets")?;
    require(
        packets.len() == count && digest(&value["actual_pcm_sha256"]),
        "invalid packet count/output identity",
    )?;
    require(
        value["actual_pcm_sha256"] == super::output_identity(packets),
        "case output identity disagrees with ordered packets",
    )?;
    let mut failed = false;
    let mut mismatched = false;
    let mut different = 0_u64;
    let mut unpaired = 0_u64;
    for (index, packet) in packets.iter().enumerate() {
        require(
            packet["packet_index"].as_u64() == Some(index as u64),
            "noncontiguous packet diagnostics",
        )?;
        if failed {
            require(
                packet["status"] == "not_exercised_after_codec_failure"
                    && packet["actual_pcm_sha256"].is_null(),
                "codec failure does not invalidate later state",
            )?;
            continue;
        }
        match packet["status"].as_str() {
            Some("pass" | "pcm_mismatch") => {
                validate_comparison(packet)?;
                mismatched |= packet["status"] == "pcm_mismatch";
                different = different
                    .checked_add(
                        packet["different_common_samples"]
                            .as_u64()
                            .expect("validated count"),
                    )
                    .ok_or("baseline count overflow")?;
                unpaired = unpaired
                    .checked_add(
                        packet["unpaired_samples"]
                            .as_u64()
                            .expect("validated count"),
                    )
                    .ok_or("baseline count overflow")?;
            }
            Some("codec_error") => {
                require(
                    packet["error"]
                        .as_str()
                        .is_some_and(|text| !text.is_empty()),
                    "missing codec error",
                )?;
                require(
                    packet["actual_samples"].is_null() && packet["actual_pcm_sha256"].is_null(),
                    "codec failure claims PCM evidence",
                )?;
                failed = true;
            }
            Some("codec_panic") => {
                require(
                    packet["actual_samples"].is_null() && packet["actual_pcm_sha256"].is_null(),
                    "panic claims PCM evidence",
                )?;
                failed = true;
            }
            _ => return Err("invalid baseline packet status".into()),
        }
    }
    require(
        value["different_common_samples"] == different && value["unpaired_samples"] == unpaired,
        "case totals disagree with packets",
    )?;
    require(
        status
            == if failed {
                "codec_error"
            } else if mismatched {
                "pcm_mismatch"
            } else {
                "pass"
            },
        "case status disagrees with packets",
    )
}

pub fn load_for_root(path: &Path, root: &Path) -> Result<Baseline> {
    let baseline = load(path)?;
    let (entries, _) = super::corpus::inventory(root)?;
    for (id, record) in &baseline.records {
        let entry = entries
            .get(id)
            .ok_or("baseline case absent from current corpus")?;
        let (packets, identity) = super::corpus::packets(root, id, entry)?;
        require(
            record["fixture_sha256"] == identity,
            "baseline fixture identity differs from current fixture",
        )?;
        if let Some(records) = record["packets"].as_array() {
            for (record, vector) in records.iter().zip(packets) {
                if matches!(record["status"].as_str(), Some("pass" | "pcm_mismatch")) {
                    require(
                        record["expected_samples"] == vector.expected_pcm.len(),
                        "baseline expected count differs from current fixture",
                    )?;
                    let expected_bytes = vector
                        .expected_pcm
                        .iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect::<Vec<_>>();
                    require(
                        (record["actual_pcm_sha256"] == super::hash(&expected_bytes))
                            == (record["status"] == "pass"),
                        "baseline pass/hash disagrees with reference PCM",
                    )?;
                    if let Some(first) = record["first_difference"].as_object() {
                        let index = first["index"]
                            .as_u64()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or("invalid difference index")?;
                        require(
                            vector
                                .expected_pcm
                                .get(index)
                                .is_some_and(|expected| first["expected"] == *expected),
                            "baseline first difference disagrees with reference PCM",
                        )?;
                    }
                }
            }
        }
    }
    Ok(baseline)
}

pub fn load(path: &Path) -> Result<Baseline> {
    let bytes = read(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    parse(text)
}

pub fn parse(text: &str) -> Result<Baseline> {
    let mut lines = text.lines();
    let header_value: Value = serde_json::from_str(lines.next().ok_or("empty baseline")?)
        .map_err(|error| error.to_string())?;
    header(&header_value)?;
    let mut records = BTreeMap::new();
    let mut summary = None;
    for line in lines {
        require(summary.is_none(), "records after summary")?;
        let value: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if value["type"] == "summary" {
            summary = Some(value);
        } else {
            require(value["type"] == "case", "unexpected record type")?;
            record(&value, &header_value)?;
            let id = value["id"].as_str().expect("validated ID").to_owned();
            require(records.insert(id, value).is_none(), "duplicate case")?;
        }
    }
    let summary = summary.ok_or("baseline missing summary")?;
    let mut counts = BTreeMap::<String, usize>::new();
    for record in records.values() {
        *counts
            .entry(record["status"].as_str().expect("validated status").into())
            .or_default() += 1;
    }
    require(
        !records.is_empty() && summary["statuses"] == json!(counts),
        "summary counts mismatch",
    )?;
    let all = summary["scope"] == "all_cases";
    require(
        (all && records.len() == required::CASES.len())
            || (summary["scope"] == "focused" && records.len() == 1),
        "incomplete scope",
    )?;
    let diagnostic = header_value["mode"] == "diagnostic";
    let success = successful(&counts, diagnostic);
    require(
        summary["success"] == success
            && summary["selected_cases_pass"] == (!diagnostic && success)
            && summary["whole_corpus_pass"] == (all && !diagnostic && success)
            && digest(&summary["corpus_sha256"])
            && summary["baseline_cases_not_selected"].is_array(),
        "invalid summary",
    )?;
    Ok(Baseline {
        header: header_value,
        records,
    })
}
