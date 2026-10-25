"""Offline harness tests; no codec build/execution or network required."""

import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from qualification_performance import (
    CASES,
    CANDIDATE_CASES,
    measure,
    verify_receipt,
    performance_verdict,
)


class PerformanceTests(unittest.TestCase):
    def valid_report(self):
        rows = []
        for case in CASES:
            for repetition in range(2):
                timing = dict(
                    total_ns=200_000_000,
                    median_ns=20_000_000,
                    p95_ns=20_000_000,
                    max_ns=20_000_000,
                )
                rows.append(
                    dict(
                        case=case,
                        repetition=repetition,
                        frames=10,
                        audio_ns=200_000_000,
                        debug_assertions=False,
                        encode=copy.copy(timing),
                        decode=copy.copy(timing),
                        input_sha256="a" * 64,
                        packets_sha256="b" * 64,
                        pcm_sha256="c" * 64,
                    )
                )
        return dict(
            cases=rows,
            frames=10,
            repetitions=2,
            measurement_complete=True,
            artifact_stable=True,
            source_before_sha256="same",
            source_after_sha256="same",
        )

    def test_realtime_boundary_and_failure_do_not_use_rounded_derived_multiple(self):
        report = self.valid_report()
        self.assertTrue(performance_verdict(report)["accepted"])
        report["cases"][0]["encode"].update(total_ns=200_000_001, max_ns=20_000_001)
        report["cases"][0]["encode"]["audio_realtime_multiple"] = 999
        self.assertFalse(performance_verdict(report)["accepted"])
        report = self.valid_report()
        report["cases"][0]["decode"].update(total_ns=400_000_000, max_ns=40_000_000)
        self.assertFalse(performance_verdict(report)["accepted"])

    def test_verdict_fails_missing_repeats_duplicate_candidate_and_changed_source(self):
        for mutation in (
            lambda r: r["cases"].pop(),
            lambda r: r["cases"].append(copy.deepcopy(r["cases"][0])),
            lambda r: r["cases"][0].update(case=CANDIDATE_CASES[0]),
            lambda r: r.update(source_after_sha256="changed"),
            lambda r: r.update(artifact_stable=False),
            lambda r: r.update(measurement_complete=False),
            lambda r: r["cases"][0].update(pcm_sha256="wrong"),
            lambda r: r["cases"][0]["encode"].update(total_ns=float("nan")),
            lambda r: r["cases"][0]["encode"].update(p95_ns=1),
        ):
            report = self.valid_report()
            mutation(report)
            self.assertFalse(performance_verdict(report)["accepted"])

    def test_child_metrics_and_linux_peak_memory_are_normalized(self):
        row = dict(
            schema="moosicbox.opus.performance-case.v1",
            debug_assertions=False,
            case="celt-mono",
            frames=10,
            audio_ns=200_000_000,
            encode=dict(total_ns=100_000_000),
            decode=dict(total_ns=50_000_000),
        )

        class Child:
            pid = 123
            returncode = None

        def launch(arguments, stdout, stderr):
            stdout.write(json.dumps(row).encode())
            return Child()

        with patch(
            "qualification_performance.subprocess.Popen", side_effect=launch
        ), patch(
            "qualification_performance.platform.system", return_value="Linux"
        ), patch(
            "qualification_performance.os.wait4",
            create=True,
            return_value=(123, 0, SimpleNamespace(ru_maxrss=42)),
        ):
            result = measure(Path("unused"), "celt-mono", 10, 1)
        self.assertEqual(result["peak_rss_bytes"], 42 * 1024)
        self.assertEqual(result["encode"]["audio_realtime_multiple"], 2)
        self.assertEqual(result["decode"]["audio_realtime_multiple"], 4)
        self.assertEqual(result["encode"]["frames_per_second"], 100)

    def test_standard_cases_keep_highrate_and_auto_coverage(self):
        self.assertEqual(len(CASES), len(set(CASES)))
        self.assertIn("celt-highrate", CASES)
        self.assertIn("auto-stereo", CASES)
        self.assertIn("silk-mono", CASES)
        self.assertIn("hybrid-stereo", CASES)
        self.assertEqual(len(CASES), 14)
        self.assertEqual(set(CANDIDATE_CASES), {"silk-mono-16k", "silk-mono-32k"})
        self.assertFalse(set(CASES) & set(CANDIDATE_CASES))
        for case in (
            "silk-mono-48k",
            "silk-stereo-96k",
            "hybrid-stereo-32k",
            "hybrid-stereo-64k",
            "celt-stereo-32k",
            "celt-stereo-64k",
            "auto-stereo-32k",
            "auto-stereo-64k",
        ):
            self.assertIn(case, CASES)

    def test_receipt_requires_coherent_release_build(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            receipt = dict(
                schema="moosicbox.opus.performance-build.v1",
                source_before_sha256="source",
                source_after_sha256="source",
                executable_sha256="image",
                profile="release",
                command="cargo build --release --locked --offline",
                rustc_version="reviewed compiler",
            )
            path.write_text(json.dumps(receipt), encoding="utf-8")
            self.assertEqual(verify_receipt(path, "source", "image"), receipt)
            for key in (
                "source_before_sha256",
                "source_after_sha256",
                "executable_sha256",
                "profile",
            ):
                path.write_text(
                    json.dumps(dict(receipt, **{key: "wrong"})), encoding="utf-8"
                )
                with self.subTest(key=key), self.assertRaises(ValueError):
                    verify_receipt(path, "source", "image")
            for key in ("command", "rustc_version"):
                path.write_text(
                    json.dumps(dict(receipt, **{key: ""})), encoding="utf-8"
                )
                with self.subTest(key=key), self.assertRaises(ValueError):
                    verify_receipt(path, "source", "image")


if __name__ == "__main__":
    unittest.main()
