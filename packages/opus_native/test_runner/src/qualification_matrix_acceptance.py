"""Fail-closed aggregation for existing qualification runners, not a codec runner.

The bundle names actual artifact paths. Hashes and source identities are verified;
missing/debug-timeout/ignored-only evidence cannot be converted into acceptance.
CI comparison is delegated to its existing artifact verifier (no new platform policy).
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
from types import SimpleNamespace

import qualification_ci as ci
import qualification_matrix as matrix
import qualification_performance as performance
import qualification_silk as silk


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def local_path(root, name):
    path = (root / name).resolve(strict=True)
    if not path.is_relative_to(root.resolve()):
        raise ValueError("evidence path escapes bundle directory")
    return path


def read_artifact(root, entry):
    path = local_path(root, entry["path"])
    if digest(path) != entry["sha256"]:
        raise ValueError(f"evidence checksum mismatch: {path.name}")
    return path, json.loads(path.read_text(encoding="utf-8"))


def verify_full_suite(root, suite, names, ignored):
    if suite.get("exit_code") != 0 or suite.get("timed_out") is not False:
        raise ValueError("full unit suite failed or timed out")
    log = local_path(root, suite["log"])
    if digest(log) != suite["log_sha256"]:
        raise ValueError("full unit log changed")
    text = log.read_text(encoding="utf-8", errors="replace")
    passed = re.findall(r"^test (.+) \.\.\. ok\s*$", text, re.MULTILINE)
    skipped = re.findall(r"^test (.+) \.\.\. ignored(?:,.*)?\s*$", text, re.MULTILINE)
    if (len(passed) != len(set(passed)) or set(passed) != set(names)
            or len(skipped) != len(set(skipped)) or set(skipped) != set(ignored)):
        raise ValueError("full unit suite does not cover discovered inventory exactly once")
    summaries = re.findall(r"^test result: ok\. (\d+) passed; 0 failed; (\d+) ignored; 0 measured; (\d+) filtered out;", text, re.MULTILINE)
    if summaries != [(str(len(names)), str(len(ignored)), "0")]:
        raise ValueError("full unit summary missing/incomplete/filtered")
    return set(passed)


def verify_unit_shards(root, entry, source):
    """All discovered ordinary tests in both profiles: full suite or bounded shards.

    Discovery is captured from actual --list output and retained with an image
    hash/build receipt by the build owner. A complete full-suite log may cover all
    tests at once; bounded exact shards may also cover them. Neither allows a
    timeout exemption or a partial-pass-count shortcut.
    """
    _, report = read_artifact(root, entry)
    if report.get("schema") != "moosicbox.opus.unit-shards.v1" or report.get("source_sha256") != source:
        raise ValueError("unit shard evidence source/schema mismatch")
    if report.get("test_source_manifest") != ci.sources():
        raise ValueError("unit test source manifest missing/stale, including newly scored tests")
    profiles = report.get("profiles", {})
    if set(profiles) != {"debug", "release"}:
        raise ValueError("both debug/release unit evidence required")
    discovered_sets = []
    for profile, evidence in profiles.items():
        listing_path, _ = read_artifact(root, evidence["inventory"])
        inventory = json.loads(listing_path.read_text(encoding="utf-8"))
        names = inventory.get("ordinary_tests", [])
        if not names or len(names) != len(set(names)):
            raise ValueError("empty/duplicate ordinary test inventory")
        # Discovery bytes are required, not just a hand-asserted number of tests.
        listing = local_path(root, inventory["list_log"])
        if digest(listing) != inventory["list_sha256"]:
            raise ValueError("unit discovery log changed")
        discovered = re.findall(r"^(.+): test$", listing.read_text(encoding="utf-8"), re.MULTILINE)
        ignored = inventory.get("ignored_tests", [])
        ignored_log = local_path(root, inventory["ignored_list_log"])
        if digest(ignored_log) != inventory["ignored_list_sha256"]:
            raise ValueError("ignored unit discovery log changed")
        actual_ignored = re.findall(r"^(.+): test$", ignored_log.read_text(encoding="utf-8"), re.MULTILINE)
        if set(actual_ignored) != set(ignored):
            raise ValueError("ignored unit inventory is not backed by discovery")
        if set(discovered) != set(names) | set(ignored) or set(names) & set(ignored):
            raise ValueError("ordinary inventory differs from discovery")
        executable = local_path(root, evidence["executable_file"])
        if digest(executable) != evidence["executable_sha256"]:
            raise ValueError("unit executable bytes changed")
        receipt = evidence["build_receipt"]
        if (receipt.get("source_before_sha256") != source or receipt.get("source_after_sha256") != source
                or receipt.get("executable_sha256") != evidence["executable_sha256"]
                or receipt.get("profile") != profile or not receipt.get("command")):
            raise ValueError("unit source/build binding missing")
        seen = (verify_full_suite(root, evidence["full_suite"], names, ignored)
                if "full_suite" in evidence else set())
        for case in evidence.get("cases", []):
            name = case["name"]
            if name in seen or name not in names or case.get("exit_code") != 0 or case.get("timed_out") is not False:
                raise ValueError("duplicate/failed/timed-out unit case")
            log = local_path(root, case["log"])
            if digest(log) != case["log_sha256"]:
                raise ValueError("unit case log changed")
            text = log.read_text(encoding="utf-8", errors="replace")
            if (len(re.findall(r"^test " + re.escape(name) + r" \.\.\. ok\s*$", text, re.MULTILINE)) != 1
                    or "test result: ok. 1 passed; 0 failed; 0 ignored;" not in text):
                raise ValueError("exact unit test did not execute and pass once")
            seen.add(name)
        if seen != set(names):
            raise ValueError("missing ordinary unit tests, including any timed-out tail")
        discovered_sets.append(set(names))
    if discovered_sets[0] != discovered_sets[1]:
        raise ValueError("debug/release ordinary test inventories differ")
    return len(discovered_sets[0])


def verify_performance(report, source):
    verdict = performance.performance_verdict(report)
    if report.get("source_before_sha256") != source or not verdict["accepted"] or report.get("performance_accepted") is not True:
        raise ValueError("performance is stale, incomplete or below real time")
    if report.get("frames", 0) < 500:
        raise ValueError("final performance requires >=500 frames per repetition")
    if report.get("peak_memory_complete") is True:
        if any(not isinstance(row.get("peak_rss_bytes"), int) or row["peak_rss_bytes"] <= 0
               for row in report["cases"]):
            raise ValueError("actual per-child raw RSS totals missing")
    elif (report.get("platform", {}).get("system") not in ("Windows",)
          or any(row.get("peak_rss_bytes") is not None
                 or "unavailable" not in row.get("peak_rss_method", "") for row in report["cases"])):
        raise ValueError("peak memory unavailable without an explicit supported-platform limitation")
    receipt = report.get("build_receipt", {})
    if (receipt.get("source_before_sha256") != source or receipt.get("source_after_sha256") != source
            or receipt.get("executable_sha256") != report.get("executable_sha256")):
        raise ValueError("coherent performance build receipt missing")
    return verdict


def verify_public_silk(path, report):
    if report.get("schema") != silk.PUBLIC_SCHEMA or report.get("interface") != "public":
        raise ValueError("internal SILK evidence cannot satisfy public acceptance")
    identity = report["identity"]
    receipt = identity.get("build_receipt", {})
    if (receipt.get("schema") != "moosicbox.opus.silk-unit-build.v1"
            or receipt.get("profile") != "release"
            or receipt.get("source_before_sha256") != identity.get("source_sha256")
            or receipt.get("source_after_sha256") != identity.get("source_sha256")
            or not identity.get("executable_sha256")
            or receipt.get("executable_sha256") != identity.get("executable_sha256")
            or receipt.get("reference_sha256") != identity.get("reference_sha256")
            or not receipt.get("command") or not receipt.get("rustc_version")):
        raise ValueError("public SILK coherent build receipt missing or mismatched")
    if identity.get("source_sha256") != silk.source_identity():
        raise ValueError("public SILK source snapshot is stale")
    provenance = silk.provenance_hash(identity)
    if report.get("provenance_sha256") != provenance:
        raise ValueError("public SILK provenance checksum mismatch")
    checked = dict(report)
    silk.summarize(checked, path.parent, provenance)
    if not checked.get("complete") or checked.get("verified_passes") != 192:
        raise ValueError("all 192 hash-verified public cases required")


def verify_pipeline(root, entry):
    _, report = read_artifact(root, entry)
    if report.get("schema") != "moosicbox.opus.pipeline-evidence.v1" or report.get("source_sha256") != ci.sources():
        raise ValueError("pipeline source/schema mismatch")
    if report.get("files_test_sources") != {
        str(path.relative_to(ci.ROOT)): digest(path)
        for path in sorted((ci.ROOT / "packages/files").rglob("*.rs"))
    }:
        raise ValueError("files pipeline source manifest missing/stale")
    required = set(ci.PRODUCT_TESTS) | {
        ("moosicbox_audio_output", "opus-native", "opus_native_pipeline"),
        ("moosicbox_files", "encoder-opus-native", "opus_pipeline"),
    }
    expected = {(profile, *target) for profile in ("debug", "release") for target in required}
    seen = set()
    for row in report.get("suites", []):
        key = (row.get("profile"), row.get("package"), row.get("features"), row.get("target"))
        if key not in expected or key in seen or row.get("exit_code") != 0 or row.get("timed_out") is not False:
            raise ValueError("unexpected, duplicate or failed pipeline suite")
        log = local_path(root, row["log"])
        if digest(log) != row["log_sha256"]:
            raise ValueError("pipeline log changed")
        text = log.read_text(encoding="utf-8")
        if "PARTIAL_DIAGNOSTIC" in text:
            raise ValueError("pipeline PARTIAL_DIAGNOSTIC cannot establish full acceptance")
        if not re.search(r"test result: ok\. [1-9][0-9]* passed; 0 failed;", text):
            raise ValueError("pipeline suite did not execute")
        seen.add(key)
    if seen != expected:
        raise ValueError("missing debug/release pipeline suites")


def verify_core_matrix(report):
    """Reconstruct the current strict runner's decoder, loss and fuzz coverage."""
    if report.get("schema") != "moosicbox.opus.native-qualification.v1" or report.get("runtime_source_sha256") != matrix.native_source_identity():
        raise ValueError("core quality schema/source missing or stale")
    if report.get("accepted") is not True or report.get("failures") != 0:
        raise ValueError("core quality failed")
    expected = {
        (rate, channels, mode, ms)
        for rate in (8000, 12000, 16000, 24000, 48000)
        for channels in (1, 2) for mode in (1000, 1001, 1002)
        if mode != 1001 or rate >= 24000
        for ms in ((10, 20, 40, 60) if mode == 1000 else
                   ((10, 20) if mode == 1001 else (2.5, 5, 10, 20)))
    }
    seen = set()
    for row in report.get("decoder_matrix", []):
        key = tuple(row.get(k) for k in ("rate", "channels", "mode", "duration_ms"))
        if key not in expected or key in seen:
            raise ValueError("core decoder unexpected or duplicate cell")
        rate, channels, mode, ms = key
        size = int(rate * ms / 1000)
        if (row.get("status") != "pass" or row.get("mismatched_samples") != 0
                or row.get("max_abs_error") != 0
                or not row.get("actual_modes")
                or len(row["actual_modes"]) != len(set(row["actual_modes"]))
                or not set(row["actual_modes"]) <= {1000, 1001, 1002}
                or row.get("output_samples_per_channel") != [size]
                or not re.fullmatch(r"[0-9a-f]{64}", row.get("pcm_sha256", ""))):
            raise ValueError("core decoder incomplete or failed cell")
        seen.add(key)
    if seen != expected:
        raise ValueError("core decoder missing required cells")
    stream = report.get("long_stream", {})
    # qualification_ci invokes the strict runner's current default: 3000 frames.
    if (stream.get("frames") != 3000 or stream.get("duration_seconds") != 60
            or stream.get("loss_frames") != sum(i % 97 in (30, 31, 32) for i in range(3000))
            or stream.get("status") != "pass" or stream.get("replay_exact") is not True
            or not re.fullmatch(r"[0-9a-f]{64}", stream.get("sha256", ""))):
        raise ValueError("core longstream requires 3000 frames, burst loss and exact replay")
    fuzz = report.get("adversarial", {})
    counts = [fuzz.get(k) for k in ("accepted", "rejected", "crashes", "timeouts")]
    if (fuzz.get("cases") != 128 or fuzz.get("seed") != 2457
            or any(type(n) is not int or n < 0 for n in counts)
            or sum(counts) != 128 or counts[2:] != [0, 0]
            or fuzz.get("status") != "pass"
            or not re.fullmatch(r"[0-9a-f]{64}", fuzz.get("sha256", ""))):
        raise ValueError("core adversarial requires all 128 cases without crashes/timeouts")


