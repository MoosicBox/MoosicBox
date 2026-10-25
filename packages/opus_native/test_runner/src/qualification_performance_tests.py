"""Offline harness tests; no codec build/execution or network required."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from qualification_performance import CASES, measure, verify_receipt


class PerformanceTests(unittest.TestCase):
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
