#!/usr/bin/env python3
"""Deterministic self-tests for the acceptance recorder, not app acceptance."""
import base64
from pathlib import Path
import runpy
import unittest
import zlib

ACCEPTANCE = runpy.run_path(str(Path(__file__).with_name("bmux_pty_acceptance.py")))
Screen = ACCEPTANCE["Screen"]


class RecorderTests(unittest.TestCase):
    def test_split_utf8_private_modes_and_absolute_cell_navigation(self):
        screen = Screen(20, 4)
        stream = b"\x1b[>7u\x1b[?1049h\x1b[2J\x1b[2;3HSettings\x1b[3;1H" + "音".encode()
        for byte in stream:
            screen.feed(bytes([byte]))
        self.assertEqual(screen.locate("Settings"), (2, 1))
        self.assertEqual(screen.cells[2][0], "音")
        screen.feed(b"\x1b[2;3H\x1b[K")
        self.assertIsNone(screen.locate("Settings"))

    def test_kitty_chunked_compressed_pixels_and_placement(self):
        screen = Screen()
        pixels = bytes([255, 0, 0, 255, 0, 255, 0, 255])
        compressed = zlib.compress(pixels)
        stream = (
            b"\x1b[3;8H\x1b_Ga=t,f=32,s=2,v=1,i=4,o=z,m=1;"
            + base64.b64encode(compressed[:5])
            + b"\x1b\\"
            + b"\x1b_Gm=0;"
            + base64.b64encode(compressed[5:])
            + b"\x1b\\"
            + b"\x1b_Ga=p,i=4,p=1,c=2,r=1;\x1b\\"
        )
        for offset in range(0, len(stream), 7):
            screen.feed(stream[offset : offset + 7])
        self.assertEqual(len(screen.images), 1)
        params, decoded, position = screen.images[0]
        self.assertEqual(decoded, pixels)
        self.assertEqual((params["s"], params["v"]), ("2", "1"))
        self.assertEqual(position, (7, 2))
        self.assertEqual(
            screen.graphics[-1],
            ({"a": "p", "i": "4", "p": "1", "c": "2", "r": "1"}, 7, 2),
        )

    def test_other_protocol_decoders_check_pixels_and_cursor_placement(self):
        width, height, pixels = ACCEPTANCE["decode_png"](ACCEPTANCE["png_fixture"]())
        self.assertEqual((width, height), (2, 2))
        self.assertEqual(pixels[:4], bytes([255, 0, 0, 255]))
        # Two bands/colors with repeat and carriage return.
        sixel = b'"1;1;2;2#0;2;100;0;0!2@$#1;2;0;100;0!2A'
        width, height, pixels = ACCEPTANCE["decode_sixel"](sixel)
        self.assertEqual((width, height), (2, 2))
        self.assertEqual(pixels, bytes([255, 0, 0, 255] * 2 + [0, 255, 0, 255] * 2))
        screen = Screen()
        screen.feed(
            b"\x1b[3;4H\x1b]1337;File=inline=1;width=2;height=2:"
            + base64.b64encode(ACCEPTANCE["png_fixture"]())
            + b"\x07"
        )
        self.assertEqual(screen.other_images[0][0], "iterm2")
        self.assertEqual(screen.other_images[0][2], (3, 2))
        screen.feed(b"\x1b[4;5H\x1bPq" + sixel + b"\x1b\\")
        self.assertEqual(screen.other_images[1][0], "sixel")
        self.assertEqual(screen.other_images[1][2], (4, 3))

    def test_fixture_has_exact_opaque_quadrants(self):
        png = ACCEPTANCE["png_fixture"]()
        self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
        size = int.from_bytes(png[33:37], "big")
        decoded = zlib.decompress(png[41 : 41 + size])
        self.assertEqual(
            decoded, b"\0\xff\0\0\xff\0\xff\0\xff\0\0\0\xff\xff\xff\xff\xff\xff"
        )


if __name__ == "__main__":
    unittest.main()