def verify_core_evidence(platform_root):
    """Verify core artifacts before trusting the platform comparator's summary."""
    seen = set()
    for path in sorted(platform_root.rglob("ci.json")):
        report = json.loads(path.read_text(encoding="utf-8"))
        if report.get("mode") != "quality" or report.get("quality_shard") != "core":
            continue
        pid = report.get("platform")
        if pid not in ci.PLATFORMS or pid in seen or report.get("profile") != "release":
            raise ValueError("unexpected or duplicate core platform")
        seen.add(pid)
        quality_path = path.parent / "quality.json"
        if digest(quality_path) != report.get("quality_sha256"):
            raise ValueError("core quality artifact changed")
        quality = json.loads(quality_path.read_text(encoding="utf-8"))
        if (quality.get("native_sha256") != report.get("native_sha256")
                or quality.get("reference_sha256") != report.get("reference_sha256")
                or not report.get("native_sha256") or not report.get("reference_sha256")):
            raise ValueError("core oracle image identities missing or inconsistent")
        verify_core_matrix(quality)
        verify_combined_scoreboard(path.parent)
    if seen != set(ci.PLATFORMS):
        raise ValueError(f"missing core platforms: {sorted(set(ci.PLATFORMS) - seen)}")


def verify_combined_scoreboard(root):
    # Existing commands.json identifies the exact no-filter combined invocation.
    commands = json.loads((root / "commands.json").read_text(encoding="utf-8"))
    matches = [(i, row) for i, row in enumerate(commands)
               if len(row.get("argv", [])) == 1 and
               Path(row["argv"][0]).name in
               ("moosicbox_opus_native_test_runner", "moosicbox_opus_native_test_runner.exe")]
    if len(matches) != 1:
        raise ValueError("missing unique unfiltered combined scoreboard command")
    index, command = matches[0]
    if command.get("exit_code") != 0 or command.get("timed_out", False) is not False:
        raise ValueError("combined scoreboard failed or timed out")
    # Current CI copies this image but never hashes it in the build receipt.
    # Validate retained execution output, but never infer compile provenance
    # from the scoreboard's runtime source header.
    text = (root / f"command-{index}.stdout").read_text(encoding="utf-8")
    if "PARTIAL_DIAGNOSTIC" in text:
        raise ValueError("combined scoreboard partial diagnostic")
    records = [json.loads(line) for line in text.splitlines() if line.strip()]
    if len(records) < 3 or records[0].get("type") != "run" or records[-1].get("type") != "summary":
        raise ValueError("combined scoreboard missing header/cases/summary")
    header, summary = records[0], records[-1]
    if (header.get("schema") != 3 or header.get("mode") != "strict"
            or header.get("build_profile") != "release_no_debug_assertions"
            or header.get("comparison_contract") != "raw_exact_pcm_packet_identity_v2"
            or any(header.get("features", {}).get(k) is not True for k in ("celt", "silk", "hybrid"))):
        raise ValueError("combined scoreboard configuration is not full strict release")
    inventory = json.loads((ci.ROOT / "packages/opus_native/test_vectors/frozen/corpus/inventory.json").read_text())
    expected = {row["id"] for row in inventory["cases"]}
    cases = records[1:-1]
    if (len(cases) != len(expected) or {row.get("id") for row in cases} != expected
            or any(row.get("status") != "pass" or row.get("exercised") is not True for row in cases)
            or summary.get("statuses") != {"pass": len(expected)}
            or summary.get("scope") != "all_cases"
            or any(summary.get(k) is not True for k in ("success", "selected_cases_pass", "whole_corpus_pass"))):
        raise ValueError("combined scoreboard incomplete or failed corpus")
    # Existing bundle/CI receipts bind native/reference oracles, not the
    # combined executable. Fail closed rather than invent a receipt contract.
    raise ValueError("missing combined scoreboard compiled-image/source receipt and independent schema-3 packet/corpus verification")


