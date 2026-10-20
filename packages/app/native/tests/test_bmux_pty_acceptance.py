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
    def connection_screen(self, upper="[ Save ]", lower="[ Delete ]  [ Save ]"):
        screen = Screen(140, 30)
        screen.feed(
            (
                f"\x1b[?2026h\x1b[5;126H{upper}\x1b[8;34HConnections"
                f"\x1b[11;130HName:\x1b[13;126HAPI URL:"
                f"\x1b[16;111H{lower}\x1b[?2026l"
            ).encode()
        )
        return screen

    def test_connection_save_uses_form_anchor_with_two_saves(self):
        screen = self.connection_screen()
        self.assertEqual(ACCEPTANCE["connection_save_target"](screen), (124, 15))

    def test_connection_save_accepts_clipped_unrelated_name_save(self):
        screen = self.connection_screen(upper="[ S")
        self.assertEqual(ACCEPTANCE["connection_save_target"](screen), (124, 15))

    def test_connection_save_rejects_missing_or_ambiguous_targets(self):
        for lower in ("[ Delete ]  [ S", "Save", "[ Save ]", "[ Delete ]"):
            with self.subTest(lower=lower):
                self.assertIsNone(
                    ACCEPTANCE["connection_save_target"](
                        self.connection_screen(lower=lower)
                    )
                )
        for extra in (b"[ Delete ]  [ Save ]", b"[ Save ]"):
            screen = self.connection_screen()
            screen.feed(b"\x1b[18;111H" + extra)
            self.assertIsNone(ACCEPTANCE["connection_save_target"](screen))

    def test_connection_save_rejects_missing_duplicate_or_uncommitted_anchors(self):
        for update in (
            b"\x1b[13;126H        ",
            b"\x1b[8;34H           ",
            b"\x1b[11;130H     ",
            b"\x1b[20;100HAPI URL:",
            b"\x1b[?2026h",
        ):
            with self.subTest(update=update):
                screen = self.connection_screen()
                screen.feed(update)
                self.assertIsNone(ACCEPTANCE["connection_save_target"](screen))

    def test_connection_save_rejects_button_beyond_next_form(self):
        screen = self.connection_screen(lower="")
        screen.feed(b"\x1b[15;100HName:\x1b[18;111H[ Delete ]  [ Save ]")
        self.assertIsNone(ACCEPTANCE["connection_save_target"](screen))

    def test_artwork_validation_png_and_rgba_preserve_dimensions_and_pixels(self):
        validation = ACCEPTANCE["ArtworkValidation"]()
        encoded = ACCEPTANCE["artwork_fixture"](1, 64)
        width, height, pixels = ACCEPTANCE["decode_png"](encoded)
        png = {"f": "100", "s": str(width), "v": str(height)}
        rgba = {**png, "f": "32"}
        self.assertEqual(validation.rgba(png, encoded), (width, height, pixels))
        self.assertTrue(validation.matches(png, encoded))
        self.assertTrue(validation.matches(rgba, pixels))
        self.assertEqual(validation.png_decodes, 1)
        tiny = ACCEPTANCE["png_fixture"]()
        self.assertTrue(validation.matches({"f": "100", "s": "2", "v": "2"}, tiny))
        self.assertEqual(
            validation.rgba({"f": "100", "s": "2", "v": "2"}, tiny)[:2], (2, 2)
        )
        transparent = bytearray(pixels)
        for index in range(3, len(transparent), 4):
            transparent[index] = 0
        self.assertFalse(validation.matches(rgba, transparent))
        with self.assertRaises(AssertionError):
            validation.rgba({**png, "s": "63"}, encoded)

    def test_artwork_validation_rejects_invalid_oversized_and_bomb_png(self):
        import struct

        validation = ACCEPTANCE["ArtworkValidation"]()

        def chunk(kind, data):
            return (
                struct.pack("!I", len(data))
                + kind
                + data
                + struct.pack("!I", zlib.crc32(kind + data) & 0xFFFFFFFF)
            )

        oversized = (
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack("!2I5B", 4097, 1, 8, 6, 0, 0, 0))
            + chunk(b"IEND", b"")
        )
        bomb = (
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack("!2I5B", 2, 2, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\0" * 10000))
            + chunk(b"IEND", b"")
        )
        corrupt = bytearray(ACCEPTANCE["png_fixture"]())
        corrupt[20] ^= 1
        for payload in (
            b"not PNG",
            oversized,
            bomb,
            bytes(corrupt),
            ACCEPTANCE["png_fixture"]()[:-4],
        ):
            with self.subTest(size=len(payload)), self.assertRaises(AssertionError):
                validation.rgba({"f": "100", "s": "2", "v": "2"}, payload)
        with self.assertRaises(AssertionError):
            validation.rgba({"f": "100", "s": "4097", "v": "1"}, oversized)

    def test_artwork_validation_cache_has_entry_and_byte_bounds(self):
        validation = ACCEPTANCE["ArtworkValidation"](
            max_entries=1, max_bytes=64 * 64 * 4
        )
        for identity in (1, 2):
            validation.rgba(
                {"f": "100", "s": "64", "v": "64"},
                ACCEPTANCE["artwork_fixture"](identity, 64),
            )
            self.assertLessEqual(len(validation.cache), 1)
            self.assertLessEqual(validation.bytes, 64 * 64 * 4)
        self.assertEqual(validation.png_decodes, 2)
        with self.assertRaises(AssertionError):
            validation.rgba({"f": "100", "s": "65", "v": "65"}, b"")

    def test_cover_png_is_not_decoded_in_screen_feed(self):
        from unittest.mock import patch

        encoded = ACCEPTANCE["artwork_fixture"](1, 64)
        screen = Screen()
        with patch.dict(
            Screen.feed.__globals__, decode_png=lambda _: self.fail("hot decode")
        ):
            # Covers larger than the readiness-icon bound remain transport bytes.
            screen.feed(
                b"\x1b_Ga=t,f=100,s=512,v=512,i=1;"
                + base64.b64encode(encoded)
                + b"\x1b\\"
            )
        self.assertEqual(screen.images[0][1], encoded)

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

    def test_gear_rejects_transparent_wrong_position_and_deleted_payload(self):
        screen = Screen()
        screen.feed(
            b"\x1b_Ga=t,f=32,s=1,v=1,i=3;////AA==\x1b\\"
            b"\x1b[?2026h\x1b[2;18H\x1b_Ga=p,i=3,p=3,c=3,r=1;\x1b\\"
            b"\x1b[?2026l"
        )
        self.assertFalse(screen.gear_ready())
        screen.feed(b"\x1b_Ga=t,f=32,s=1,v=1,i=3;/////w==\x1b\\")
        self.assertTrue(screen.gear_ready())
        screen.feed(b"\x1b[3;18H\x1b_Ga=p,i=3,p=3,c=3,r=1;\x1b\\")
        self.assertFalse(screen.gear_ready())
        screen.feed(b"\x1b[2;18H\x1b_Ga=p,i=3,p=3,c=3,r=1;\x1b\\")
        self.assertTrue(screen.gear_ready())
        screen.feed(b"\x1b_Ga=d,d=A;\x1b\\")
        self.assertFalse(screen.gear_ready())
        screen.feed(b"\x1b[2;18H\x1b_Ga=p,i=3,p=3,c=3,r=1;\x1b\\")
        self.assertFalse(screen.gear_ready())

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
