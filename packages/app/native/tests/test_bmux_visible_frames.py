"""Deterministic checks for compositor timing bounds and missing matches."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from bmux_visible_frames import FrameSampler, XImage
import ctypes


class BoundsTests(unittest.TestCase):
    def test_footer_pixels_require_all_fixture_quadrants(self):
        from bmux_populated_visual import footer_artwork_pixels

        red, green, blue = b"\x00\x00\xff\xff", b"\x00\xff\x00\xff", b"\xff\x00\x00\xff"
        self.assertFalse(footer_artwork_pixels(b"\xff" * 400)["matched"])
        self.assertFalse(footer_artwork_pixels(red * 100)["matched"])
        self.assertFalse(
            footer_artwork_pixels(red * 20 + green * 20 + blue * 7)["matched"]
        )
        self.assertTrue(
            footer_artwork_pixels(red * 20 + green * 20 + blue * 20)["matched"]
        )

    def test_ximage_header_layout_matches_xlib_64bit(self):
        self.assertEqual(XImage.data.offset, 16)
        self.assertEqual(XImage.bytes_per_line.offset, 44)
        self.assertEqual(XImage.bits_per_pixel.offset, 48)
        self.assertEqual(ctypes.sizeof(ctypes.c_void_p), 8)

    def test_back_uses_capture_end_without_subtracting_cost(self):
        sampler = object.__new__(FrameSampler)
        sampler.grab = mock.Mock(
            side_effect=[(10.01, 10.013, b"old", 4), (10.02, 10.024, b"expected", 4)]
        )
        with tempfile.TemporaryDirectory() as directory, mock.patch(
            "bmux_visible_frames.time.monotonic", side_effect=[10.0, 10.001, 10.015]
        ), mock.patch("bmux_visible_frames.time.sleep"):
            send = mock.Mock()
            result = sampler.cached_back(
                send, (0, 0, 1, 1), b"expected", 4, Path(directory)
            )
            send.assert_called_once()
            self.assertAlmostEqual(result["lower_seconds"], 0.01)
            self.assertAlmostEqual(result["upper_seconds"], 0.024)
            self.assertTrue(
                json.loads((Path(directory) / "visible-back.json").read_text())[
                    "polls"
                ][-1]["matched"]
            )

    def test_missing_back_is_not_a_zero_latency_success(self):
        sampler = object.__new__(FrameSampler)
        with tempfile.TemporaryDirectory() as directory, mock.patch(
            "bmux_visible_frames.time.monotonic", side_effect=[10.0, 11.0]
        ):
            result = sampler.cached_back(
                lambda: None, (0, 0, 1, 1), b"expected", 4, Path(directory)
            )
            self.assertIsNone(result["upper_seconds"])


if __name__ == "__main__":
    unittest.main()