def aggregate(bundle_path):
    root = bundle_path.resolve().parent
    bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    if bundle.get("schema") != "moosicbox.opus.finish-bundle.v1":
        raise ValueError("unsupported acceptance bundle schema")
    source = matrix.native_source_identity()
    if bundle.get("runtime_source_sha256") != source:
        raise ValueError("finish bundle source is not current")
    # Fails on any new unclassified scored/ignored test before accepting artifacts.
    inventory = ci.acceptance_inventory()
    perf_path, perf = read_artifact(root, bundle["performance"])
    verdict = verify_performance(perf, source)
    public_path, public = read_artifact(root, bundle["public_silk"])
    verify_public_silk(public_path, public)
    unit_count = verify_unit_shards(root, bundle["unit_shards"], source)
    platform_root = (root / bundle["platform_directory"]).resolve(strict=True)
    if not platform_root.is_relative_to(root):
        raise ValueError("platform evidence escapes bundle")
    # Existing verifier checks every platform/profile/public/layout shard, log and
    # artifact identity plus current classified test inventories. No remote actions.
    verify_pipeline(root, bundle["pipeline"])
    verify_core_evidence(platform_root)
    ci.compare(SimpleNamespace(root=platform_root))
    platform_report = json.loads((platform_root / "platform-acceptance.json").read_text())
    if platform_report.get("accepted") is not True or platform_report.get("source_sha256") != ci.sources():
        raise ValueError("platform comparison did not accept")
    if matrix.native_source_identity() != source:
        raise ValueError("source changed while aggregating")
    return dict(schema="moosicbox.opus.finish-acceptance.v1", accepted=True,
                runtime_source_sha256=source, performance=verdict,
                ordinary_tests_per_profile=unit_count, classified_required_inventory=inventory,
                evidence_sha256=dict(performance=digest(perf_path), public_silk=digest(public_path)),
                memory_scope="raw child RSS, possibly launch-floor dominated; no isolated allocation proof",
                limitation="acceptance of declared codec/test matrix only, not perceptual certification")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = aggregate(args.bundle)
    except (ValueError, KeyError, OSError, RuntimeError) as error:
        report = dict(schema="moosicbox.opus.finish-acceptance.v1", accepted=False, error=str(error))
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report))
    return 0 if report["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
