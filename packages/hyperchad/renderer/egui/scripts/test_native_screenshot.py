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
import copy
import threading
import time
import json
from types import SimpleNamespace
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


class FixtureContracts(unittest.TestCase):
    def modal_state(self):
        def node(parent, tag, rect, maximum=0, offset=0):
            return dict(
                parent=parent,
                tag=tag,
                rect_pixels=rect,
                visible=rect is not None,
                enabled=True,
                generation=0,
                scroll_max_pixels=[0, maximum],
                scroll_pixels=[0, offset],
            )

        return dict(
            window_id=42,
            frame=1,
            client_pixels=[100, 100],
            nodes={
                "overlay": node(None, "div", [0, 0, 100, 100]),
                "panel": node("overlay", "div", [20, 10, 60, 80], 200),
                "heading": node("panel", "heading", [25, 15, 40, 10]),
                "row": node("panel", "div", [25, 30, 40, 10]),
                "close": node("panel", "button", [70, 15, 5, 5]),
            },
        )

    def test_structural_targets_and_ambiguity(self):
        state = self.modal_state()
        self.assertEqual(
            CAPTURE.select_node(state, {"scroll_ancestor": {"tag": "heading"}}, True)[
                0
            ],
            "panel",
        )
        self.assertEqual(
            CAPTURE.select_node(
                state, {"button_within": {"tag": "div", "parent": None}}, True
            )[0],
            "close",
        )
        state["nodes"]["extra"] = copy.deepcopy(state["nodes"]["close"])
        with self.assertRaises(ValueError):
            CAPTURE.select_node(state, {"button_within": {"parent": None}})
        state["nodes"]["overlay"]["scroll_max_pixels"] = [0, 20]
        with self.assertRaises(ValueError):
            CAPTURE.select_node(state, {"scroll_ancestor": {"tag": "heading"}})
        state["nodes"]["panel"]["scroll_max_pixels"] = [0, 0]
        state["nodes"]["overlay"]["scroll_max_pixels"] = [0, 0]
        with self.assertRaises(ValueError):
            CAPTURE.select_node(state, {"scroll_ancestor": {"tag": "heading"}})

    def test_outside_point_is_derived_and_contained(self):
        state = self.modal_state()
        x, y = CAPTURE.outside_point(state, {"parent": None}, {"tag": "heading"})
        self.assertTrue(0 <= x < 100 and 0 <= y < 100)
        self.assertFalse(20 <= x < 80 and 10 <= y < 90)
        state["nodes"]["panel"]["rect_pixels"] = [0, 0, 100, 100]
        with self.assertRaises(ValueError):
            CAPTURE.outside_point(state, {"parent": None}, {"tag": "heading"})

    def test_sticky_movement_identity_and_negative_gate(self):
        before = self.modal_state()
        after = copy.deepcopy(before)
        after["frame"] = 2
        after["nodes"]["panel"]["scroll_pixels"] = [0, 20]
        after["nodes"]["row"]["rect_pixels"] = [25, 10, 40, 10]
        self.assertTrue(
            CAPTURE.sticky_check(
                before, after, {"tag": "heading"}, {"tag": "div", "parent": "panel"}
            )["header_stationary"]
        )
        broken = copy.deepcopy(after)
        broken["nodes"]["heading"]["rect_pixels"][1] += 5
        with self.assertRaises(ValueError):
            CAPTURE.sticky_check(
                before, broken, {"tag": "heading"}, {"tag": "div", "parent": "panel"}
            )
        broken = copy.deepcopy(after)
        broken["nodes"]["row"]["generation"] = 1
        with self.assertRaises(ValueError):
            CAPTURE.sticky_check(
                before, broken, {"tag": "heading"}, {"tag": "div", "parent": "panel"}
            )
        broken = copy.deepcopy(after)
        broken["nodes"]["panel"]["scroll_pixels"] = [0, 0]
        with self.assertRaises(ValueError):
            CAPTURE.sticky_check(
                before, broken, {"tag": "heading"}, {"tag": "div", "parent": "panel"}
            )

    def test_exact_canvas_crop_equality_and_honest_negative(self):
        from PIL import Image

        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            before = self.modal_state()
            before["nodes"]["canvas"] = dict(
                parent=None,
                tag="canvas",
                rect_pixels=[10, 10, 20, 20],
                visible=True,
                enabled=True,
                generation=0,
            )
            after = copy.deepcopy(before)
            after["frame"] = 2
            a = Image.new("RGBA", (100, 100), "black")
            a.save(directory / "capture-0001.png")
            b = a.copy()
            b.putpixel((1, 1), (255, 255, 255, 255))
            b.save(directory / "capture-0002.png")
            selector = {"tag": "canvas"}
            CAPTURE.compare_canvas(directory, 1, before, 2, after, selector, True)
            with self.assertRaises(ValueError):
                CAPTURE.compare_canvas(directory, 1, before, 2, after, selector, False)
            b.putpixel((15, 15), (255, 0, 0, 255))
            b.save(directory / "capture-0002.png")
            CAPTURE.compare_canvas(directory, 1, before, 2, after, selector, False)
            with self.assertRaises(ValueError):
                CAPTURE.compare_canvas(directory, 1, before, 2, after, selector, True)
            after["nodes"]["canvas"]["rect_pixels"] = [10, 10, 200, 20]
            with self.assertRaises(ValueError):
                CAPTURE.compare_canvas(directory, 1, before, 2, after, selector, True)

    def test_delayed_native_wheel_waits_effect_and_two_distinct_stable_frames(self):
        before = self.modal_state()
        before.update(
            revision=2, ready=True, pending=dict(routes=0, resources=0), diagnostics=[]
        )
        frames = []
        for frame, revision, offset, ready in [
            (2, 2, 0, True),
            (3, 3, 192, False),
            (4, 3, 192, True),
            (4, 3, 192, True),
            (5, 3, 200, True),
            (6, 3, 200, True),
        ]:
            state = copy.deepcopy(before)
            state.update(frame=frame, revision=revision, ready=ready)
            state["nodes"]["panel"]["scroll_pixels"] = [0, offset]
            frames.append(state)
        consumer = CAPTURE.InteractionConsumer(
            Path("/unused"),
            "nonce",
            SimpleNamespace(poll=lambda: None),
            time.monotonic() + 2,
            None,
        )
        observed = []

        def state():
            value = frames.pop(0)
            observed.append(value["frame"])
            return value

        consumer.state = state
        with patch.object(CAPTURE.time, "sleep"):
            result = consumer.wait_wheel(
                before,
                dict(
                    op="wheel",
                    selector={"scroll_ancestor": {"tag": "heading"}},
                    ticks=4,
                    settle_frames=2,
                ),
            )
        self.assertEqual(result["frame"], 6)
        self.assertEqual(observed, [2, 3, 4, 4, 5, 6])
        self.assertEqual(result["revision"], 3)
        # A request would now use this current effect's revision, never rebase a
        # rejected capture. Existing stale-reply regression remains unchanged.

    def test_effective_physical_scale_not_just_requested_metadata(self):
        self.assertEqual(
            CAPTURE.scale_check(dict(client_pixels=[1250, 875]), [1000, 700], 1.25)[
                "effective_scale_xy"
            ],
            [1.25, 1.25],
        )
        CAPTURE.scale_check(dict(client_pixels=[1000, 700]), [1000, 700], 1.0)
        with self.assertRaises(ValueError):
            CAPTURE.scale_check(dict(client_pixels=[1000, 700]), [1000, 700], 1.25)
        with self.assertRaises(ValueError):
            CAPTURE.scale_check(dict(client_pixels=[1250, 700]), [1000, 700], 1.25)

    def test_font_preverification_and_plan(self):
        import hashlib

        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / "font.ttf"
            p.write_bytes(b"local-test-bytes")
            digest = hashlib.sha256(p.read_bytes()).hexdigest()
            options = SimpleNamespace(
                font_regular=p,
                font_regular_sha256=digest,
                font_bold=p,
                font_bold_sha256=digest,
                interaction_plan=True,
            )
            self.assertEqual(set(CAPTURE.verified_fonts(options)), {"regular", "bold"})
            options.font_bold_sha256 = "0" * 64
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                CAPTURE.verified_fonts(options)
            options.font_bold_sha256 = digest
            options.font_bold = None
            with self.assertRaises(ValueError):
                CAPTURE.verified_fonts(options)
        plan = CAPTURE.validate_plan(
            CAPTURE.bounded_json(SCRIPT.with_name("production-template.plan.json"))
        )
        self.assertEqual(len(plan["phases"]), 29)
        self.assertEqual(sum(p["op"] == "capture" for p in plan["phases"]), 8)
        with self.assertRaises(ValueError):
            CAPTURE.validate_plan(
                dict(
                    version=1,
                    phases=[
                        dict(
                            op="pixels",
                            before=1,
                            after=2,
                            selector={"tag": "canvas"},
                            equal=True,
                        )
                    ],
                )
            )


