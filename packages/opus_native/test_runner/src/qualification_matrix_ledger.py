"""Offline intended-support inventory from current runner definitions; no codec launches.

The ledger enumerates requested cells, not passing qualification. Raw loop metadata
is retained from each actual runner function to expose drift and omissions.
"""

import argparse
import ast
import hashlib
import json
from pathlib import Path

import qualification_matrix as matrix
import qualification_ci as ci
import qualification_performance as performance
import qualification_silk as silk
from qualification_inputs import MANIFEST, verify_inputs


def build_ledger(media_directory):
    source = Path(matrix.__file__)
    tree = ast.parse(source.read_text(encoding="utf-8"))
    functions = {
        node.name: node for node in tree.body if isinstance(node, ast.FunctionDef)
    }
    metadata = {}
    for name in (
        "frozen",
        "compare",
        "quality",
        "silence_matrix",
        "gain_matrix",
        "decode_matrix",
        "long_stream",
        "adversarial",
    ):
        function = functions[name]
        metadata[name] = dict(
            source_sha256=hashlib.sha256(
                ast.get_source_segment(
                    source.read_text(encoding="utf-8"), function
                ).encode()
            ).hexdigest(),
            loops=[
                dict(target=ast.unparse(node.target), values=ast.unparse(node.iter))
                for node in ast.walk(function)
                if isinstance(node, ast.For)
            ],
            evidence="required_not_executed_by_ledger",
        )
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    media = []
    for label, entry in manifest["media"].items():
        path = Path(media_directory) / entry["file"]
        exists = path.is_file()
        actual = hashlib.sha256(path.read_bytes()).hexdigest() if exists else None
        media.append(
            dict(
                label=label,
                path=str(path),
                required_sha256=entry["sha256"],
                actual_sha256=actual,
                reviewed_available=actual == entry["sha256"],
                license=entry["license"],
                license_url=entry["license_url"],
                publication_permission_inferred=False,
            )
        )
    compare_cells = []
    for rate in (8000, 12000, 16000, 24000, 48000):
        for channels in (1, 2):
            for mode in (1000, 1001, 1002):
                if mode == 1001 and rate < 24000:
                    continue
                for duration in (
                    (10, 20, 40, 60)
                    if mode == 1000
                    else (10, 20) if mode == 1001 else (2.5, 5, 10, 20)
                ):
                    bitrate = (
                        96000
                        if mode == 1000
                        else 128000 if mode == 1001 else 64000 * channels
                    )
                    compare_cells.append(
                        dict(
                            rate=rate,
                            channels=channels,
                            mode=mode,
                            duration_ms=duration,
                            bitrate=bitrate,
                            interface="public_constructor",
                            evidence="pending_actual_report",
                        )
                    )
    media_cells = []
    for label, entry in manifest["media"].items():
        rate, channels = entry["rate"], entry["channels"]
        for mode in (0, 1000, 1001, 1002) if rate >= 24000 else (0, 1000, 1002):
            for bitrate in sorted(
                {16000 * channels, 32000 * channels, 64000 * channels, 128000, 256000}
            ):
                media_cells.append(
                    dict(
                        media=label,
                        rate=rate,
                        channels=channels,
                        mode=mode,
                        bitrate=bitrate,
                        duration_ms=20,
                        interface="public_constructor_requested_not_necessarily_supported",
                        evidence="pending_actual_report",
                    )
                )
    silence_cells = []
    for rate, modes in ((16000, (0, 1000, 1002)), (48000, (0, 1001, 1002))):
        for channels in (1, 2):
            for mode in modes:
                for bitrate in (
                    (32000, 64000, 128000) if channels == 2 else (16000, 32000, 128000)
                ):
                    silence_cells.append(
                        dict(
                            rate=rate,
                            channels=channels,
                            mode=mode,
                            bitrate=bitrate,
                            frames=12,
                            duration_ms=20,
                            evidence="pending_actual_report",
                        )
                    )
    return dict(
        schema="moosicbox.opus.intended-support-ledger.v1",
        runtime_source_sha256=matrix.native_source_identity(),
        runner_metadata=metadata,
        public_roundtrip_cells=compare_cells,
        real_media_cells=media_cells,
        silence_cells=silence_cells,
        real_media=media,
        missing_reviewed_media=[
            row["label"] for row in media if not row["reviewed_available"]
        ],
        real_media_matrix=dict(
            modes={
                "speech_8khz": [0, 1000, 1002],
                "music_48khz": [0, 1000, 1001, 1002],
            },
            bitrate_rule="unique 16000*channels,32000*channels,64000*channels,128000,256000; 20ms; actual WAV layout",
            limitation="does not cover every rate/layout/duration/low-level combination",
        ),
        public_performance_cases=list(performance.CASES),
        candidate_performance_cases=[
            dict(case=case, interface="below_current_public_guard", accepted=False)
            for case in performance.CANDIDATE_CASES
        ],
        silk_internal_cells=[
            dict(
                case=case,
                interface="internal_test_only_candidate",
                evidence="pending_verified_log",
            )
            for case in silk.cases()
        ],
        silk_required_count=len(silk.cases()),
        silk_exact_test=silk.TEST,
        missing_intended_media=[
            "reviewed fullband speech and rate/layout variants if required for broader public support"
        ],
        required_scored_tests=ci.acceptance_inventory(),
        required_layout_cells=[ci.layout_cell(shard) for shard in ci.LAYOUT_SHARDS],
        required_platform_executions=[dict(platform=pid, profile=profile)
                                      for pid in ci.PLATFORMS for profile in ("debug", "release")],
        required_pipeline_suites=list(ci.PRODUCT_TESTS) + [
            ("moosicbox_audio_output", "opus-native", "opus_native_pipeline"),
            ("moosicbox_files", "files,encoder-opus-native", "opus_pipeline"),
        ],
        unit_completion_policy="actual --list minus --ignored --list; complete full suite or bounded shards in debug/release; timeout is failure",
        required_final_performance=dict(frames=500, minimum_repetitions=2,
                                        minimum_encode_realtime_multiple=1,
                                        minimum_decode_realtime_multiple=1,
                                        raw_child_rss_scope="actual raw totals where available; explicit unavailable-platform limitation otherwise; no allocation budget"),
        accepted=False,
        limitation="inventory only; test presence, synthetic timing and internal candidates do not establish public/perceptual/platform acceptance",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media-directory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = build_ledger(args.media_directory)
    args.output.write_text(
        json.dumps(report, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    print(
        json.dumps(
            dict(
                output=str(args.output),
                public_roundtrip_cells=len(report["public_roundtrip_cells"]),
                internal_silk_cells=report["silk_required_count"],
                missing_reviewed_media=report["missing_reviewed_media"],
                accepted=False,
            )
        )
    )


if __name__ == "__main__":
    main()
