"""Deterministic checks for compositor timing bounds and missing matches."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from bmux_visible_frames import FrameSampler, XImage
import ctypes


class BoundsTests(unittest.TestCase):
    def test_sequential_pixels_rebaseline_and_reject_preexisting_target(self):
        for already_present in (False, True):
            clock = [10.0]
            sampler = object.__new__(FrameSampler)
            baseline = bytes(range(100)) * 4
            calls = []

            def grab(rect):
                phase = len(calls) % 3
                calls.append(rect)
                raw = baseline if phase != 1 or already_present else b"other"
                return clock[0], clock[0] + 0.004, raw, 4

            def tick():
                clock[0] += 0.2
                return clock[0]

            sampler.grab = grab
            with tempfile.TemporaryDirectory() as directory, mock.patch(
                "bmux_visible_frames.time.monotonic", side_effect=tick
            ), mock.patch("bmux_visible_frames.time.sleep"):
                send = mock.Mock()
                result = sampler.translated_samples(
                    send, (20, 7, 12, 7), 3, Path(directory), sequential_down=True
                )
                for index in range(16):
                    self.assertTrue(
                        (Path(directory) / f"visible-{index}-before.bgra").exists()
                    )
                    self.assertTrue(
                        (Path(directory) / f"visible-{index}-after.bgra").exists()
                    )
                    self.assertTrue(
                        (
                            Path(directory) / f"visible-{index}-target-before.bgra"
                        ).exists()
                    )
                    self.assertIn("baseline_capture_bounds", result["samples"][index])
                    self.assertIn(
                        "target_before_capture_bounds", result["samples"][index]
                    )
                if already_present:
                    self.assertTrue(
                        all(
                            s["status"] == "missing_exact_translation"
                            for s in result["samples"]
                        )
                    )
            self.assertEqual(send.call_count, 16)
            self.assertTrue(all(c.args == (65,) for c in send.call_args_list))
            self.assertEqual(result["motion_pattern"], "sequential_down")
            if already_present:
                self.assertIsNone(result["p95_upper_seconds"])
                self.assertEqual(result["completed"], 0)
            else:
                self.assertEqual(result["completed"], 16)
                self.assertTrue(
                    all(s["baseline_rect"] == (20, 7, 12, 7) for s in result["samples"])
                )
                self.assertTrue(
                    all(s["upper_seconds"] >= 0.2 for s in result["samples"])
                )

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


class WheelObservationTests(unittest.TestCase):
    def classify(self, before, frames):
        from bmux_live_acceptance import classify_wheel_observations

        return classify_wheel_observations(before, frames, 10.0, 11.0)

    def test_centered_retained_four_album_rows_and_negative_artist(self):
        from bmux_live_acceptance import album_click_candidates, centered_ascii_label

        screen = mock.Mock()
        albums = [
            {
                "albumId": 1809,
                "title": "A Different Kind Of Pain (Domestic Version)",
                "artist": "Cold",
            },
            {"albumId": 1521, "title": "A Distant (Dark) Source", "artist": "Hypno5e"},
            {"albumId": 161, "title": "A Dream in Static", "artist": "Earthside"},
            {
                "albumId": 982,
                "title": "A Dying Machine (Deluxe Version)",
                "artist": "Tremonti",
            },
        ]
        for album in albums:
            lines = [""] * 12
            lines[9] = " " * 20 + centered_ascii_label(album["title"], 12)
            lines[10] = " " * 20 + centered_ascii_label(album["artist"], 12)
            screen.text.return_value = "\n".join(lines)
            payloads = [("/menu/albums", {"items": albums})]
            self.assertEqual(
                album_click_candidates(screen, payloads, [(20, 12)])[0][1], album
            )
            lines[10] = " " * 20 + album["artist"].ljust(12)
            screen.text.return_value = "\n".join(lines)
            self.assertEqual(album_click_candidates(screen, payloads, [(20, 12)]), [])

    def test_select_requires_bracket_and_matching_visible_form(self):
        from bmux_live_acceptance import connection_select_target

        endpoint = "http://127.0.0.1:1234"
        text = (
            "(Selected)\nFixture A\nhttp://127.0.0.1:1235\n[ Save ]\nFixture B\nAPI URL:\n"
            + endpoint
            + "\n[ Select ] [ Delete ] [ Save ]"
        )
        self.assertEqual(
            connection_select_target(text, "Fixture B", endpoint, 20), (5, 7)
        )
        self.assertIsNone(connection_select_target(text, "Fixture B", endpoint, 7))
        self.assertIsNone(
            connection_select_target(
                text.replace("[ Select ]", "(Selected)"), "Fixture B", endpoint, 20
            )
        )
        self.assertIsNone(
            connection_select_target(text, "Fixture B", "http://wrong", 20)
        )
        self.assertIsNone(
            connection_select_target(text + "\n[ Select ]", "Fixture B", endpoint, 20)
        )

    def test_retained_wrapped_detail_and_back_display_identities(self):
        from bmux_live_acceptance import (
            detail_metadata_matches,
            displayed_album_positions,
        )

        details = " MoosicBox                                                                      \n Settings                          A Different Kind Of Pain (Domestic           \n Home                              Version)                                     \n Downloads                         Cold                                         \n My Collection                     August 30, 2005                              \n Albums                            *FLAC 44.1 kHz 16-bit*AACAPI:Tidal           \n Artists                                                                        \n                                                                                \n                                                                                \n                         [i Play        [im Options                             \n                                                                                \n                                                                                \n                      #      Title                         Artist     Time      \n                                                                                \n                                                                                \n                      1      Back Home                     Cold       4:31      \n                                                                                \n                                                                                \n                      2      Feel It in Your Heart         Cold       3:46      \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                "
        before = " MoosicBox              2016           2005           2019            2021      \n Settings            API:Tidal     FLAC 44.1 k…   FLAC 44.1 k…     API          \n Home                                                                           \n Downloads                                                                      \n My Collection                                                                  \n Albums                                                                         \n Artists                                                                        \n                                                                                \n                                                                                \n                    A Different…   A Distant (…   A Dream in …    A Dying Mac…  \n                        Cold         Hypno5e       Earthside        Tremonti    \n                        2005           2019           2015            2018      \n                    FLAC 44.1 k…    API:Tidal     FLAC 44.1 k…    FLAC 44.1 k…  \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                "
        after = " MoosicBox              2016           2005           2019            2021      \n Settings            API:Tidal     FLAC 44.1 k…   FLAC 44.1 k…     API          \n Home                                                                           \n Downloads                                                                      \n My Collection                                                                  \n Albums                                                                         \n Artists                                                                        \n                                                                                \n                                                                                \n                    A Different…   A Distant (…   A Dream in …    A Dying Mac…  \n                        Cold         Hypno5e       Earthside        Tremonti    \n                        2005           2019           2015            2018      \n                    FLAC 44.1 k…    API:Tidal     FLAC 44.1 k…    FLAC 44.1 k…  \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                \n                                                                                "
        title = "A Different Kind Of Pain (Domestic Version)"
        self.assertTrue(detail_metadata_matches(details, title, "Cold"))
        self.assertFalse(
            detail_metadata_matches(
                details.replace("Version)", "Wrong)"), title, "Cold"
            )
        )
        self.assertFalse(detail_metadata_matches(details, title, "Other"))
        self.assertFalse(
            detail_metadata_matches(
                details.replace("Version)", " Version)"), title, "Cold"
            )
        )
        self.assertFalse(
            detail_metadata_matches(
                details.replace("Version)", "Version) Other card"), title, "Cold"
            )
        )
        albums = [
            {"albumId": 1809, "title": title, "artist": "Cold"},
            {"albumId": 1521, "title": "A Distant (Dark) Source", "artist": "Hypno5e"},
            {"albumId": 161, "title": "A Dream in Static", "artist": "Earthside"},
            {
                "albumId": 982,
                "title": "A Dying Machine (Deluxe Version)",
                "artist": "Tremonti",
            },
        ]
        payloads = [("/menu/albums", {"items": albums})]
        columns = [(20, 12), (35, 12), (50, 12), (66, 12)]
        screen = mock.Mock()
        screen.text.return_value = before
        expected = {"1809": (20, 9), "1521": (35, 9), "161": (50, 9), "982": (66, 9)}
        self.assertEqual(displayed_album_positions(screen, payloads, columns), expected)
        screen.text.return_value = after
        self.assertEqual(displayed_album_positions(screen, payloads, columns), expected)
        screen.text.return_value = after.replace("Hypno5e", "Unknown")
        self.assertNotEqual(
            displayed_album_positions(screen, payloads, columns), expected
        )
        screen.text.return_value = "\n" + after
        self.assertNotEqual(
            displayed_album_positions(screen, payloads, columns), expected
        )
        duplicate = dict(albums[0], albumId=9999)
        screen.text.return_value = after
        self.assertNotIn(
            "1809",
            displayed_album_positions(
                screen, [("/menu/albums", {"items": albums + [duplicate]})], columns
            ),
        )

    def test_selected_form_and_exact_sidebar_are_required(self):
        from bmux_live_acceptance import (
            selected_connection_visible,
            exact_sidebar_target,
        )

        text = " Settings           (Selected)\n Albums             Name:\n                    Fixture B\n                    API URL:\n                    http://127.0.0.1:1234\n"
        self.assertTrue(
            selected_connection_visible(text, "Fixture B", "http://127.0.0.1:1234", 20)
        )
        self.assertFalse(
            selected_connection_visible(text, "Fixture A", "http://127.0.0.1:1234", 20)
        )
        self.assertFalse(
            selected_connection_visible(
                text.replace("(Selected)", "[ Select ]"),
                "Fixture B",
                "http://127.0.0.1:1234",
                20,
            )
        )
        self.assertEqual(exact_sidebar_target(text, "Albums", 20), (4, 1))
        self.assertIsNone(exact_sidebar_target(text + " Albums\n", "Albums", 20))
        self.assertIsNone(
            exact_sidebar_target(text.replace(" Albums", " AlbumsOther"), "Albums", 20)
        )

    def test_settings_loopback_schemas_and_artwork_barrier(self):
        from bmux_live_acceptance import SettingsLoopback

        a, b = SettingsLoopback("A", True), SettingsLoopback("B")
        try:
            self.assertFalse(a.art_release.is_set())
            self.assertTrue(b.art_release.is_set())
            self.assertNotEqual(a.artwork, b.artwork)
            self.assertEqual(a.response("/config/profiles"), [{"name": "master"}])
            self.assertEqual(
                a.response("/menu/albums?offset=0")["items"][0]["title"], "Loopback A"
            )
            self.assertEqual(
                b.response("/menu/albums?offset=0")["items"][0]["albumId"], 2
            )
            self.assertEqual(a.response("/menu/albums?offset=1")["items"], [])
            for path in ["/player/play", "/scan/start", "/unimplemented"]:
                with self.assertRaises(ValueError):
                    a.response(path)
        finally:
            a.server_close()
            b.server_close()

    def test_unique_ascii_ellipsis_and_ambiguous_loaded_titles(self):
        from bmux_live_acceptance import album_click_candidates, ascii_display_label

        self.assertEqual(ascii_display_label("A Dying Machine", 12), "A Dying Mac…")
        self.assertIsNone(ascii_display_label("楽曲", 12))
        screen = mock.Mock()
        lines = [""] * 12
        lines[8] = " " * 20 + "A Dying Mac…"
        lines[9] = " " * 20 + "  Tremonti  "
        screen.text.return_value = "\n".join(lines)
        album = {"albumId": 982, "title": "A Dying Machine", "artist": "Tremonti"}
        payloads = [("/menu/albums", {"items": [album]})]
        self.assertEqual(
            album_click_candidates(screen, payloads, [(20, 12)])[0][1], album
        )
        # Repeated pages for the same ID do not introduce ambiguity.
        self.assertEqual(
            len(album_click_candidates(screen, payloads * 2, [(20, 12)])), 1
        )
        for other in [
            dict(album, albumId=983),
            dict(album, albumId=983, title="A Dying Machinery", artist="Other"),
        ]:
            self.assertEqual(
                album_click_candidates(
                    screen,
                    payloads + [("/menu/albums", {"items": [other]})],
                    [(20, 12)],
                ),
                [],
            )
        lines[9] = " " * 20 + "Wrong Artist"
        screen.text.return_value = "\n".join(lines)
        self.assertEqual(album_click_candidates(screen, payloads, [(20, 12)]), [])

    def test_hover_assertion_precedes_sampling_phase(self):
        import inspect
        import bmux_live_acceptance

        source = inspect.getsource(bmux_live_acceptance.run)
        self.assertLess(
            source.index('"hover_album_positions"'),
            source.index("sampler.translated_samples("),
        )
        import ast
        import textwrap

        tree = ast.parse(textwrap.dedent(source))
        click = next(
            node
            for node in ast.walk(tree)
            if isinstance(node, ast.FunctionDef) and node.name == "click_ready_albums"
        )
        writes = [
            node
            for node in ast.walk(click)
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "os"
            and node.func.attr == "write"
        ]
        starts = [
            node
            for node in ast.walk(click)
            if isinstance(node, ast.Assign)
            and any(
                isinstance(t, ast.Name) and t.id == "albums_start" for t in node.targets
            )
        ]
        self.assertEqual(len(writes), 1)
        self.assertEqual(len(starts), 1)
        self.assertLess(starts[0].lineno, writes[0].lineno)

    def test_short_title_is_not_a_digit_inside_release_year(self):
        from bmux_live_acceptance import album_click_candidates, album_positions

        lines = [""] * 12
        lines[8] = " " * 20 + "June 2018"
        lines[9] = " " * 20 + "Artist"
        screen = mock.Mock()
        screen.text.return_value = "\n".join(lines)
        payloads = [
            (
                "/menu/albums",
                {"items": [{"albumId": 803, "title": "8", "artist": "Artist"}]},
            )
        ]
        self.assertEqual(album_positions(screen, payloads), {})
        self.assertEqual(album_click_candidates(screen, payloads), [])
        lines[8] = " " * 20 + "  8   "
        lines[9] = " " * 20 + "Artist"
        screen.text.return_value = "\n".join(lines)
        self.assertEqual(
            album_click_candidates(screen, payloads, [(20, 6)])[0][1]["albumId"], 803
        )

    def test_deleted_or_repositioned_cover_cannot_use_old_geometry(self):
        from bmux_live_acceptance import current_cover_placements

        old = ({"a": "p", "i": "1", "p": "2"}, 20, 5)
        new = ({"a": "p", "i": "1", "p": "2"}, 40, 10)
        self.assertEqual(current_cover_placements([old, new]), [new])
        delete = ({"a": "d", "d": "i", "i": "1"}, 0, 0)
        self.assertEqual(current_cover_placements([old, delete]), [])

    def test_cover_elapsed_origin_is_not_clamped_or_negative(self):
        from bmux_live_acceptance import elapsed_receipt

        self.assertIsNone(elapsed_receipt(9.9, 10.0))
        self.assertAlmostEqual(elapsed_receipt(10.02, 10.0), 0.02)

    def test_queue_telemetry_bounded_windows_keep_totals(self):
        from bmux_live_acceptance import QueueTelemetry

        telemetry = QueueTelemetry(limit=2)
        for i in range(3):
            telemetry.record("receipt", i * 4, (i + 1) * 4, 10.0 + i)
        result = telemetry.snapshot()
        self.assertEqual(result["counts"]["receipt"], 3)
        self.assertEqual(result["bytes"]["receipt"], 12)
        self.assertEqual(result["windows"]["receipt"], [(4, 8, 11.0), (8, 12, 12.0)])

    def test_parser_and_forwarder_preserve_chunk_order_and_offsets(self):
        from bmux_live_acceptance import PtyDrain

        screen = mock.Mock()
        drain = PtyDrain(99, screen, bytearray())
        for received, data, begin, end in [(10.0, b"ab", 0, 2), (11.0, b"cde", 2, 5)]:
            drain.parse_queue.put((received, data, begin, end))
            drain.mirror_queue.put((data, begin, end))
        drain.parse_queue.put(None)
        drain.mirror_queue.put(None)
        drain.parse()
        writes = []

        def write(fd, data):
            writes.append(bytes(data[:1]))
            return 1

        with mock.patch("bmux_live_acceptance.os.write", side_effect=write):
            drain.forward()
        self.assertIsNone(drain.error)
        self.assertEqual(b"".join(writes), b"abcde")
        self.assertEqual(
            screen.feed.call_args_list, [mock.call(b"ab"), mock.call(b"cde")]
        )
        telemetry = drain.telemetry.snapshot()
        for stage in ["parse_finish", "mirror_dequeue", "forward_start", "forward_end"]:
            self.assertEqual(
                [(a, b) for a, b, _ in telemetry["windows"][stage]], [(0, 2), (2, 5)]
            )
        for start, end in zip(
            telemetry["windows"]["forward_start"], telemetry["windows"]["forward_end"]
        ):
            self.assertLessEqual(start[2], end[2])

    def test_baseline_is_acquired_after_settle_motion(self):
        from bmux_live_acceptance import settled_album_positions

        state = {"a": (20, 8)}

        def settle(seconds):
            self.assertEqual(seconds, 0.08)
            state["a"] = (20, 5)

        with mock.patch(
            "bmux_live_acceptance.album_positions", side_effect=lambda *_: dict(state)
        ):
            before = settled_album_positions(settle, object(), [])
        result = self.classify(before, [(10.02, dict(state))])
        self.assertEqual(before, {"a": (20, 5)})
        self.assertIsNone(result["seconds"])

    def test_clock_anchor_preserves_bracket(self):
        from bmux_live_acceptance import clock_anchor

        with mock.patch(
            "bmux_live_acceptance.time.monotonic_ns", side_effect=[100, 110]
        ), mock.patch("bmux_live_acceptance.time.time_ns", return_value=999):
            self.assertEqual(
                clock_anchor(),
                {"monotonic_before_ns": 100, "wall_ns": 999, "monotonic_after_ns": 110},
            )

    def test_retained_80_column_clipping_transitions_are_inconclusive(self):
        for before, after in [
            ({"1311": (71, 6)}, {}),
            ({}, {"1311": (71, 0)}),
            ({"1311": (71, 0)}, {"702": (52, 19), "1263": (70, 19)}),
        ]:
            with self.subTest(before=before, after=after):
                result = self.classify(before, [(10.02, after)])
                self.assertEqual(result["status"], "inconclusive_overlap_lost")
                self.assertIsNone(result["seconds"])

    def test_stale_motion_cannot_match_a_later_input(self):
        result = self.classify({"a": (20, 8)}, [(9.99, {"a": (20, 5)})])
        self.assertEqual(result["status"], "no_response_observed")
        self.assertIsNone(result["seconds"])

    def test_no_response_and_unchanged_frame_are_not_success(self):
        before = {"a": (20, 8)}
        for frames, status in [
            ([], "no_response_observed"),
            ([(10.02, before)], "no_upward_movement_observed"),
        ]:
            result = self.classify(before, frames)
            self.assertEqual(result["status"], status)
            self.assertIsNone(result["seconds"])

    def test_only_same_identity_same_column_upward_motion_matches(self):
        before = {"a": (20, 8)}
        for after in [{"b": (20, 5)}, {"a": (21, 5)}, {"a": (20, 9)}]:
            self.assertIsNone(self.classify(before, [(10.02, after)])["seconds"])
        result = self.classify(before, [(10.02, {"a": (20, 5)})])
        self.assertAlmostEqual(result["seconds"], 0.02)

    def test_late_overlap_recovery_preserves_full_delay(self):
        result = self.classify({"a": (20, 8)}, [(10.01, {}), (10.12, {"a": (20, 5)})])
        self.assertEqual(result["status"], "matched")
        self.assertAlmostEqual(result["seconds"], 0.12)
        self.assertGreater(result["seconds"], 0.1)

    def test_response_after_observation_deadline_stays_missing(self):
        result = self.classify({"a": (20, 8)}, [(11.01, {"a": (20, 5)})])
        self.assertIsNone(result["seconds"])


if __name__ == "__main__":
    unittest.main()
