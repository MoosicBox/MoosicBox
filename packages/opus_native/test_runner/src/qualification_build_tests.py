"""Receipt capture tests; Cargo/rustc calls are mocked, never executed."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from qualification_build import capture


class BuildReceiptTests(unittest.TestCase):
    def test_receipt_captures_actual_build_and_exclusive_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            image = target / "qualification_performance"
            image.write_bytes(b"fake-test-image")
            receipt_path = target / "receipt.json"
            message = dict(
                reason="compiler-artifact",
                target=dict(name="qualification_performance", kind=["example"]),
                executable=str(image),
                fresh=False,
                profile=dict(debug_assertions=False, opt_level="3"),
            )
            result = SimpleNamespace(
                returncode=0, stdout=json.dumps(message) + "\n", stderr=""
            )
            with patch(
                "qualification_build.native_source_identity",
                side_effect=["before", "before"],
            ), patch(
                "qualification_build.subprocess.check_output",
                return_value="test compiler",
            ), patch(
                "qualification_build.subprocess.run", return_value=result
            ) as build:
                receipt = capture("performance", target, receipt_path)
            self.assertEqual(receipt["source_before_sha256"], "before")
            self.assertEqual(receipt["source_after_sha256"], "before")
            self.assertEqual(build.call_args.kwargs["env"]["CARGO_BUILD_JOBS"], "2")
            self.assertIn("--release", build.call_args.args[0])
            self.assertIn("--locked", build.call_args.args[0])
            self.assertIn("--offline", build.call_args.args[0])
            self.assertEqual(json.loads(receipt_path.read_text()), receipt)
            with patch("qualification_build.subprocess.run") as build:
                with self.assertRaises(FileExistsError):
                    capture("performance", target, receipt_path)
                build.assert_not_called()

    def test_source_mutation_and_failed_build_never_issue_receipt(self):
        for code in (0, 1):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as directory:
                target = Path(directory)
                receipt_path = target / "receipt.json"
                result = SimpleNamespace(returncode=code, stdout="", stderr="")
                with patch(
                    "qualification_build.native_source_identity",
                    side_effect=["before", "after"],
                ), patch(
                    "qualification_build.subprocess.check_output",
                    return_value="test compiler",
                ), patch(
                    "qualification_build.subprocess.run", return_value=result
                ):
                    with self.assertRaises((ValueError, RuntimeError)):
                        capture("performance", target, receipt_path)
                self.assertFalse(receipt_path.exists())


if __name__ == "__main__":
    unittest.main()
