#!/usr/bin/env python3
"""Capture an explicitly supplied native HyperChad/MoosicBox executable on private X11."""

import ctypes
import math
import stat
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import select
import shutil
import signal
import subprocess
import sys
import time


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--kind", required=True, choices=("hyperchad", "moosicbox"))
    parser.add_argument(
        "--artifacts",
        required=True,
        type=Path,
        help="New directory; logs, isolated state, PNG and metadata are retained",
    )
    parser.add_argument(
        "--required-id",
        action="append",
        default=[],
        help="Required visible production element ID; repeat for each ID",
    )
    parser.add_argument(
        "--variant",
        choices=("alternate",),
        help="Genuine feedback fixture variation, at unchanged native scale",
    )
    parser.add_argument(
        "--scale",
        type=float,
        default=1.0,
        help="Explicit X11 window scale (0.5..3; default 1); useful for native mismatch checks",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=45,
        help="Capture deadline in seconds (1..300; default: 45)",
    )
    parser.add_argument(
        "--screenshot-env",
        default="HYPERCHAD_EGUI_SCREENSHOT",
        help="Renderer opt-in environment variable receiving an absolute PNG path",
    )
    parser.add_argument(
        "--feedback-example",
        action="store_true",
        help="Enable feedback example-specific baseline comparator (no capture aliases)",
    )
    parser.add_argument(
        "--expect-mismatch",
        action="store_true",
        help="Assert native comparator failure plus expected/actual/diff; requires --baseline",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        help="Feedback example only: strict native-frame baseline PNG",
    )
    for weight in ("regular", "bold"):
        parser.add_argument(f"--font-{weight}", type=Path)
        parser.add_argument(f"--font-{weight}-sha256")
    parser.add_argument("--interaction-plan", type=Path)
    parser.add_argument("--x11-library", type=Path)
    parser.add_argument("--xtst-library", type=Path)
    parser.add_argument(
        "args",
        nargs=argparse.REMAINDER,
        help="Arguments after -- are passed verbatim; do not supply real state paths",
    )
    options = parser.parse_args()
    if not 0.5 <= options.scale <= 3:
        parser.error("--scale must be in 0.5..3")
    if not 1 <= options.timeout <= 300:
        parser.error("--timeout must be in 1..300")
    if (
        not options.screenshot_env.isidentifier()
        or not options.screenshot_env.startswith("HYPERCHAD_")
    ):
        parser.error("--screenshot-env must be a HYPERCHAD_ environment variable")
    if options.expect_mismatch and not options.baseline:
        parser.error("--expect-mismatch requires --baseline")
    if options.baseline and not options.feedback_example:
        parser.error("--baseline currently requires --feedback-example")
    if any(not value.strip() or "," in value for value in options.required_id):
        parser.error("--required-id values must be nonempty and contain no commas")
    if options.interaction_plan and (
        not options.x11_library
        or not options.xtst_library
        or options.baseline
        or options.feedback_example
        or options.variant
    ):
        parser.error(
            "interaction requires explicit X11/Xtst libraries and excludes comparator/variant"
        )
    return options