class InteractionContracts(unittest.TestCase):
    def test_settings_plan_home_selector_disambiguates_logo(self):
        plan = CAPTURE.validate_plan(
            CAPTURE.bounded_json(SCRIPT.with_name("settings-disconnected.plan.json"))
        )
        self.assertEqual(len(plan["phases"]), 24)
        self.assertEqual(sum(phase["op"] == "capture" for phase in plan["phases"]), 6)
        selector = plan["phases"][16]["selector"]
        self.assertEqual(selector, dict(tag="anchor", href="/", text="Home"))
        state = dict(
            nodes={
                "logo": dict(
                    tag="anchor", href="/", text="MoosicBox", visible=True, enabled=True
                ),
                "home": dict(
                    tag="anchor", href="/", text="Home", visible=True, enabled=True
                ),
            }
        )
        with self.assertRaises(ValueError):
            CAPTURE.select_node(state, dict(tag="anchor", href="/"), True)
        self.assertEqual(CAPTURE.select_node(state, selector, True)[0], "home")

    def test_owned_authority_used_and_restored_on_success_and_failure(self):
        # XOpenDisplay runs in this process, not the child's env mapping.
        for previous in (None, "/unrelated/desktop-authority"):
            for fail in (False, True):
                with self.subTest(previous=previous, fail=fail):
                    with patch.dict(CAPTURE.os.environ, {}, clear=True):
                        if previous is not None:
                            CAPTURE.os.environ["XAUTHORITY"] = previous
                        CAPTURE.os.environ["DISPLAY"] = ":0"
                        sentinel = object()

                        def open_native(*args):
                            self.assertEqual(
                                args, (":12345", 123, Path("/x11"), Path("/xtst"))
                            )
                            self.assertEqual(
                                CAPTURE.os.environ["XAUTHORITY"],
                                "/fresh/private/Xauthority",
                            )
                            if fail:
                                raise ValueError("simulated XOpenDisplay failure")
                            return sentinel

                        with patch.object(
                            CAPTURE, "NativeXTest", side_effect=open_native
                        ):
                            args = (
                                ":12345",
                                123,
                                Path("/x11"),
                                Path("/xtst"),
                                "/fresh/private/Xauthority",
                            )
                            if fail:
                                with self.assertRaisesRegex(ValueError, "XOpenDisplay"):
                                    CAPTURE.open_owned_native(*args)
                            else:
                                self.assertIs(
                                    CAPTURE.open_owned_native(*args), sentinel
                                )
                        self.assertEqual(CAPTURE.os.environ.get("XAUTHORITY"), previous)
                        self.assertEqual(CAPTURE.os.environ["DISPLAY"], ":0")
                        if previous is None:
                            self.assertNotIn("XAUTHORITY", CAPTURE.os.environ)

    def state(self):
        return dict(
            version=1,
            nonce="nonce",
            pid=123,
            window_id=456,
            revision=7,
            frame=42,
            route="/settings",
            ready=True,
            diagnostics=[],
            pending=dict(routes=0, resources=0),
            client_pixels=[100, 100],
            focused_key="input",
            nodes={
                "input": dict(
                    parent=None,
                    id=None,
                    tag="input",
                    name="name",
                    href=None,
                    text="",
                    value="temporary",
                    visible=True,
                    enabled=True,
                    rect_pixels=[10, 10, 50, 20],
                    scroll_pixels=[0, 0],
                    scroll_max_pixels=[0, 0],
                )
            },
        )

    def test_state_provenance_geometry_and_tree_rejections(self):
        state = self.state()
        self.assertIs(CAPTURE.validate_state(state, "nonce", 123), state)
        mutations = [
            ("nonce", "wrong"),
            ("pid", 124),
            ("window_id", 0),
            ("frame", -1),
            ("diagnostics", ["failed image"]),
            ("client_pixels", [float("nan"), 100]),
            ("focused_key", "missing"),
        ]
        for key, value in mutations:
            with self.subTest(key=key):
                broken = copy.deepcopy(state)
                broken[key] = value
                with self.assertRaises(ValueError):
                    CAPTURE.validate_state(broken, "nonce", 123)
        for field, value in [
            ("parent", "input"),
            ("rect_pixels", [90, 10, 50, 20]),
            ("scroll_pixels", [0, 20]),
            ("value", 123),
        ]:
            broken = copy.deepcopy(state)
            broken["nodes"]["input"][field] = value
            with self.assertRaises(ValueError):
                CAPTURE.validate_state(broken, "nonce", 123)

    def test_selectors_and_plan_fail_closed(self):
        state = self.state()
        self.assertEqual(CAPTURE.select_node(state, {"name": "name"}, True)[0], "input")
        state["nodes"]["duplicate"] = copy.deepcopy(state["nodes"]["input"])
        with self.assertRaises(ValueError):
            CAPTURE.select_node(state, {"name": "name"}, True)
        for phase in [
            dict(op="key", key="Return"),
            dict(op="text", text="x\n"),
            dict(op="wheel", selector={"id": "pane"}, ticks=21),
            dict(op="assert", expect={"focused": True}),
            dict(op="click", selector={"x": 100}),
            dict(op="capture", path="/tmp/output"),
        ]:
            with self.subTest(phase=phase):
                with self.assertRaises(ValueError):
                    CAPTURE.validate_plan(dict(version=1, phases=[phase]))
        CAPTURE.validate_plan(
            dict(
                version=1,
                phases=[
                    dict(op="capture"),
                    dict(op="click", selector={"href": "/settings"}),
                    dict(op="text", text="temporary"),
                ],
            )
        )

    def test_native_input_fails_before_events_without_overflow_or_safe_target(self):
        native = CAPTURE.NativeXTest.__new__(CAPTURE.NativeXTest)
        native.verify = lambda state: None
        state = self.state()
        with self.assertRaisesRegex(ValueError, "no-overflow"):
            native.input(state, dict(op="wheel", selector={"name": "name"}, ticks=2))
        state["nodes"]["input"]["text"] = "Save"
        with self.assertRaisesRegex(ValueError, "unsafe"):
            native.input(state, dict(op="click", selector={"name": "name"}))
        with self.assertRaisesRegex(ValueError, "private high display"):
            CAPTURE.NativeXTest(":0", 123, Path("/unused"), Path("/unused"))

    def test_protocol_file_limits_and_atomic_no_path_redirection(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            path.write_text('{"version":1,"version":2}')
            with self.assertRaises(ValueError):
                CAPTURE.bounded_json(path)
            path.write_text('{"value":NaN}')
            with self.assertRaises(ValueError):
                CAPTURE.bounded_json(path)
            path.write_bytes(b" " * (CAPTURE.JSON_LIMIT + 1))
            with self.assertRaises(ValueError):
                CAPTURE.bounded_json(path)
            link = Path(directory) / "link.json"
            link.symlink_to(path)
            with self.assertRaises(OSError):
                CAPTURE.bounded_json(link)
            CAPTURE.atomic_json(path, dict(version=1))
            self.assertEqual(CAPTURE.bounded_json(path), dict(version=1))

    def test_real_file_exchange_capture_then_finish(self):
        # Mock producer/native adapter validates wire contract only, not native product proof.
        from PIL import Image

        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            state = self.state()
            CAPTURE.atomic_json(directory / "state.json", state)
            app = SimpleNamespace(pid=123, returncode=None, poll=lambda: None)
            consumer = CAPTURE.InteractionConsumer(
                directory, "nonce", app, time.monotonic() + 3, None
            )
            failures = []

            def producer():
                try:
                    for seq, op in [(1, "capture"), (2, "finish")]:
                        until = time.monotonic() + 2
                        while time.monotonic() < until:
                            try:
                                req = CAPTURE.bounded_json(directory / "request.json")
                            except FileNotFoundError:
                                req = {}
                            if req.get("seq") == seq:
                                break
                            time.sleep(0.005)
                        else:
                            raise RuntimeError("no request")
                        self.assertEqual(
                            req,
                            dict(
                                version=1,
                                nonce="nonce",
                                seq=seq,
                                op=op,
                                after_frame=42,
                                expected_revision=7,
                            ),
                        )
                        captured = copy.deepcopy(state)
                        captured["frame"] = 43
                        if op == "capture":
                            image = Image.new("RGB", (100, 100), "black")
                            image.putpixel((1, 1), (255, 0, 0))
                            image.save(directory / f"capture-{seq:04d}.png")
                        CAPTURE.atomic_json(
                            directory / f"reply-{seq:04d}.json",
                            dict(
                                version=1,
                                nonce="nonce",
                                seq=seq,
                                status="ok",
                                error=None,
                                state=captured,
                            ),
                        )
                except BaseException as error:
                    failures.append(error)

            thread = threading.Thread(target=producer)
            thread.start()
            try:
                self.assertEqual(
                    consumer.request(state, "capture")["state"]["frame"], 43
                )
                consumer.request(state, "finish")
            finally:
                thread.join(timeout=4)
            self.assertFalse(thread.is_alive())
            if failures:
                raise failures[0]

    def test_stale_reply_rejected_and_no_ready_surrogate(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            state = self.state()
            app = SimpleNamespace(pid=123, poll=lambda: None)
            consumer = CAPTURE.InteractionConsumer(
                directory, "nonce", app, time.monotonic() + 2, None
            )
            original = CAPTURE.atomic_json

            def publish(path, request):
                original(path, request)
                original(
                    directory / "reply-0001.json",
                    dict(
                        version=1,
                        nonce="nonce",
                        seq=1,
                        status="ok",
                        error=None,
                        state=state,
                    ),
                )

            with patch.object(CAPTURE, "atomic_json", side_effect=publish):
                with self.assertRaisesRegex(ValueError, "stale"):
                    consumer.request(state, "capture")
            self.assertFalse((directory / "capture-0001.png").exists())


if __name__ == "__main__":
    unittest.main()
