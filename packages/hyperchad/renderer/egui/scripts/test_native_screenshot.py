#!/usr/bin/env python3
"""Launcher contract checks, not renderer/native product acceptance.

Run inside egui-visual-test: python3 -B -m unittest discover -s
packages/hyperchad/renderer/egui/scripts -p 'test_*.py'
"""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("native-screenshot.py")
SPEC = importlib.util.spec_from_file_location("native_capture", SCRIPT)
CAPTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAPTURE)


class LauncherContracts(unittest.TestCase):
    def test_invalid_arguments_have_no_side_effects(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory) / "never-created"
            common = [
                sys.executable,
                "-B",
                str(SCRIPT),
                "--executable",
                "/bin/false",
                "--kind",
                "hyperchad",
                "--artifacts",
                str(artifacts),
            ]
            for extra in (
                ["--timeout", "0"],
                ["--timeout", "nan"],
                ["--scale", "inf"],
                ["--required-id", "a,b"],
                ["--required-id", " "],
                ["--screenshot-env", "DISPLAY"],
                ["--expect-mismatch"],
                ["--baseline", "/missing-baseline"],
            ):
                with self.subTest(extra=extra):
                    result = subprocess.run(common + extra, capture_output=True)
                    self.assertEqual(result.returncode, 2)
                    self.assertFalse(artifacts.exists())

    def test_repeatable_semantic_ids_and_variant(self):
        with patch.object(
            sys,
            "argv",
            [
                str(SCRIPT),
                "--executable",
                "/bin/false",
                "--kind",
                "hyperchad",
                "--artifacts",
                "/tmp/not-created-by-parser",
                "--required-id",
                "one",
                "--required-id",
                "two",
                "--variant",
                "alternate",
            ],
        ):
            options = CAPTURE.arguments()
        self.assertEqual(options.required_id, ["one", "two"])
        self.assertEqual(options.variant, "alternate")

    def test_image_rejections_and_hash_without_mutation(self):
        from PIL import Image
        import hashlib

        with tempfile.TemporaryDirectory() as directory:
            image = Path(directory) / "image.png"
            Image.new("RGB", (64, 64), "black").save(image)
            with self.assertRaisesRegex(ValueError, "uniform"):
                CAPTURE.image_info(image)
            Image.new("RGB", (8, 8), "black").save(image)
            with self.assertRaisesRegex(ValueError, "32x32"):
                CAPTURE.image_info(image)
            image.write_bytes(b"not PNG")
            with self.assertRaises(OSError):
                CAPTURE.image_info(image)
            pixels = Image.new("RGB", (64, 64), "black")
            pixels.putpixel((1, 1), (255, 0, 0))
            pixels.save(image)
            before = image.read_bytes()
            info = CAPTURE.image_info(image)
            self.assertEqual((info["width"], info["height"]), (64, 64))
            self.assertEqual(info["sha256"], hashlib.sha256(before).hexdigest())
            self.assertEqual(image.read_bytes(), before)

    def test_cleanup_reaps_owned_process(self):
        process = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            start_new_session=True,
        )
        try:
            CAPTURE.stop(process)
            self.assertIsNotNone(process.poll())
        finally:
            if process.poll() is None:
                CAPTURE.stop(process)


if __name__ == "__main__":
    unittest.main()