def stop(process):
    """Terminate the whole owned session, even if its original leader exited."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=2)


def run(command, env, timeout=5):
    return subprocess.run(
        command, env=env, check=True, capture_output=True, text=True, timeout=timeout
    ).stdout


def image_info(path):
    # Imported lazily so --help works without the visual-test shell.
    from PIL import Image

    with Image.open(path) as image:
        if image.format != "PNG" or min(image.size) < 32:
            raise ValueError("capture must be a PNG at least 32x32")
        image.load()
        if all(low == high for low, high in image.convert("RGB").getextrema()):
            raise ValueError("capture is a uniform image, not native UI proof")
        return {
            "width": image.width,
            "height": image.height,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }


JSON_LIMIT = 1_048_576


def bounded_json(path):
    """No symlinks/devices, oversized files, NaN, or ambiguous duplicate keys."""

    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise ValueError(f"duplicate JSON key: {key}")
            value[key] = item
        return value

    with os.fdopen(
        os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb"
    ) as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > JSON_LIMIT:
            raise ValueError("protocol JSON must be a bounded regular file")
        data = stream.read(JSON_LIMIT + 1)
    if len(data) > JSON_LIMIT:
        raise ValueError("protocol JSON exceeds 1 MiB")
    return json.loads(
        data,
        object_pairs_hook=pairs,
        parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)),
    )


def atomic_json(path, value):
    data = json.dumps(value, allow_nan=False).encode()
    if len(data) > JSON_LIMIT:
        raise ValueError("protocol JSON exceeds 1 MiB")
    temporary = path.with_suffix(".consumer-tmp")
    with temporary.open("xb") as stream:
        stream.write(data)
    os.replace(temporary, path)


def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def vector(value, length):
    return (
        isinstance(value, list)
        and len(value) == length
        and all(type(v) in (int, float) and math.isfinite(v) for v in value)
    )


def validate_state(state, nonce, pid):
    if (
        not isinstance(state, dict)
        or type(state.get("version")) is not int
        or state.get("version") != 1
        or state.get("nonce") != nonce
    ):
        raise ValueError("interaction version/nonce mismatch")
    if state.get("pid") != pid or not integer(state.get("window_id"), 1):
        raise ValueError("interaction PID/window provenance mismatch")
    if not integer(state.get("revision")) or not integer(state.get("frame")):
        raise ValueError("invalid frame/revision")
    if type(state.get("ready")) is not bool or not isinstance(state.get("route"), str):
        raise ValueError("missing readiness/route")
    if not isinstance(state.get("diagnostics"), list):
        raise ValueError("missing diagnostics")
    if state["diagnostics"]:
        raise ValueError(f"host diagnostics: {state['diagnostics']}")
    pending = state.get("pending")
    if not isinstance(pending, dict) or not all(
        integer(pending.get(k)) for k in ("routes", "resources")
    ):
        raise ValueError("invalid pending counters")
    size = state.get("client_pixels")
    if not vector(size, 2) or any(v <= 0 or v > 16384 for v in size):
        raise ValueError("invalid client dimensions")
    nodes = state.get("nodes")
    if not isinstance(nodes, dict) or len(nodes) > 4096:
        raise ValueError("invalid/bloated semantic tree")
    focused = state.get("focused_key")
    if focused is not None and focused not in nodes:
        raise ValueError("focused key not in current document")
    for key, node in nodes.items():
        if (
            not isinstance(node, dict)
            or not isinstance(key, str)
            or not key
            or len(key) > 256
        ):
            raise ValueError("invalid semantic node")
        if node.get("parent") is not None and node["parent"] not in nodes:
            raise ValueError("missing parent key")
        for field in ("id", "name", "href", "value"):
            if field not in node or (
                node[field] is not None and not isinstance(node[field], str)
            ):
                raise ValueError(f"missing/invalid node {field}")
        if not isinstance(node.get("tag"), str) or not isinstance(
            node.get("text"), str
        ):
            raise ValueError("missing semantic tag/text")
        if (
            type(node.get("visible")) is not bool
            or type(node.get("enabled")) is not bool
        ):
            raise ValueError("invalid visible/enabled flag")
        rect = node.get("rect_pixels")
        if rect is not None:
            if (
                not vector(rect, 4)
                or min(rect) < 0
                or rect[0] + rect[2] > size[0] + 1
                or rect[1] + rect[3] > size[1] + 1
            ):
                raise ValueError("unclipped/invalid client rectangle")
        if node["visible"] and (rect is None or min(rect[2:]) <= 0):
            raise ValueError("visible node has no positive clipped rectangle")
        for field in ("scroll_pixels", "scroll_max_pixels"):
            if not vector(node.get(field), 2) or min(node[field]) < 0:
                raise ValueError(f"invalid {field}")
        if any(
            a > b + 1 for a, b in zip(node["scroll_pixels"], node["scroll_max_pixels"])
        ):
            raise ValueError("scroll exceeds maximum")
        # Parent loops must not permit selector ancestry to hang.
        seen = {key}
        ancestor = node.get("parent")
        while ancestor is not None:
            if ancestor in seen:
                raise ValueError("cyclic semantic tree")
            seen.add(ancestor)
            ancestor = nodes[ancestor].get("parent")
    return state


def selector_valid(selector, depth=0):
    if not isinstance(selector, dict) or not selector or depth > 2:
        return False
    if set(selector) in ({"scroll_ancestor"}, {"button_within"}):
        return selector_valid(next(iter(selector.values())), depth + 1)
    return selector.keys() <= {"id", "tag", "name", "href", "text", "parent"} and all(
        value is None or isinstance(value, str) for value in selector.values()
    )


def ancestors(state, key):
    result = []
    parent = state["nodes"][key]["parent"]
    while parent is not None:
        if parent in result or len(result) >= len(state["nodes"]):
            raise ValueError("cyclic semantic ancestry")
        result.append(parent)
        parent = state["nodes"][parent]["parent"]
    return result


def select_node(state, selector, actionable=False):
    if not selector_valid(selector):
        raise ValueError("invalid semantic/structural selector")
    if "scroll_ancestor" in selector:
        source, _ = select_node(state, selector["scroll_ancestor"])
        matches = [
            (key, state["nodes"][key])
            for key in ancestors(state, source)
            if state["nodes"][key]["scroll_max_pixels"][1] > 0
        ]
    elif "button_within" in selector:
        root, _ = select_node(state, selector["button_within"])
        matches = [
            (key, node)
            for key, node in state["nodes"].items()
            if node["tag"] == "button" and root in ancestors(state, key)
        ]
    else:
        matches = [
            (key, node)
            for key, node in state["nodes"].items()
            if all(node.get(k) == v for k, v in selector.items())
        ]
    if len(matches) != 1:
        raise ValueError(
            f"semantic selector must resolve once, got {len(matches)}: {selector}"
        )
    key, node = matches[0]
    if actionable and (not node["visible"] or not node["enabled"]):
        raise ValueError("input target hidden/disabled")
    return key, node


def settled(state):
    return state["ready"] and not any(state["pending"].values())


def outside_point(state, overlay_selector, content_selector):
    overlay_key, overlay = select_node(state, overlay_selector, True)
    panel_key, panel = select_node(state, {"scroll_ancestor": content_selector}, True)
    if overlay_key not in ancestors(state, panel_key):
        raise ValueError("scroll panel is not inside overlay")
    x, y, w, h = overlay["rect_pixels"]
    px, py, pw, ph = panel["rect_pixels"]
    # Centers of actual free strips, never authored/fixed desktop coordinates.
    regions = [
        (x, y, px - x, h),
        (px + pw, y, x + w - px - pw, h),
        (x, y, w, py - y),
        (x, py + ph, w, y + h - py - ph),
    ]
    for rx, ry, rw, rh in sorted(regions, key=lambda r: r[2] * r[3], reverse=True):
        point = (math.floor(rx + rw / 2), math.floor(ry + rh / 2))
        if (
            rw >= 2
            and rh >= 2
            and x <= point[0] < x + w
            and y <= point[1] < y + h
            and not (px <= point[0] < px + pw and py <= point[1] < py + ph)
        ):
            return point
    raise ValueError("no native outside-overlay point available")


def same_identity(before, after, selector):
    a, node_a = select_node(before, selector)
    b, node_b = select_node(after, selector)
    if (
        not integer(node_a.get("generation"))
        or not integer(node_b.get("generation"))
        or a != b
        or node_a["generation"] != node_b["generation"]
    ):
        raise ValueError("comparison crosses document/control generation")
    return node_a, node_b


def sticky_check(before, after, header, content):
    if (
        before["window_id"] != after["window_id"]
        or before["client_pixels"] != after["client_pixels"]
        or after["frame"] <= before["frame"]
    ):
        raise ValueError("sticky comparison has stale/native provenance")
    head_a, head_b = same_identity(before, after, header)
    row_a, row_b = same_identity(before, after, content)
    pane_a, pane_b = same_identity(before, after, {"scroll_ancestor": content})
    if (
        not head_a["visible"]
        or not head_b["visible"]
        or any(
            abs(a - b) > 1 for a, b in zip(head_a["rect_pixels"], head_b["rect_pixels"])
        )
    ):
        raise ValueError("sticky header moved or became hidden")
    delta = pane_b["scroll_pixels"][1] - pane_a["scroll_pixels"][1]
    if (
        min(pane_a["scroll_max_pixels"][1], pane_b["scroll_max_pixels"][1]) <= 0
        or delta <= 1
    ):
        raise ValueError("no-overflow/no movement: sticky proof rejected")
    if not row_a["visible"] or not (
        not row_b["visible"] or row_b["rect_pixels"][1] < row_a["rect_pixels"][1] - 1
    ):
        raise ValueError("content did not move/clip while scrolling")
    return dict(scroll_delta_pixels=delta, header_stationary=True, content_moved=True)


def canvas_pixels(directory, seq, state, selector):
    from PIL import Image

    _, node = select_node(state, selector, True)
    if node["tag"] != "canvas":
        raise ValueError("pixel comparison target must be canvas")
    x, y, w, h = node["rect_pixels"]
    # Inward rounding excludes neighboring focus/hover/control pixels.
    box = (math.ceil(x), math.ceil(y), math.floor(x + w), math.floor(y + h))
    if box[2] <= box[0] or box[3] <= box[1]:
        raise ValueError("empty canvas pixel crop")
    with Image.open(directory / f"capture-{seq:04d}.png") as image:
        if (
            list(image.size) != state["client_pixels"]
            or box[0] < 0
            or box[1] < 0
            or box[2] > image.width
            or box[3] > image.height
        ):
            raise ValueError("canvas crop exceeds exact native capture")
        pixels = image.convert("RGBA").crop(box)
        return box, pixels.size, pixels.tobytes()


def compare_canvas(directory, seq_a, before, seq_b, after, selector, equal):
    same_identity(before, after, selector)
    box_a, size_a, a = canvas_pixels(directory, seq_a, before, selector)
    box_b, size_b, b = canvas_pixels(directory, seq_b, after, selector)
    if (
        before["window_id"] != after["window_id"]
        or box_a != box_b
        or size_a != size_b
        or after["frame"] <= before["frame"]
    ):
        raise ValueError("canvas comparison geometry/provenance changed")
    if (a == b) != equal:
        raise ValueError("canvas exact RGBA equality assertion failed")
    return dict(
        equal=equal,
        crop=list(box_a),
        rgba_sha256_a=hashlib.sha256(a).hexdigest(),
        rgba_sha256_b=hashlib.sha256(b).hexdigest(),
    )


def scale_check(state, logical_client, requested_scale):
    ratios = [p / l for p, l in zip(state["client_pixels"], logical_client)]
    if any(
        abs(p - requested_scale * l) > 1
        for p, l in zip(state["client_pixels"], logical_client)
    ):
        raise ValueError(
            "effective client scale disagrees with fixed logical fixture size"
        )
    return dict(
        effective_scale_xy=ratios,
        requested_scale=requested_scale,
        client_pixels=state["client_pixels"],
        logical_client=logical_client,
    )


def verified_fonts(options):
    fonts = {}
    for weight in ("regular", "bold"):
        path, digest = getattr(options, f"font_{weight}"), getattr(
            options, f"font_{weight}_sha256"
        )
        if path is None and digest is None:
            continue
        if (
            path is None
            or digest is None
            or not path.is_absolute()
            or len(digest) != 64
            or any(c not in "0123456789abcdef" for c in digest)
        ):
            raise ValueError("font requires absolute path and lowercase SHA-256")
        path = path.resolve(strict=True)
        if not path.is_file() or not 0 < path.stat().st_size <= 32 * 1024 * 1024:
            raise ValueError("font must be bounded regular local file")
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f"{weight} font hash mismatch")
        fonts[weight] = dict(path=str(path), sha256=digest)
    if fonts and (set(fonts) != {"regular", "bold"} or not options.interaction_plan):
        raise ValueError("fixture fonts require both weights and interaction mode")
    return fonts


def open_owned_native(display, pid, x11_library, xtst_library, authority):
    """Use the fresh child authority for XOpenDisplay, restoring caller state."""
    previous_authority = os.environ.get("XAUTHORITY")
    os.environ["XAUTHORITY"] = authority
    try:
        return NativeXTest(display, pid, x11_library, xtst_library)
    finally:
        if previous_authority is None:
            os.environ.pop("XAUTHORITY", None)
        else:
            os.environ["XAUTHORITY"] = previous_authority


class NativeXTest:
    """Explicit native libraries and private display only; no PATH/tool fallback."""

    def __init__(self, display, pid, x11_path, xtst_path):
        if (
            not display.startswith(":")
            or not display[1:].isdigit()
            or int(display[1:]) < 1000
        ):
            raise ValueError("XTEST requires launcher's private high display")
        for path in (x11_path, xtst_path):
            if not path.is_absolute() or not path.is_file():
                raise ValueError("native library paths must be explicit absolute files")
        c = ctypes
        self.x = c.CDLL(str(x11_path))
        self.t = c.CDLL(str(xtst_path))
        self.pid = pid
        self.window = None
        self.display = None
        D, U, I, P = c.c_void_p, c.c_ulong, c.c_int, c.POINTER
        signatures = {
            "XOpenDisplay": ([c.c_char_p], D),
            "XCloseDisplay": ([D], I),
            "XFlush": ([D], I),
            "XSync": ([D, I], I),
            "XInternAtom": ([D, c.c_char_p, I], U),
            "XFree": ([D], I),
            "XGetGeometry": (
                [
                    D,
                    U,
                    P(U),
                    P(I),
                    P(I),
                    P(c.c_uint),
                    P(c.c_uint),
                    P(c.c_uint),
                    P(c.c_uint),
                ],
                I,
            ),
            "XTranslateCoordinates": ([D, U, U, I, I, P(I), P(I), P(U)], I),
            "XGetWindowProperty": (
                [
                    D,
                    U,
                    U,
                    c.c_long,
                    c.c_long,
                    I,
                    U,
                    P(U),
                    P(I),
                    P(U),
                    P(U),
                    P(c.POINTER(c.c_ubyte)),
                ],
                I,
            ),
            "XSetInputFocus": ([D, U, I, U], I),
            "XGetInputFocus": ([D, P(U), P(I)], I),
            "XStringToKeysym": ([c.c_char_p], U),
            "XKeysymToKeycode": ([D, U], c.c_ubyte),
            "XkbKeycodeToKeysym": ([D, c.c_ubyte, I, I], U),
        }
        for name, (args, result) in signatures.items():
            fn = getattr(self.x, name)
            fn.argtypes = args
            fn.restype = result
        for name, args in {
            "XTestQueryExtension": [D, P(I), P(I), P(I), P(I)],
            "XTestFakeMotionEvent": [D, I, I, I, U],
            "XTestFakeButtonEvent": [D, c.c_uint, I, U],
            "XTestFakeKeyEvent": [D, c.c_uint, I, U],
        }.items():
            fn = getattr(self.t, name)
            fn.argtypes = args
            fn.restype = I
        self.error_code = 0
        # Keep callback alive until the display is closed; stale windows must fail
        # as Python errors, not terminate the process in Xlib's default handler.
        self.error_callback = c.CFUNCTYPE(I, D, D)(self._xerror)
        self.x.XSetErrorHandler.argtypes = [c.c_void_p]
        self.x.XSetErrorHandler.restype = c.c_void_p
        self.previous_handler = self.x.XSetErrorHandler(c.cast(self.error_callback, D))
        self.display = self.x.XOpenDisplay(display.encode())
        if not self.display:
            self.close()
            raise ValueError("cannot open authenticated private display")
        values = [I() for _ in range(4)]
        if not self.t.XTestQueryExtension(self.display, *[c.byref(v) for v in values]):
            self.close()
            raise ValueError("private X server lacks XTEST")

    def _xerror(self, _display, _event):
        self.error_code = 1
        return 0

    def checked_sync(self):
        self.x.XSync(self.display, 0)
        if self.error_code:
            raise ValueError("native X11 window/input error")

    def close(self):
        if self.display:
            self.x.XCloseDisplay(self.display)
            self.display = None
        self.x.XSetErrorHandler(self.previous_handler)

    def verify(self, state):
        c = ctypes
        U, I = c.c_ulong, c.c_int
        window = state["window_id"]
        if self.window is not None and window != self.window:
            raise ValueError("native window identity changed")
        atom = self.x.XInternAtom(self.display, b"_NET_WM_PID", 1)
        typ, count, after, fmt = U(), U(), U(), I()
        data = c.POINTER(c.c_ubyte)()
        result = self.x.XGetWindowProperty(
            self.display,
            window,
            atom,
            0,
            1,
            0,
            6,
            c.byref(typ),
            c.byref(fmt),
            c.byref(count),
            c.byref(after),
            c.byref(data),
        )
        try:
            if (
                result
                or typ.value != 6
                or fmt.value != 32
                or count.value != 1
                or not data
            ):
                raise ValueError("native window has no verifiable PID property")
            if c.cast(data, c.POINTER(U))[0] != self.pid:
                raise ValueError("native window not owned by launched app")
        finally:
            if data:
                self.x.XFree(data)
        root, x, y = U(), I(), I()
        w, h, border, depth = [c.c_uint() for _ in range(4)]
        if not self.x.XGetGeometry(
            self.display,
            window,
            c.byref(root),
            c.byref(x),
            c.byref(y),
            c.byref(w),
            c.byref(h),
            c.byref(border),
            c.byref(depth),
        ):
            raise ValueError("cannot query native client geometry")
        if [w.value, h.value] != state["client_pixels"]:
            raise ValueError("host/native client dimensions disagree")
        self.window = window
        rx, ry, child = I(), I(), U()
        if not self.x.XTranslateCoordinates(
            self.display, window, root, 0, 0, c.byref(rx), c.byref(ry), c.byref(child)
        ):
            raise ValueError("cannot translate client coordinates")
        self.origin = (rx.value, ry.value)
        self.x.XSetInputFocus(self.display, window, 1, 0)
        focus, revert = U(), I()
        self.x.XGetInputFocus(self.display, c.byref(focus), c.byref(revert))
        if focus.value != window:
            raise ValueError("private window could not receive focus")
        self.checked_sync()

    def key(self, name, pressed):
        symbol = self.x.XStringToKeysym(name.encode())
        code = self.x.XKeysymToKeycode(self.display, symbol)
        if not symbol or not code:
            raise ValueError(f"key unavailable in private X keymap: {name}")
        if not self.t.XTestFakeKeyEvent(self.display, code, int(pressed), 0):
            raise ValueError("XTEST key failed")

    def input(self, state, phase):
        self.verify(state)
        op = phase["op"]
        if op == "outside":
            x, y = outside_point(state, phase["selector"], phase["content"])
            if not self.t.XTestFakeMotionEvent(
                self.display, -1, self.origin[0] + x, self.origin[1] + y, 0
            ):
                raise ValueError("XTEST outside motion failed")
            for pressed in (1, 0):
                if not self.t.XTestFakeButtonEvent(self.display, 1, pressed, 0):
                    raise ValueError("XTEST outside click failed")
        elif op in ("click", "wheel"):
            _, node = select_node(state, phase["selector"], True)
            if op == "click" and node["text"].strip().lower() in (
                "save",
                "new",
                "connect",
                "select",
                "play",
            ):
                raise ValueError("unsafe action forbidden in disconnected acceptance")
            if op == "wheel" and node["scroll_max_pixels"][1] <= 0:
                raise ValueError("no-overflow: cannot validate scrolling")
            if op == "click" and node["href"] not in (None, "/", "/settings"):
                raise ValueError(
                    "navigation outside disconnected Home/Settings forbidden"
                )
            x, y, w, h = node["rect_pixels"]
            if not self.t.XTestFakeMotionEvent(
                self.display,
                -1,
                int(self.origin[0] + x + w / 2),
                int(self.origin[1] + y + h / 2),
                0,
            ):
                raise ValueError("XTEST motion failed")
            if op == "wheel" and node["scroll_max_pixels"][1] <= 0:
                raise ValueError("no-overflow: cannot validate scrolling")
            button = 1 if op == "click" else (5 if phase["ticks"] > 0 else 4)
            for _ in range(1 if op == "click" else abs(phase["ticks"])):
                for pressed in (1, 0):
                    if not self.t.XTestFakeButtonEvent(
                        self.display, button, pressed, 0
                    ):
                        raise ValueError("XTEST button failed")
        elif op == "key":
            names = {
                "Tab": ["Tab"],
                "Shift+Tab": ["Shift_L", "Tab"],
                "Ctrl+A": ["Control_L", "a"],
                "BackSpace": ["BackSpace"],
                "Escape": ["Escape"],
                "Left": ["Left"],
                "Right": ["Right"],
            }[phase["key"]]
            try:
                for key in names:
                    self.key(key, True)
            finally:
                for key in reversed(names):
                    self.key(key, False)
        elif op == "text":
            key = state["focused_key"]
            if key is None:
                raise ValueError("text input has no semantic focus")
            node = state["nodes"][key]
            if node["tag"] not in ("input", "textarea") or node["value"] is None:
                raise ValueError("focused target is not an observable text field")
            for char in phase["text"]:
                symbol = ord(char)
                code = self.x.XKeysymToKeycode(self.display, symbol)
                if not code:
                    raise ValueError("text character not in private keymap")
                shift = self.x.XkbKeycodeToKeysym(self.display, code, 0, 0) != symbol
                if (
                    shift
                    and self.x.XkbKeycodeToKeysym(self.display, code, 0, 1) != symbol
                ):
                    raise ValueError("text character requires unsupported keymap level")
                try:
                    if shift:
                        self.key("Shift_L", True)
                    if not self.t.XTestFakeKeyEvent(self.display, code, 1, 0):
                        raise ValueError("XTEST text press failed")
                    if not self.t.XTestFakeKeyEvent(self.display, code, 0, 0):
                        raise ValueError("XTEST text release failed")
                finally:
                    if shift:
                        self.key("Shift_L", False)
        self.x.XFlush(self.display)
        self.checked_sync()


def validate_plan(plan):
    if (
        not isinstance(plan, dict)
        or set(plan) != {"version", "phases"}
        or plan["version"] != 1
    ):
        raise ValueError("invalid interaction plan envelope")
    if not isinstance(plan["phases"], list) or not 1 <= len(plan["phases"]) <= 32:
        raise ValueError("plan must contain 1..32 phases")
    allowed = {
        "capture": set(),
        "assert": {"expect"},
        "click": {"selector"},
        "wheel": {"selector", "ticks"},
        "key": {"key"},
        "text": {"text"},
        "outside": {"selector", "content"},
        "sticky": {"before", "after", "header", "content"},
        "pixels": {"before", "after", "selector", "equal"},
        "scale": {"logical_client"},
    }
    for phase_index, phase in enumerate(plan["phases"]):
        if not isinstance(phase, dict) or phase.get("op") not in allowed:
            raise ValueError("unsupported interaction operation")
        required_fields = {"op"} | allowed[phase["op"]]
        optional_fields = {"settle_frames"} if phase["op"] == "wheel" else set()
        if (
            not required_fields <= set(phase)
            or not set(phase) <= required_fields | optional_fields
        ):
            raise ValueError("invalid operation fields")
        if "settle_frames" in phase and (
            type(phase["settle_frames"]) is not int or phase["settle_frames"] != 2
        ):
            raise ValueError("wheel requires exactly two settled effect frames")
        if "selector" in phase and not selector_valid(phase["selector"]):
            raise ValueError("invalid semantic selector")
        for field in ("header", "content"):
            if field in phase and not selector_valid(phase[field]):
                raise ValueError("invalid structural comparison selector")
        if phase["op"] in ("sticky", "pixels"):
            if (
                not integer(phase["before"], 1)
                or not integer(phase["after"], 1)
                or not phase["before"]
                < phase["after"]
                <= sum(p.get("op") == "capture" for p in plan["phases"][:phase_index])
            ):
                raise ValueError("comparison requires two prior ordered captures")
        if phase["op"] == "pixels" and type(phase["equal"]) is not bool:
            raise ValueError("pixel equality must be boolean")
        if phase["op"] == "scale" and (
            not vector(phase["logical_client"], 2) or min(phase["logical_client"]) <= 0
        ):
            raise ValueError("scale requires fixed positive logical client size")
        if phase["op"] == "wheel" and (
            not integer(abs(phase["ticks"]), 1) or abs(phase["ticks"]) > 20
        ):
            raise ValueError("wheel ticks must be signed nonzero integer <=20")
        if phase["op"] == "key" and phase["key"] not in (
            "Tab",
            "Shift+Tab",
            "Ctrl+A",
            "BackSpace",
            "Escape",
            "Left",
            "Right",
        ):
            raise ValueError("unsupported/unsafe key")
        if phase["op"] == "text" and (
            not isinstance(phase["text"], str)
            or not 1 <= len(phase["text"]) <= 256
            or any(not 32 <= ord(c) <= 126 for c in phase["text"])
        ):
            raise ValueError("text must be 1..256 printable ASCII characters")
        if phase["op"] == "assert":
            expect = phase["expect"]
            if (
                not isinstance(expect, dict)
                or not expect
                or not expect.keys()
                <= {
                    "route",
                    "selector",
                    "visible",
                    "focused",
                    "value",
                    "scroll_y_min",
                    "scroll_max_y_min",
                }
            ):
                raise ValueError("unsupported assertion")
            if set(expect) != {"route"} and "selector" not in expect:
                raise ValueError("node assertions require a selector")
            for key in ("visible", "focused"):
                if key in expect and type(expect[key]) is not bool:
                    raise ValueError("assertion booleans must be explicit")
            for key in ("route", "value"):
                if key in expect and not isinstance(expect[key], str):
                    raise ValueError("assertion route/value must be text")
            for key in ("scroll_y_min", "scroll_max_y_min"):
                if key in expect and (
                    type(expect[key]) not in (int, float)
                    or not math.isfinite(expect[key])
                    or expect[key] < 0
                ):
                    raise ValueError("scroll assertion must be finite nonnegative")
            if "selector" in expect:
                selector = expect["selector"]
                if not selector_valid(selector):
                    raise ValueError("invalid assertion selector")
    return plan


class InteractionConsumer:
    def __init__(self, directory, nonce, app, deadline, native, requested_scale=1.0):
        self.captures = {}
        self.requested_scale = requested_scale
        self.directory, self.nonce, self.app = directory, nonce, app
        self.deadline, self.native = deadline, native
        self.frame, self.revision, self.window, self.seq = -1, -1, None, 0

    def state(self):
        value = validate_state(
            bounded_json(self.directory / "state.json"), self.nonce, self.app.pid
        )
        if value["frame"] < self.frame or value["revision"] < self.revision:
            raise ValueError("host state regressed")
        if self.window is not None and value["window_id"] != self.window:
            raise ValueError("host window changed")
        self.frame, self.revision, self.window = (
            value["frame"],
            value["revision"],
            value["window_id"],
        )
        return value

    def wait(self, predicate, settled_only=True):
        until = min(self.deadline, time.monotonic() + 10)
        while time.monotonic() < until:
            if self.app.poll() is not None:
                raise RuntimeError(f"interaction app exited: {self.app.returncode}")
            try:
                value = self.state()
                if (not settled_only or settled(value)) and predicate(value):
                    return value
            except FileNotFoundError:
                pass
            time.sleep(0.02)
        raise TimeoutError("interaction phase deadline exceeded")

    def wait_wheel(self, before, phase):
        """Require the native effect and two distinct settled, stable frames."""
        previous = None
        observations = 0
        _, source = select_node(before, phase["selector"], True)
        direction = 1 if phase["ticks"] > 0 else -1

        def committed(value):
            nonlocal previous, observations
            _, target = same_identity(before, value, phase["selector"])
            if not settled(value):
                previous, observations = None, 0
                return False
            if (
                value["frame"] <= before["frame"]
                or direction * (target["scroll_pixels"][1] - source["scroll_pixels"][1])
                <= 1
            ):
                previous, observations = None, 0
                return False
            signature = (
                value["revision"],
                value["window_id"],
                value["client_pixels"],
                value["nodes"],
            )
            if previous is not None and previous[0] == signature:
                if value["frame"] > previous[1]:
                    observations += 1
            else:
                observations = 1
            previous = (signature, value["frame"])
            return observations >= phase.get("settle_frames", 2)

        return self.wait(committed, settled_only=False)

    def request(self, state, op):
        self.seq += 1
        request = dict(
            version=1,
            nonce=self.nonce,
            seq=self.seq,
            op=op,
            after_frame=state["frame"],
            expected_revision=state["revision"],
        )
        png = self.directory / f"capture-{self.seq:04d}.png"
        reply_path = self.directory / f"reply-{self.seq:04d}.json"
        if png.exists() or reply_path.exists():
            raise ValueError("stale reply/capture exists")
        atomic_json(self.directory / "request.json", request)
        until = min(self.deadline, time.monotonic() + 10)
        while time.monotonic() < until:
            if reply_path.exists():
                reply = bounded_json(reply_path)
                if not isinstance(reply, dict):
                    raise ValueError("reply must be a JSON object")
                if any(reply.get(k) != request[k] for k in ("version", "nonce", "seq")):
                    raise ValueError("reply provenance mismatch")
                if reply.get("status") != "ok":
                    raise ValueError(f"host rejected request: {reply.get('error')}")
                captured = validate_state(reply.get("state"), self.nonce, self.app.pid)
                if (
                    captured["revision"] != state["revision"]
                    or captured["frame"] <= state["frame"]
                    or captured["window_id"] != state["window_id"]
                    or not settled(captured)
                ):
                    raise ValueError("capture/finish reply is stale or unsettled")
                if op == "capture":
                    if (
                        png.is_symlink()
                        or not png.is_file()
                        or png.stat().st_size > 64 * 1024 * 1024
                    ):
                        raise ValueError("invalid native capture file")
                    info = image_info(png)
                    if [info["width"], info["height"]] != captured["client_pixels"]:
                        raise ValueError("capture/native dimensions mismatch")
                return reply
            if self.app.poll() is not None:
                raise RuntimeError("app exited before reply")
            time.sleep(0.02)
        raise TimeoutError("interaction reply deadline exceeded")

    def execute(self, plan):
        state = self.wait(lambda value: True)
        evidence = []
        for phase in plan["phases"]:
            if time.monotonic() >= self.deadline:
                raise TimeoutError("interaction deadline exceeded")
            state = self.wait(lambda value: True)
            self.native.verify(state)
            before = state
            comparison = None
            if phase["op"] in ("sticky", "pixels"):
                a, b = phase["before"], phase["after"]
                if a not in self.captures or b not in self.captures:
                    raise ValueError("missing exact capture state")
                if phase["op"] == "sticky":
                    comparison = sticky_check(
                        self.captures[a],
                        self.captures[b],
                        phase["header"],
                        phase["content"],
                    )
                else:
                    comparison = compare_canvas(
                        self.directory,
                        a,
                        self.captures[a],
                        b,
                        self.captures[b],
                        phase["selector"],
                        phase["equal"],
                    )
            elif phase["op"] == "scale":
                comparison = scale_check(
                    state, phase["logical_client"], self.requested_scale
                )
            elif phase["op"] == "capture":
                result = self.request(state, "capture")
                state = result["state"]
                self.captures[self.seq] = state
            elif phase["op"] == "assert":
                expect = phase["expect"]

                def matches(value):
                    if "route" in expect and value["route"] != expect["route"]:
                        return False
                    if "selector" not in expect:
                        return True
                    key, node = select_node(value, expect["selector"])
                    return (
                        all(
                            (node[k] == expect[k])
                            for k in ("visible", "value")
                            if k in expect
                        )
                        and (
                            "focused" not in expect
                            or (value["focused_key"] == key) == expect["focused"]
                        )
                        and (
                            "scroll_y_min" not in expect
                            or node["scroll_pixels"][1] >= expect["scroll_y_min"]
                        )
                        and (
                            "scroll_max_y_min" not in expect
                            or node["scroll_max_pixels"][1]
                            >= expect["scroll_max_y_min"]
                        )
                    )

                state = self.wait(matches)
            else:
                self.native.input(state, phase)
                if phase["op"] == "wheel":
                    state = self.wait_wheel(before, phase)
                else:
                    state = self.wait(lambda value: value["frame"] > before["frame"])
            evidence.append(dict(phase=phase, before=before, after=state))
            if comparison is not None:
                evidence[-1]["comparison"] = comparison
            # Bounded incremental evidence; no secret fields should be emitted by host.
            atomic_json(
                self.directory / f"phase-{len(evidence):04d}.json", evidence[-1]
            )
        self.request(state, "finish")
        self.app.wait(timeout=max(0.01, min(5, self.deadline - time.monotonic())))
        if self.app.returncode != 0:
            raise RuntimeError("interaction app failed on finish")
        return dict(status="interacted", phases=len(evidence), captures=self.seq - 1)


def main():
    options = arguments()
    fonts = verified_fonts(options)
    plan = (
        validate_plan(bounded_json(options.interaction_plan))
        if options.interaction_plan
        else None
    )
    executable = options.executable.absolute()
    executable.resolve(
        strict=True
    )  # Validate without changing argv[0] for multicall binaries.
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError("--executable must name an executable file")
    for tool in ("Xvfb", "xauth", "xdpyinfo", "glxinfo", "fc-list"):
        if not shutil.which(tool):
            raise ValueError(f"missing {tool}; enter nix develop .#egui-visual-test")
    fontconfig = Path(os.environ.get("FONTCONFIG_FILE", ""))
    if not fontconfig.is_file() or not str(fontconfig).startswith("/nix/store/"):
        raise ValueError(
            "FONTCONFIG_FILE must be the pinned visual-test shell's Nix font config"
        )
    artifacts = options.artifacts.resolve()
    baseline_hash = None
    if options.baseline:
        options.baseline = options.baseline.resolve(strict=True)
        if not options.baseline.is_file() or options.baseline.is_relative_to(artifacts):
            raise ValueError(
                "baseline must be a regular PNG outside the new artifact directory"
            )
        image_info(options.baseline)
        baseline_hash = hashlib.sha256(options.baseline.read_bytes()).hexdigest()
    artifacts.mkdir(mode=0o700)  # Refuse reused directories/stale screenshot success.
    state = artifacts / "state"
    state.mkdir(mode=0o700)
    env = {
        key: os.environ[key]
        for key in (
            "PATH",
            "LD_LIBRARY_PATH",
            "LIBGL_DRIVERS_PATH",
            "FONTCONFIG_FILE",
            "VK_ICD_FILENAMES",
        )
        if key in os.environ
    }
    for weight, font in fonts.items():
        env[f"HYPERCHAD_ACCEPTANCE_FONT_{weight.upper()}"] = font["path"]
    env.update(
        HOME=str(state),
        LANG="C.UTF-8",
        LC_ALL="C.UTF-8",
        TZ="UTC",
        LIBGL_ALWAYS_SOFTWARE="1",
        GALLIUM_DRIVER="llvmpipe",
        WINIT_UNIX_BACKEND="x11",
        WINIT_X11_SCALE_FACTOR=str(options.scale),
        GDK_BACKEND="x11",
        QT_QPA_PLATFORM="xcb",
        WGPU_BACKEND="gl",
        RUST_LOG="info",
    )
    for name, directory in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "local-state"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "runtime"),
        ("TMPDIR", "tmp"),
    ):
        path = state / directory
        path.mkdir(mode=0o700)
        env[name] = str(path)
    config_root = state / "moosicbox"
    config_root.mkdir(mode=0o700)
    env["MOOSICBOX_CONFIG_DIR"] = str(config_root)
    env["MOOSICBOX_STATE_DIR"] = env["XDG_STATE_HOME"]
    log_root = state / "logs"
    log_root.mkdir(mode=0o700)
    env["MOOSICBOX_LOG_DIR"] = str(log_root)
    # No inherited DISPLAY, WAYLAND_DISPLAY, D-Bus, audio, credentials or app config.
    # Explicit high display + X server lock prevents attachment to an existing server.
    display = f":{secrets.randbelow(50000) + 1000}"
    env["DISPLAY"] = display
    env["XAUTHORITY"] = str(state / "Xauthority")
    run(
        [
            "xauth",
            "-f",
            env["XAUTHORITY"],
            "add",
            display,
            "MIT-MAGIC-COOKIE-1",
            secrets.token_hex(16),
        ],
        env,
    )
    png = artifacts / "screenshot.png"
    env[options.screenshot_env] = str(png)
    env["HYPERCHAD_EGUI_SCREENSHOT_EXIT"] = "1"
    env["HYPERCHAD_EGUI_REQUIRED_IDS"] = ",".join(options.required_id)
    env["HYPERCHAD_EGUI_READY"] = str(artifacts / "ready")
    if options.variant:
        env["HYPERCHAD_FEEDBACK_VARIANT"] = options.variant
    if options.feedback_example:
        # Comparator settings are example-specific; capture always uses the
        # agreed production names, never a legacy OUTPUT/READY alias.
        if options.baseline:
            env["HYPERCHAD_FEEDBACK_BASELINE"] = str(
                options.baseline.resolve(strict=True)
            )
            env["HYPERCHAD_FEEDBACK_ARTIFACTS"] = str(artifacts / "comparison")
    interaction_dir = None
    nonce = None
    if plan is not None:
        interaction_dir = artifacts / "interaction"
        interaction_dir.mkdir(mode=0o700)
        nonce = secrets.token_hex(32)
        for key in (
            options.screenshot_env,
            "HYPERCHAD_EGUI_SCREENSHOT_EXIT",
            "HYPERCHAD_EGUI_READY",
        ):
            env.pop(key, None)
        env["HYPERCHAD_EGUI_INTERACTION_DIR"] = str(interaction_dir)
        env["HYPERCHAD_EGUI_INTERACTION_NONCE"] = nonce
    with executable.open("rb") as executable_file:
        executable_hash = hashlib.file_digest(executable_file, "sha256").hexdigest()
    metadata = {
        "status": "starting",
        "kind": options.kind,
        "executable": str(executable),
        "executable_sha256": executable_hash,
        "display": display,
        "protocol": "x11",
        "software_graphics": True,
        "window_scale": options.scale,
        "fixture_fonts": fonts,
        "fontconfig": str(fontconfig),
        "capture_env": options.screenshot_env,
        "state_root": str(state),
        "argv": options.args,
        "feedback_example_comparator": options.feedback_example,
        "required_ids": options.required_id,
        "variant": options.variant,
        "baseline_sha256_before": baseline_hash,
        "baseline": str(options.baseline.resolve()) if options.baseline else None,
        "note": "Kind is caller-declared; inspect PNG and app log for product acceptance.",
    }
    read_fd = None
    processes = []
    logs = []
    start = time.monotonic()
    deadline = start + options.timeout
    try:
        read_fd, write_fd = os.pipe()
        try:
            xvfb_log = (artifacts / "xvfb.log").open("wb")
            logs.append(xvfb_log)
            xvfb = subprocess.Popen(
                [
                    "Xvfb",
                    display,
                    "-displayfd",
                    str(write_fd),
                    "-screen",
                    "0",
                    "1600x1200x24",
                    "-dpi",
                    "96",
                    "-nolisten",
                    "tcp",
                    "-auth",
                    env["XAUTHORITY"],
                    "-noreset",
                ],
                env=env,
                stdout=xvfb_log,
                stderr=subprocess.STDOUT,
                pass_fds=(write_fd,),
                start_new_session=True,
            )
            processes.append(xvfb)
            os.close(write_fd)
            write_fd = None
            if not select.select([read_fd], [], [], min(10, options.timeout))[0]:
                raise TimeoutError("Xvfb did not report readiness")
            reported = os.read(read_fd, 64).decode().strip()
            if reported != display[1:] or xvfb.poll() is not None:
                raise RuntimeError("private Xvfb failed; refusing any existing display")
        finally:
            # Xvfb may publish readiness again during startup/reset. Keep its
            # pipe reader alive until server teardown, avoiding an EPIPE crash.
            if write_fd is not None:
                os.close(write_fd)
        (artifacts / "x11.txt").write_text(run(["xdpyinfo"], env))
        graphics = run(["glxinfo", "-B"], env)
        (artifacts / "graphics.txt").write_text(graphics)
        if "llvmpipe" not in graphics.lower():
            raise RuntimeError(
                "Mesa llvmpipe not confirmed; refusing hardware/unknown renderer"
            )
        (artifacts / "fonts.txt").write_text(run(["fc-list"], env))
        if time.monotonic() >= deadline:
            raise TimeoutError("display setup exhausted capture deadline")
        app_log = (artifacts / "app.log").open("wb")
        logs.append(app_log)
        app_args = options.args[1:] if options.args[:1] == ["--"] else options.args
        app = subprocess.Popen(
            [str(executable), *app_args],
            cwd=state,
            env=env,
            stdout=app_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        processes.append(app)
        metadata["app_pid"] = app.pid
        if plan is not None:
            native = open_owned_native(
                display,
                app.pid,
                options.x11_library,
                options.xtst_library,
                env["XAUTHORITY"],
            )
            try:
                metadata.update(
                    InteractionConsumer(
                        interaction_dir, nonce, app, deadline, native, options.scale
                    ).execute(plan)
                )
            finally:
                native.close()
            print(f"Interaction evidence retained in {interaction_dir}")
            return
        last_image_error = "no screenshot written"
        while time.monotonic() < deadline:
            if png.is_file():
                try:
                    metadata["image"] = image_info(png)
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError(
                            "no time left for requested clean screenshot exit"
                        )
                    metadata["app_returncode_at_capture"] = app.wait(timeout=remaining)
                    if (
                        options.baseline
                        and hashlib.sha256(options.baseline.read_bytes()).hexdigest()
                        != baseline_hash
                    ):
                        raise RuntimeError("baseline changed during native comparison")
                    if options.expect_mismatch:
                        comparison = artifacts / "comparison"
                        for name in ("expected.png", "actual.png", "diff.png"):
                            image_info(comparison / name)
                        app_output = (artifacts / "app.log").read_text(errors="replace")
                        if (
                            app.returncode == 0
                            or "Mismatch { pixels:" not in app_output
                        ):
                            raise RuntimeError(
                                "expected native mismatch was not reported by comparator"
                            )
                        from PIL import Image, ImageChops

                        with Image.open(options.baseline) as baseline_image, Image.open(
                            comparison / "expected.png"
                        ) as expected_image:
                            if (
                                baseline_image.size != expected_image.size
                                or ImageChops.difference(
                                    baseline_image.convert("RGB"),
                                    expected_image.convert("RGB"),
                                ).getbbox()
                            ):
                                raise RuntimeError(
                                    "comparator expected image does not match baseline"
                                )
                        metadata["expected_mismatch"] = True
                    elif app.returncode != 0:
                        raise RuntimeError(f"application failed with {app.returncode}")
                    if not (artifacts / "ready").is_file():
                        raise RuntimeError(
                            "capture completed without geometry/assets readiness marker"
                        )
                    if png.with_suffix(".error.txt").exists():
                        raise RuntimeError("renderer reported a capture error artifact")
                    sidecar = png.with_suffix(".json")
                    if sidecar.is_file():
                        metadata["renderer_metadata"] = json.loads(sidecar.read_text())
                    metadata["status"] = "captured"
                    break
                except (OSError, ValueError) as error:
                    last_image_error = str(error)
            if app.poll() is not None:
                raise RuntimeError(
                    f"application exited ({app.returncode}): {last_image_error}"
                )
            if xvfb.poll() is not None:
                raise RuntimeError("private Xvfb exited during capture")
            time.sleep(0.1)
        else:
            raise TimeoutError(
                f"native screenshot deadline exceeded: {last_image_error}"
            )
    except BaseException as error:
        metadata.update(status="failed", error=str(error))
        raise
    finally:
        for process in reversed(processes):
            stop(process)
        if read_fd is not None:
            os.close(read_fd)
        for log in logs:
            log.close()
        metadata["ready_marker_exists"] = (artifacts / "ready").is_file()
        error_path = png.with_suffix(".error.txt")
        if error_path.is_file():
            metadata["renderer_error"] = error_path.read_text(errors="replace")
        if options.baseline:
            metadata["baseline_sha256_after"] = hashlib.sha256(
                options.baseline.read_bytes()
            ).hexdigest()
        metadata["elapsed_seconds"] = round(time.monotonic() - start, 3)
        metadata["cleanup"] = "owned process groups terminated; isolated state retained"
        (artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Captured {png}; inspect metadata.json, graphics.txt and app.log")


if __name__ == "__main__":

    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"received signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(f"native-screenshot: {error}", file=sys.stderr)
        sys.exit(1)
