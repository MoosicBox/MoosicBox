"""Harness-only tests: python3 -B -m unittest discover -s .../src -p qualification_inputs_tests.py."""

import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import wave

import qualification_inputs as inputs


class QualificationInputsTests(unittest.TestCase):
    def test_quality_thresholds_are_strict_and_parity_is_bounded(self):
        self.assertEqual(inputs.quality_status(3.0001, 2), "pass")
        self.assertNotEqual(inputs.quality_status(3, 2), "pass")
        self.assertNotEqual(inputs.quality_status(2.9, 0), "pass")
        self.assertNotEqual(inputs.quality_status(30, 3), "pass")
        self.assertNotEqual(inputs.quality_status(float("nan"), 0), "pass")
        self.assertNotEqual(inputs.quality_status(float("inf"), 0), "pass")
        self.assertNotEqual(inputs.quality_status(30, float("nan")), "pass")

    def test_source_changes_and_missing_evidence_fail_acceptance(self):
        self.assertTrue(inputs.accepted(0, False, True, False, True))
        for values in (
            (1, False, True, False, True),
            (0, True, True, False, True),
            (0, False, False, False, True),
            (0, False, True, True, True),
            (0, False, True, False, False),
        ):
            self.assertFalse(inputs.accepted(*values))

    def test_pinned_reference_policy_rejects_changed_revision_and_float(self):
        expected = json.loads(inputs.MANIFEST.read_text(encoding="utf-8"))
        manifest = dict(
            schema=expected["reference_schema"],
            revision=expected["reference_revision"],
            tracked_clean=True,
            fixed_point=True,
            float_api=False,
            intrinsics=False,
            qext=False,
            res24=False,
        )
        inputs.verify_reference(manifest, expected)
        for key, value in (
            ("revision", "wrong"),
            ("fixed_point", False),
            ("tracked_clean", False),
            ("float_api", True),
            ("intrinsics", True),
            ("qext", True),
            ("res24", True),
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                inputs.verify_reference(dict(manifest, **{key: value}), expected)

    def test_reviewed_media_checksum_and_format_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wav_path = root / "fixture.wav"
            with wave.open(str(wav_path), "wb") as wav:
                wav.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
                wav.writeframes(b"\0\0" * 160)
            entry = dict(
                file=wav_path.name,
                bytes=wav_path.stat().st_size,
                sha256=inputs.sha256(wav_path),
                rate=8000,
                channels=1,
            )
            manifest = dict(
                schema="moosicbox.opus.qualification-inputs.v1",
                media=dict(speech=entry, music=copy.copy(entry)),
            )
            path = root / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with patch.object(inputs, "MANIFEST", path):
                _, media = inputs.verify_inputs(root)
                self.assertEqual(media["speech"], wav_path)
                wav_path.write_bytes(wav_path.read_bytes() + b"changed")
                with self.assertRaises(ValueError):
                    inputs.verify_inputs(root)
                wav_path.unlink()
                with self.assertRaises(FileNotFoundError):
                    inputs.verify_inputs(root)

    def test_build_receipt_binds_both_source_snapshots_and_executables(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "build.json"
            receipt = dict(
                schema="moosicbox.opus.qualification-build.v1",
                source_before_sha256="source",
                source_after_sha256="source",
                native_sha256="native",
                reference_sha256="reference",
                command="cargo build --locked --offline",
                profile="release",
            )
            path.write_text(json.dumps(receipt), encoding="utf-8")
            inputs.verify_build_receipt(path, "source", "native", "reference")
            for key in (
                "source_before_sha256",
                "source_after_sha256",
                "native_sha256",
                "reference_sha256",
            ):
                path.write_text(
                    json.dumps(dict(receipt, **{key: "changed"})), encoding="utf-8"
                )
                with self.subTest(key=key), self.assertRaises(ValueError):
                    inputs.verify_build_receipt(path, "source", "native", "reference")


if __name__ == "__main__":
    unittest.main()
