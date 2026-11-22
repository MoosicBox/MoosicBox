"""Explicit, fail-closed CI execution for native Opus; no acceptance baselines written.

Dependency network provisioning is a separate workflow step; reviewed media
network provisioning requires the explicit quality URL. All Cargo commands here are
locked/offline, with two build jobs. Native jobs never build/link the C oracle.
Quality jobs explicitly build the pinned, test-only fixed-point oracle. The
existing strict harness, frozen expectations and reviewed media hashes decide
acceptance; this orchestrator cannot lower their thresholds.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import wave
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
PLATFORMS = {
    "linux-x64": ("Linux", {"x86_64", "amd64"}),
    "linux-arm64": ("Linux", {"aarch64", "arm64"}),
    "macos-x64": ("Darwin", {"x86_64", "amd64"}),
    "windows-x64": ("Windows", {"amd64", "x86_64"}),
}
FEATURES = "celt,silk,hybrid"
LIVE_SHARD_COUNT = 9
LIVE_SHARDS = tuple(f"live-{index}" for index in range(LIVE_SHARD_COUNT))
PUBLIC_SHARDS = tuple(f"public-{index}" for index in range(24))
LAYOUT_SHARDS = tuple(f"layout-{index}" for index in range(40))
HYBRID_SHARDS = tuple(f"hybrid-{index}" for index in range(48))
HYBRID_TEST = "encoder::adaptive::tests::reference_public_hybrid_auto_matrix"
QUALITY_SHARDS = ("core",) + LIVE_SHARDS + PUBLIC_SHARDS + LAYOUT_SHARDS + HYBRID_SHARDS
LAYOUT_TEST = "encoder::adaptive::tests::reference_practical_layout_quality"
PUBLIC_MEDIA_TEST = "encoder::silk::public::public_practical_floor_media_matrix"


def digest(path):
    sha = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def write_json(path, value):
    Path(path).write_text(
        json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )


def sources():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / ".cargo/config.toml"]
    paths += sorted((ROOT / "packages/opus_native").rglob("*.rs"))
    paths += sorted((ROOT / "packages/opus_native").rglob("Cargo.toml"))
    paths += sorted((ROOT / "packages/opus_native").rglob("*.py"))
    for package in ("audio_encoder", "audio_output", "audio_decoder", "opus", "ogg_opus", "player"):
        directory = ROOT / "packages" / package
        paths += sorted(directory.rglob("*.rs"))
        paths += [directory / "Cargo.toml"]
    paths += [
        ROOT / "packages/opus_native/test_runner/fixtures/qualification_inputs.json",
        ROOT / ".github/workflows/opus-native-qualification.yml",
    ]
    # C source provenance is independently checked by reference_oracle.
    return {
        str(path.relative_to(ROOT)): digest(path)
        for path in paths
        if "target" not in path.relative_to(ROOT).parts
    }


def acceptance_environment(environment):
    """Remove diagnostic selectors and unpromoted experiment switches."""
    clean = dict(environment)
    for key in ("MOOSICBOX_OPUS_PRODUCT_CELL", "MOOSICBOX_OPUS_BOUNDED_PREFIX",
                "MOOSICBOX_OPUS_SILK_CASE", "MOOSICBOX_OPUS_LAYOUT_CELL",
                "MOOSICBOX_OPUS_PUBLIC_CELL"):
        clean.pop(key, None)
    return clean


def reject_partial(output):
    if "PARTIAL_DIAGNOSTIC" in output:
        raise RuntimeError("partial diagnostic output cannot establish full acceptance")


class Execution:
    def __init__(self, output):
        self.output = output
        self.commands = []
        self.env = acceptance_environment(dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true"))
        self.env["CARGO_TARGET_DIR"] = str(output / "target")

    def run(self, arguments, timeout=900, env=None):
        index = len(self.commands)
        row = {"argv": list(map(str, arguments)), "timeout_seconds": timeout}
        self.commands.append(row)
        write_json(self.output / "commands.json", self.commands)
        started = time.monotonic()
        # Files bound memory even for verbose test failures. Timeout kills and
        # waits for the direct process; job-level timeout bounds the entire run.
        with (self.output / f"command-{index}.stdout").open("wb") as stdout, (
            self.output / f"command-{index}.stderr"
        ).open("wb") as stderr:
            try:
                result = subprocess.run(
                    row["argv"],
                    cwd=ROOT,
                    env=env or self.env,
                    stdout=stdout,
                    stderr=stderr,
                    timeout=timeout,
                    check=False,
                )
                row["exit_code"] = result.returncode
                if result.returncode:
                    raise RuntimeError(
                        f"command {index} failed ({result.returncode}); see retained logs"
                    )
            except subprocess.TimeoutExpired:
                row["timed_out"] = True
                raise
            finally:
                row["wall_seconds"] = time.monotonic() - started
                write_json(self.output / "commands.json", self.commands)
        return (self.output / f"command-{index}.stdout").read_text(encoding="utf-8")

    def build(self, package, example, profile, binary=False):
        arguments = [
            "cargo",
            "build",
            "--locked",
            "--offline",
            "-j",
            "2",
            "-p",
            package,
            "--bin" if binary else "--example",
            example,
            "--message-format=json-render-diagnostics",
        ]
        if package != "moosicbox_opus_native_libopus":
            arguments += ["--no-default-features", "--features", FEATURES]
        if profile == "release":
            arguments.append("--release")
        output = self.run(arguments)
        paths = set()
        for line in output.splitlines():
            if line.startswith("{"):
                item = json.loads(line)
                if (
                    item.get("reason") == "compiler-artifact"
                    and item.get("executable")
                    and item.get("target", {}).get("name") == example
                ):
                    paths.add(item["executable"])
        if len(paths) != 1:
            raise RuntimeError(f"expected one executable for {example}: {paths}")
        source = Path(paths.pop()).resolve(strict=True)
        destination = self.output / (example + source.suffix)
        shutil.copy2(source, destination)
        return destination


def native_tests(execution, profile, package="moosicbox_opus_native"):
    args = [
        "cargo",
        "test",
        "--locked",
        "--offline",
        "-j",
        "2",
        "-p",
        package,
        "--no-default-features",
        "--features",
        FEATURES,
        "--lib",
        "--tests",
    ]
    # test_runner is a binary crate, not a library.
    if package == "moosicbox_opus_native_test_runner":
        args.remove("--lib")
    if profile == "release":
        args.append("--release")
    return execution.run(args, timeout=1800)


# Explicit acceptance inventory. These invoke public APIs or pinned-reference
# decoder arithmetic. New ignored tests must be classified, never silently lost.
LIVE_INTEGRATION = {
    "celt_plc_tests": "celt_loss_and_recovery_matches_pinned_public_decoder",
    "celt_transition_tests": "transition_payload_and_silence_match_public_reference smooth_fade_matches_pinned_c_arithmetic accumulation_matches_qualified_c_deemphasis",
    "decoder_completion": "short_fec_constituents_match_reference one_byte_constituents_match_reference supported_output_fixtures_match_reference legal_channel_fec_and_short_loss_match_reference nonzero_gain_matches_reference silk_nonzero_to_silence_finish_matches_reference",
    "encoder_adaptive_tests": "reference_hybrid_24k_input_preserves_delay_and_duration reference_silk_high_rate_input_preserves_alignment_and_tail reference_silk_and_hybrid_switching_quality reference_nonstationary_content_retains_alignment_across_modes reference_decodes_transitions_with_continuous_timing_and_signal",
    "encoder_celt_completion": "short_duration_low_budget_music_matches_portable_reference joint_stereo_channel_geometry_matches_reference_with_fixed_packet_budget practical_celt_music_and_attacks_match_portable_reference",
    "encoder_silk_completion": "pinned_c_decodes_native_voiced_speech",
    "encoder_tests": "public_libopus_decodes_nonzero_audio public_libopus_rates_and_durations public_libopus_silk_audio_quality public_libopus_silk_stereo public_libopus_hybrid_quality public_libopus_silk_bitrate_boundaries public_libopus_celt_bitrate_boundaries public_libopus_hybrid_bitrate_boundaries",
    "multistream_tests": "pinned_c_multistream_interoperability pinned_c_multistream_exact_pcm short_constituents_match_public_c_multistream",
    "output_tests": "pinned_reference_deemphasis_matches_all_rates_channels_and_shapes silk_mono_to_stereo_exact_public_api celt_mono_to_stereo_exact_public_api silk_stereo_coded_output_mapping_matches_pinned_api_all_rates celt_stereo_coded_output_mapping_matches_pinned_api_all_rates storage_shrink_matches_upstream_shared_entropy_exactly hybrid_nonzero_persistent_mapping_matches_pinned_api_all_rates hybrid_stereo_nonzero_persistent_mapping_matches_pinned_api_all_rates",
    "stream_decoder_tests": "persistent_silk_duration_changes_match_public_c_decoder public_celt_loss_bursts_and_recovery_match_pinned_api public_celt_loss_output_rates_match_pinned_api public_silk_loss_prototype_matches_pinned_api persistent_mode_transitions_match_public_c legal_empty_constituent_matches_public_c public_hybrid_loss_matches_pinned_api public_hybrid_short_loss_matches_pinned_api missing_lbrr_uses_real_plc_and_preserves_current_packet exhausted_hybrid_and_empty_constituent_match_public_c",
}
LIVE_LIBRARY = (
    HYBRID_TEST,
    "encoder::hybrid::cache_key_tests::native_prefix_scorer_cost_profile",
    LAYOUT_TEST,
    "encoder::silk::public::standalone_reviewed_first_frame_search_parity",
    "encoder::silk::public::standalone_reviewed_pulse_kernel_parity",
    PUBLIC_MEDIA_TEST,
    "encoder::silk::public::public_extended_floor_media_matrix",
    "encoder::silk::public::reviewed_stereo_search_optimization_parity",
    "encoder::hybrid::cache_key_tests::canonical_typed_channel_boundary_proof",
    "encoder::hybrid::cache_key_tests::channel_component_2x2_proof",
    "encoder::hybrid::cache_key_tests::practical_cache_lifecycle_packet_state_identity",
    "encoder::adaptive::tests::practical_prepared_prefix_exact_identity",
    "encoder::silk::public::public_floor_synthetic_matrix",
    "encoder::silk::public::public_floor_real_speech_matrix",
    "encoder::silk::public::public_native_fingerprint_stream_c_parity",
    "encoder::silk::public::public_silence_c_parity",
    "encoder::silk::completion::completion_tests::real_speech_low_rate_budget_and_quality",
    "encoder::adaptive::tests::reference_practical_hybrid_constructor_quality",
    "encoder::adaptive::tests::reference_practical_hybrid_intermediate_quality",
    "encoder::adaptive::tests::reference_practical_auto_steady_quality",
    "encoder::adaptive::tests::reference_public_practical_rd_quality",
    "encoder::adaptive::tests::reference_practical_unredundant_transition_quality",
)
# These remain development evidence, not acceptance and not silently 'passed'.
# Internal floor searches do not substitute for the required public floor tests:
# those intentionally fail until guards/capability satisfy the acceptance contract.
# Cold-prefix diagnostics require independent captured state, not fabricated input.
DEVELOPMENT_ONLY = {
    "prototype_pool48_common_snapshot_diagnostic": "prototype search snapshot, not production policy",
    "prototype_pool48_winner_diagnostic": "prototype candidate selection diagnostic",
    "prototype_pool48_public_matrix": "prototype pool, not the production public matrix",
    "prototype_pool48_layout_matrix": "prototype pool layout experiment",
    "prototype_pool48_cold_matrix": "prototype pool cold-start experiment",
    "prototype_pool48_first_screen": "prototype candidate screening",
    "prototype_pool48_reviewed_music_screen": "prototype candidate screening on reviewed music",
    "typed_full_search_timing": "component timing, not the realtime gate",
    "channel_reuse_full_search_timing": "component timing, not the realtime gate",
    "standalone_high_rate_clipping_root": "clipping root-cause diagnostic",
    "stereo_low_budget_first_infeasible_model": "first infeasible model diagnostic",
    "bounded_reviewed_music_layout": "test-only bounded_prefix_search explicitly enabled; unpromoted candidate, never substitutes for public layout acceptance",
    "captured_prefix_budget_frontier": "development replay requires an actual independent SILK prefix snapshot",
    "reference_public_practical_rd_voiced_10ms": "focused voiced diagnostic; full public RD matrix remains required",
    "practical_rd_voiced_proof_packet_identity": "production/proof state-equivalence diagnostic; not public quality acceptance",
    "reference_unredundant_closed_loop_energy_proof": "test-only closed-loop energy candidate; public unredundant transition acceptance remains required",
    "internal_practical_floor_media_matrix": "internal low-rate candidate; public floor equivalents remain required acceptance gates",
    "comfort_noise_estimate_matches_pinned_cng_state": "private CNG component compiler oracle; not public API acceptance",
    "concealment_parameter_update_matches_pinned_c": "private PLC component compiler oracle; not public API acceptance",
    "recovery_glue_matches_pinned_c_loss_burst_and_recovery": "private recovery component compiler oracle; not public API acceptance",
    "reference_practical_hybrid_split_candidates": "private split search",
    "reference_practical_hybrid_policy_quality": "private candidate policy",
    "reference_practical_hybrid_policy_split4_quality": "experimental private split4",
    "reference_unredundant_prefix_budget_frontier": "budget search diagnostics",
    "reference_unredundant_mono_swb_matrix": "candidate matrix",
    "reference_high_rate_long_silk_candidate_duration_and_quality": "candidate duration policy",
    "reference_mono_short_frame_candidate_quality": "candidate short frame policy",
    "reference_mono_five_ms_candidate_quality": "candidate five-ms policy",
    "captured_cold_prefix_sequence": "requires independent internal captures",
    "captured_cold_prefix_quality": "requires independent internal captures",
    "converted_rate_floor_candidates": "candidate converted-rate floors",
}


def acceptance_inventory():
    required = [
        ("--test", target, name)
        for target, names in LIVE_INTEGRATION.items()
        for name in names.split()
    ]
    required += [("--lib", None, name) for name in LIVE_LIBRARY]
    classified = {name.rsplit("::", 1)[-1] for _, _, name in required} | set(
        DEVELOPMENT_ONLY
    )
    discovered = set()
    for directory in (
        ROOT / "packages/opus_native/tests",
        ROOT / "packages/opus_native/src",
    ):
        for path in directory.rglob("*.rs"):
            discovered.update(
                re.findall(r"#\[ignore[^\n]*\][\s\S]*?fn (\w+)", path.read_text())
            )
    if classified != discovered:
        raise RuntimeError(
            f"ignored test inventory changed: unclassified={sorted(discovered-classified)}, missing={sorted(classified-discovered)}"
        )
    return required


PRODUCT_TESTS = (
    ("moosicbox_audio_encoder", "opus-native", "opus_public_acceptance"),
    ("moosicbox_audio_encoder", "opus-native", "opus_long_frames"),
    ("moosicbox_audio_encoder", "opus-native", "opus_mode_boundaries"),
    ("moosicbox_audio_encoder", "opus-native", "opus_stream_lifecycle"),
    ("moosicbox_player", "encoder-opus-native", "opus_encode_modes"),
)


def product_tests(execution, profile):
    results = []
    for package, features, target in PRODUCT_TESTS:
        argv = ["cargo", "test", "--locked", "--offline", "-j", "2", "-p", package,
                "--no-default-features", "--features", features, "--test", target]
        if profile == "release":
            argv.append("--release")
        argv += ["--", "--test-threads=1", "--show-output"]
        output = execution.run(argv, timeout=1800, env=acceptance_environment(execution.env))
        reject_partial(output)
        match = re.search(r"test result: ok\. ([1-9][0-9]*) passed; 0 failed;", output)
        if not match:
            raise RuntimeError(f"required product suite did not execute: {target}")
        results.append({"package": package, "features": features, "target": target,
                        "passed": int(match[1])})
    return results


def product_live_tests(execution, reference):
    env = dict(acceptance_environment(execution.env), MOOSICBOX_OPUS_REFERENCE_ORACLE=str(reference))
    for name in ("selected_modes_match_qualified_public_c_pcm",
                 "changing_mode_ogg_file_matches_public_c_and_player_seek"):
        output = execution.run([
            "cargo", "test", "--locked", "--offline", "-j", "2", "-p", "moosicbox_player",
            "--no-default-features", "--features", "encoder-opus-native", "--release",
            "--test", "opus_encode_modes", name, "--", "--ignored", "--exact", "--test-threads=1", "--show-output",
        ], timeout=1800, env=env)
        reject_partial(output)
        if not re.search(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", output):
            raise RuntimeError(f"required product oracle test did not execute once: {name}")


def public_cases(shard):
    if shard not in PUBLIC_SHARDS:
        raise ValueError("public case selection requires a public-N shard")
    from qualification_silk import cases

    complete = cases()
    if len(complete) != 192 or len(set(complete)) != 192:
        raise RuntimeError("reviewed public case enumeration changed")
    return complete[int(shard.removeprefix("public-")) :: len(PUBLIC_SHARDS)]


def layout_cell(shard):
    if shard not in LAYOUT_SHARDS:
        raise ValueError("layout selection requires a layout-N shard")
    cells = [
        f"{rate}:{channels}:{duration}:{shape}"
        for rate in (24000, 48000)
        for channels in (1, 2)
        for duration in (10, 20)
        for shape in range(5)
    ]
    return cells[int(shard.removeprefix("layout-"))]


def hybrid_cells(shard):
    if shard not in HYBRID_SHARDS:
        raise ValueError("hybrid selection requires a hybrid-N shard")
    cells = [f"{r}:{c}:{ms}:{b}:{m}:{s}:{bw}:{shape}"
             for r in (24000,48000) for c in (1,2) for ms in (10,20)
             for b in (32000,64000,128000) for m in (0,1) for s in (0,1,2)
             for bw in (0,1) for shape in range(6)]
    return cells[int(shard.removeprefix("hybrid-"))::len(HYBRID_SHARDS)]


def selected_live_tests(shard=None):
    required = acceptance_inventory()
    if shard is None:
        return required
    if shard in HYBRID_SHARDS:
        return [("--lib", None, HYBRID_TEST)]
    if shard in LAYOUT_SHARDS:
        return [("--lib", None, LAYOUT_TEST)]
    if shard in PUBLIC_SHARDS:
        return [("--lib", None, PUBLIC_MEDIA_TEST)]
    if shard not in LIVE_SHARDS:
        raise ValueError("live acceptance requires a known live-N or public-N shard")
    index = int(shard.removeprefix("live-"))
    required = [
        row for row in required if row[2] not in (PUBLIC_MEDIA_TEST, LAYOUT_TEST, HYBRID_TEST)
    ]
    # Stable round-robin selection is independent of hosted runner scheduling.
    return required[index::LIVE_SHARD_COUNT]


def live_acceptance(execution, profile, env, shard=None):
    required = selected_live_tests(shard)
    write_json(
        execution.output / "acceptance-inventory.json",
        {
            "required": required,
            "development_only_not_accepted": DEVELOPMENT_ONLY,
            "normal_tests": "all native lib/tests including unchanged qualification_regression",
            "strict_matrix": "required frozen-C and reviewed media; no --diagnostic",
        },
    )
    if shard in PUBLIC_SHARDS:
        cases_to_run = public_cases(shard)
    elif shard is None:
        # Local full inventory execution still expands public media into exact
        # cases; no inherited filter can quietly reduce a mandatory matrix.
        from qualification_silk import cases

        cases_to_run = cases()
    else:
        cases_to_run = ()
    invocations = []
    for kind, target, name in required:
        filters = cases_to_run if name == PUBLIC_MEDIA_TEST else (None,)
        if name == HYBRID_TEST and shard in HYBRID_SHARDS:
            filters = hybrid_cells(shard)
        for case in filters:
            clean_env = acceptance_environment(env)
            clean_env.pop("MOOSICBOX_OPUS_SILK_CASE", None)
            clean_env.pop("MOOSICBOX_OPUS_LAYOUT_CELL", None)
            clean_env.pop("MOOSICBOX_OPUS_PUBLIC_CELL", None)
            if name == LAYOUT_TEST and shard in LAYOUT_SHARDS:
                clean_env["MOOSICBOX_OPUS_LAYOUT_CELL"] = layout_cell(shard)
            if case is not None:
                key = "MOOSICBOX_OPUS_PUBLIC_CELL" if name == HYBRID_TEST else "MOOSICBOX_OPUS_SILK_CASE"
                clean_env[key] = case
            invocations.append((kind, target, name, case, clean_env))
    for kind, target, name, case, selected_env in invocations:
        args = [
            "cargo",
            "test",
            "--locked",
            "--offline",
            "-j",
            "2",
            "-p",
            "moosicbox_opus_native",
            "--no-default-features",
            "--features",
            FEATURES,
            kind,
        ]
        if target:
            args.append(target)
        if profile == "release":
            args.append("--release")
        args += [
            name,
            "--",
            "--ignored",
            "--exact",
            "--test-threads=1",
            "--show-output",
        ]
        output = execution.run(args, timeout=600, env=selected_env)
        reject_partial(output)
        # Cargo exits zero for a misspelled/filtered test: fail closed on zero runs.
        if not re.search(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", output):
            raise RuntimeError(f"required exact live test did not execute once: {name}")
        if name == HYBRID_TEST and shard in HYBRID_SHARDS:
            if "public coverage executed=1 failed=0 filtered=true" not in output:
                raise RuntimeError("hybrid cell did not prove exactly one successful execution")
        if name == LAYOUT_TEST and shard in LAYOUT_SHARDS:
            # Rust harness diagnostics are normally captured on success. Request
            # --show-output below so the cell's explicit proof count is retained.
            if "layout qualification executed=1 filtered=true" not in output:
                raise RuntimeError(
                    "layout shard did not prove exactly one filtered cell"
                )


def media_environment(directory, destination, base_env, reference):
    from qualification_inputs import verify_inputs

    manifest, paths = verify_inputs(directory)
    with wave.open(str(paths["music"]), "rb") as source:
        if (
            source.getframerate(),
            source.getnchannels(),
            source.getsampwidth(),
            source.getcomptype(),
        ) != (48000, 2, 2, "NONE"):
            raise RuntimeError(
                "reviewed music must be uncompressed stereo signed16 48k WAV"
            )
        count = source.getnframes()
        pcm = source.readframes(count)
    if len(pcm) != count * 4 or count < 32 * 960:
        raise RuntimeError("incomplete/too short reviewed music PCM")
    raw = destination / "reviewed-music-s16le-48k-stereo.pcm"
    with raw.open("xb") as stream:
        stream.write(pcm)
    write_json(
        destination / "media-transform.json",
        {
            "schema": "moosicbox.opus.ci-media-transform.v1",
            "input_manifest": manifest,
            "source_music_sha256": digest(paths["music"]),
            "source_speech_sha256": digest(paths["speech"]),
            "transform": "Python wave readframes all frames; remove RIFF container only; no resample/gain/channel changes",
            "frames": count,
            "rate": 48000,
            "channels": 2,
            "sample_format": "s16le",
            "output_sha256": digest(raw),
            "output_bytes": len(pcm),
        },
    )
    return dict(
        base_env,
        MOOSICBOX_OPUS_QUALIFICATION_MEDIA=str(Path(directory).resolve()),
        MOOSICBOX_OPUS_REFERENCE_ORACLE=str(reference),
        MOOSICBOX_OPUS_SPEECH_WAV=str(paths["speech"]),
        MOOSICBOX_OPUS_HYBRID_REAL_PCM=str(raw),
    )


def strict_matrix(execution, native, reference):
    execution.run(
        [
            sys.executable,
            "-B",
            HERE / "qualification_matrix.py",
            "--native",
            native,
            "--reference",
            reference,
            "--output",
            execution.output / "quality.json",
            "--media-directory",
            execution.output / "media",
            "--build-receipt",
            execution.output / "build-receipt.json",
        ],
        timeout=1800,
    )
    quality = json.loads((execution.output / "quality.json").read_text())
    if quality.get("accepted") is not True or quality.get("failures") != 0:
        raise RuntimeError("strict quality/frozen-C/media qualification not accepted")


def probe_files(directory):
    result = {}
    for path in sorted(directory.iterdir()):
        if path.suffix in {".input", ".pcm", ".packets", ".outcomes"}:
            result[path.name] = {"sha256": digest(path), "bytes": path.stat().st_size}
    if not result or not any(name.endswith(".pcm") for name in result):
        raise RuntimeError("probe produced no PCM fingerprints")
    return result


def run(args):
    expected_system, expected_machines = PLATFORMS[args.platform]
    if (
        platform.system() != expected_system
        or platform.machine().lower() not in expected_machines
    ):
        raise RuntimeError(
            "runner identity does not match requested real-execution platform"
        )
    if args.mode == "quality" and args.profile != "release":
        raise RuntimeError("strict quality CI requires release diagnostics")
    if args.mode == "quality" and not (args.music_wav_url or args.media_directory):
        raise RuntimeError(
            "quality requires explicit reviewed local media or music WAV URL; never skip"
        )
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    execution = Execution(output)
    before = sources()
    write_json(output / "sources-before.json", before)
    compiler = execution.run(["rustc", "-vV"], timeout=30)
    revision = execution.run(["git", "rev-parse", "HEAD"], timeout=30).strip()
    report = {
        "schema": "moosicbox.opus.ci.v2",
        "quality_shard": args.quality_shard if args.mode == "quality" else None,
        "mode": args.mode,
        "platform": args.platform,
        "profile": args.profile,
        "revision": revision,
        "rustc": compiler,
        "machine": platform.machine(),
        "system": platform.system(),
        "accepted": False,
        "source_sha256": before,
        "scope": "executed configurations only; not perceptual or hardware acceptance",
    }
    write_json(output / "ci.json", report)
    if args.mode == "native":
        tree = execution.run(
            [
                "cargo",
                "tree",
                "--locked",
                "--offline",
                "-p",
                "moosicbox_opus_native",
                "--no-default-features",
                "--features",
                FEATURES,
                "--edges",
                "normal,build",
                "--prefix",
                "none",
            ]
        )
        allowed = {
            "moosicbox_opus_native",
            "log",
            "thiserror",
            "thiserror-impl",
            "proc-macro2",
            "unicode-ident",
            "quote",
            "syn",
        }
        unexpected = {
            line.split()[0] for line in tree.splitlines() if line.strip()
        } - allowed
        if unexpected:
            raise RuntimeError(
                f"unreviewed production native graph: {sorted(unexpected)}"
            )
        native_tests(execution, args.profile)
        report["products"] = product_tests(execution, args.profile)
        executable = execution.build(
            "moosicbox_opus_native", "qualification_probe", args.profile
        )
        if sources() != before:
            raise RuntimeError("source changed during native build")
        image = digest(executable)
        # Actual immutable copies are executed twice, not mutable Cargo paths.
        execution.run([executable, output / "data", "6000"], timeout=600)
        execution.run([executable, output / "repeat", "6000"], timeout=600)
        hashes = probe_files(output / "data")
        if probe_files(output / "repeat") != hashes or digest(executable) != image:
            raise RuntimeError("fresh-process packet/PCM/corpus determinism mismatch")
        report.update(
            executable_sha256=image,
            executable_file=executable.name,
            fingerprints=hashes,
        )
    else:
        # Network is opt-in and bounded by the existing checksum-pinned provisioner.
        provision = [sys.executable, "-B", HERE / "qualification_provision.py"]
        if args.media_directory:
            provision += [
                "--source-directory",
                args.media_directory.resolve(strict=True),
            ]
        else:
            provision += ["--download", "--music-wav-url", args.music_wav_url]
        provision += ["--destination", output / "media"]
        execution.run(provision, timeout=180)
        from qualification_matrix import native_source_identity

        receipt_before = native_source_identity()
        native = execution.build(
            "moosicbox_opus_native_test_runner", "native_oracle", args.profile
        )
        reference = execution.build(
            "moosicbox_opus_native_libopus", "reference_oracle", args.profile
        )
        receipt_after = native_source_identity()
        if receipt_after != receipt_before or sources() != before:
            raise RuntimeError("source changed during explicit oracle build")
        receipt = {
            "schema": "moosicbox.opus.qualification-build.v1",
            "source_before_sha256": receipt_before,
            "source_after_sha256": receipt_after,
            "native_sha256": digest(native),
            "reference_sha256": digest(reference),
            "profile": args.profile,
            "command": execution.commands.copy(),
        }
        write_json(output / "build-receipt.json", receipt)
        execution.run([reference, "provenance"], timeout=30)
        if args.quality_shard == "core":
            product_live_tests(execution, reference)
            execution.run(
                [
                    sys.executable,
                    "-B",
                    ROOT / "packages/opus_native/libopus/src/oracle_tests.py",
                    reference,
                ],
                timeout=600,
            )
            native_tests(execution, args.profile, "moosicbox_opus_native_test_runner")
            scoreboard_build = len(execution.commands)
            scoreboard = execution.build(
                "moosicbox_opus_native_test_runner",
                "moosicbox_opus_native_test_runner",
                args.profile,
                binary=True,
            )
            image = digest(scoreboard)
            if sources() != before:
                raise RuntimeError("source changed during scoreboard build")
            execution.run([scoreboard], timeout=1800)
            if digest(scoreboard) != image:
                raise RuntimeError("scoreboard changed during execution")
            write_json(output / "scoreboard-build.json", {
                "schema": "moosicbox.opus.scoreboard-build.v1",
                "sources": before, "image_sha256": image,
                "build_index": scoreboard_build,
            })
            strict_matrix(execution, native, reference)
            report["quality_sha256"] = digest(output / "quality.json")
        else:
            live_env = media_environment(
                output / "media", output, execution.env, reference
            )
            live_acceptance(execution, args.profile, live_env, args.quality_shard)
            report["required_tests"] = selected_live_tests(args.quality_shard)
            if args.quality_shard in HYBRID_SHARDS:
                report["hybrid_cells"] = hybrid_cells(args.quality_shard)
            if args.quality_shard in LAYOUT_SHARDS:
                report["layout_cell"] = layout_cell(args.quality_shard)
            if args.quality_shard in PUBLIC_SHARDS:
                report["public_cases"] = public_cases(args.quality_shard)
            report["media_transform_sha256"] = digest(output / "media-transform.json")
        report.update(native_sha256=digest(native), reference_sha256=digest(reference))
    after = sources()
    write_json(output / "sources-after.json", after)
    if after != before:
        raise RuntimeError("source changed during execution")
    report["accepted"] = True
    write_json(output / "ci.json", report)
    print(
        json.dumps(
            {
                "output": str(output),
                "accepted": True,
                "platform": args.platform,
                "mode": args.mode,
                "profile": args.profile,
            }
        )
    )


def compare(args):
    paths = sorted(args.root.rglob("ci.json"))
    identities, reports, native = set(), [], []
    expected = {
        (pid, profile, "native", None)
        for pid in PLATFORMS
        for profile in ("debug", "release")
    }
    expected |= {
        (pid, "release", "quality", shard)
        for pid in PLATFORMS
        for shard in QUALITY_SHARDS
    }
    for path in paths:
        report = json.loads(path.read_text(encoding="utf-8"))
        if report.get("schema") != "moosicbox.opus.ci.v2":
            raise RuntimeError("unsupported CI evidence schema")
        identity = (
            report["platform"],
            report["profile"],
            report["mode"],
            report.get("quality_shard"),
        )
        if (
            identity not in expected
            or identity in identities
            or report.get("accepted") is not True
        ):
            raise RuntimeError(
                f"unexpected, duplicate or failed execution evidence: {identity}"
            )
        identities.add(identity)
        reports.append(report)
        if report["mode"] == "native":
            expected_products = [(package, features, target) for package, features, target in PRODUCT_TESTS]
            products = report.get("products", [])
            if ([(row["package"], row["features"], row["target"]) for row in products] != expected_products
                    or any(type(row.get("passed")) is not int or row["passed"] < 1 for row in products)):
                raise RuntimeError("required product execution evidence missing")
            if probe_files(path.parent / "data") != report["fingerprints"]:
                raise RuntimeError(
                    "downloaded probe artifact bytes differ from fingerprints"
                )
            filename = Path(report["executable_file"])
            if filename.name != str(filename):
                raise RuntimeError("executable must be an artifact-local basename")
            if digest(path.parent / filename) != report["executable_sha256"]:
                raise RuntimeError("downloaded executable identity mismatch")
            native.append(report)
        elif report["quality_shard"] == "core":
            quality_path = path.parent / "quality.json"
            quality = json.loads(quality_path.read_text(encoding="utf-8"))
            if (
                digest(quality_path) != report["quality_sha256"]
                or quality.get("accepted") is not True
                or quality.get("failures") != 0
            ):
                raise RuntimeError("strict quality evidence missing, failed or changed")
        else:
            required = [
                list(row) for row in selected_live_tests(report["quality_shard"])
            ]
            if report.get("required_tests") != required:
                raise RuntimeError("live shard required-test coverage changed")
            if report["quality_shard"] in HYBRID_SHARDS:
                if report.get("hybrid_cells") != hybrid_cells(report["quality_shard"]):
                    raise RuntimeError("public hybrid 1728-cell coverage changed")
            if report["quality_shard"] in LAYOUT_SHARDS:
                if report.get("layout_cell") != layout_cell(report["quality_shard"]):
                    raise RuntimeError("required 40-cell layout coverage changed")
            if report["quality_shard"] in PUBLIC_SHARDS:
                if report.get("public_cases") != list(
                    public_cases(report["quality_shard"])
                ):
                    raise RuntimeError("public 192-case shard coverage changed")
            if (
                digest(path.parent / "media-transform.json")
                != report["media_transform_sha256"]
            ):
                raise RuntimeError("reviewed media transformation changed")
    if identities != expected:
        raise RuntimeError(
            f"missing real executions: {sorted(expected - identities, key=str)}"
        )
    baseline = native[0]
    compiler = lambda value: [
        line for line in value.splitlines() if not line.startswith("host:")
    ]
    for report in reports:
        for key in ("schema", "revision", "source_sha256"):
            if report[key] != baseline[key]:
                raise RuntimeError(
                    "quality/native shards did not execute a common source snapshot"
                )
        if compiler(report["rustc"]) != compiler(baseline["rustc"]):
            raise RuntimeError("compiler version differs across real platform runs")
    for report in native[1:]:
        if report["fingerprints"] != baseline["fingerprints"]:
            raise RuntimeError("cross-platform exact fingerprints mismatch")
    write_json(
        args.root / "platform-acceptance.json",
        {
            "schema": "moosicbox.opus.platform-acceptance.v1",
            "accepted": True,
            "executions": [list(identity) for identity in sorted(identities, key=str)],
            "required_live_tests_per_platform": len(acceptance_inventory()),
            "revision": baseline["revision"],
            "source_sha256": baseline["source_sha256"],
            "scope": "only actual configurations; no hardware or perceptual acceptance",
        },
    )
    print(
        "Exact packet/PCM/input/corpus SHA-256 agreement: eight native executions; "
        "four strict frozen/media core runs and all 480 required live/public/layout/hybrid shards accepted. "
        "No unexecuted platform claim."
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    runner = sub.add_parser("run")
    runner.add_argument("--mode", choices=("native", "quality"), required=True)
    runner.add_argument("--profile", choices=("debug", "release"), required=True)
    runner.add_argument("--platform", choices=tuple(PLATFORMS), required=True)
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument(
        "--quality-shard",
        choices=QUALITY_SHARDS,
        default="core",
        help="core frozen/media matrix or one mandatory live-test shard",
    )
    media = runner.add_mutually_exclusive_group()
    media.add_argument("--music-wav-url")
    media.add_argument(
        "--media-directory",
        type=Path,
        help="explicit offline directory containing both checksum-reviewed WAVs",
    )
    comparison = sub.add_parser("compare")
    comparison.add_argument("root", type=Path)
    args = parser.parse_args()
    if args.operation == "run":
        run(args)
    else:
        compare(args)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"qualification failed: {error}", file=sys.stderr)
        sys.exit(1)
