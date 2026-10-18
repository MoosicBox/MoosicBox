#!/usr/bin/env python3
"""Bounded live Albums PTY acceptance; never builds or contacts a live WebSocket.

Usage: python3 bmux_live_acceptance.py EXECUTABLE --artifacts /tmp/live-evidence
       python3 bmux_live_acceptance.py --self-test

Only the gateway contacts the fixed authorized server, using a deny-by-default
browsing GET allowlist. Redirects, upgrades, mutations, playback, downloads and
scans are rejected locally. Fresh HOME/XDG roots contain persisted connection
state pointing ONLY at the gateway. No production read-only behavior is added.

Decoded Kitty RGBA/PNG images, text frames, raw terminal output, request timings
and failures are retained. By default a PTY is not a terminal compositor: decoded
images do not prove host placement/clipping. Optional --kitty-screenshots mirrors
the PTY into Kitty and captures a dedicated 1280x1100x24 Xvfb framebuffer using
libX11. Set BMUX_LIVE_ISOLATED_X11 to that DISPLAY explicitly; never point it at
the user's desktop. Frame capture adds diagnostic overhead between input samples.
A successful run is still not bmux-host or statistical p95 acceptance. No
fabricated WS Ready state is supplied.
"""

import argparse
import ctypes
import concurrent.futures
import multiprocessing
import errno
import fcntl
import hashlib
import http.server
import json
import math
import os
from pathlib import Path
import pty
import queue
import re
import select
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import threading
import tty
import time
import unittest
import urllib.error
import urllib.parse
import urllib.request
from unittest import mock
import zlib

from bmux_pty_acceptance import Screen, decode_png

UPSTREAM = "http://192.168.1.156:8500"
# Exact API browsing routes; no generic /menu, /files, /session or /config prefix.
GET_ROUTES = {
    "/health": set(),
    "/config/profiles": set(),
    "/music-api": {"offset", "limit"},
    "/audio-zone/with-session": set(),
    "/session/sessions": set(),
    "/menu/albums": {"offset", "limit", "sort", "source", "sources", "search"},
    "/menu/album": {"albumId", "source"},
    "/menu/album/versions": {"albumId", "source", "offset", "limit"},
    "/menu/album/tracks": {"albumId", "source", "offset", "limit"},
}
MAX_BODY = 16 * 1024 * 1024


def allowed_get(target, headers):
    """Reject alternate authorities, encoded paths, upgrades and unknown queries."""
    parsed = urllib.parse.urlsplit(target)
    if (
        parsed.scheme
        or parsed.netloc
        or parsed.fragment
        or "%" in parsed.path
        or "\\" in parsed.path
        or "//" in parsed.path
        or headers.get("Upgrade")
        or "upgrade" in headers.get("Connection", "").lower()
    ):
        return False
    keys = GET_ROUTES.get(parsed.path)
    if keys is None:
        if not re.fullmatch(r"/files/albums/[0-9]+/[0-9]{1,4}x[0-9]{1,4}", parsed.path):
            return False
        keys = {"source"}
    query = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)
    if not set(query).issubset(keys | {"moosicboxProfile"}):
        return False
    if any(len(values) != 1 for values in query.values()):
        return False
    if query.get("moosicboxProfile", ["master"]) != ["master"]:
        return False
    for key in ("offset", "limit", "albumId"):
        if key in query and not re.fullmatch(r"[0-9]{1,8}", query[key][0]):
            return False
    return len(target) <= 2048


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """Never follow a browsing response to another route or authority."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Gateway(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), GatewayHandler)
        self.requests = []
        self.payloads = []
        self.started = time.monotonic()
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}), NoRedirect()
        )
        self.forwarded_bytes = 0


class GatewayHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def deny(self):
        self.server.requests.append(
            {
                "t": time.monotonic() - self.server.started,
                "method": self.command,
                "path": self.path,
                "denied": True,
            }
        )
        self.send_error(403, "Read-only browsing gateway denied request")
        self.close_connection = True

    def __getattr__(self, name):
        # Includes arbitrary extension methods, CONNECT, HEAD and OPTIONS.
        if name.startswith("do_"):
            return self.deny
        raise AttributeError(name)

    def do_GET(self):
        if not allowed_get(self.path, self.headers):
            self.deny()
            return
        if (
            len(self.server.requests) >= 200
            or self.server.forwarded_bytes >= 32 * MAX_BODY
        ):
            self.deny()
            return
        started = time.monotonic()
        entry = {"t": started - self.server.started, "method": "GET", "path": self.path}
        self.server.requests.append(entry)
        # Do not forward credentials, cookies, Host, arbitrary headers or a body.
        request = urllib.request.Request(
            UPSTREAM + self.path, headers={"moosicbox-profile": "master"}, method="GET"
        )
        try:
            with self.server.opener.open(request, timeout=5) as response:
                body = response.read(MAX_BODY + 1)
                if len(body) > MAX_BODY:
                    raise ValueError("upstream response exceeds bound")
                entry.update(status=response.status, bytes=len(body))
                self.server.forwarded_bytes += len(body)
                if self.path.startswith("/menu/"):
                    self.server.payloads.append((self.path, json.loads(body)))
                self.send_response(response.status)
                self.send_header(
                    "Content-Type",
                    response.headers.get("Content-Type", "application/json"),
                )
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
        except urllib.error.HTTPError as error:
            entry.update(status=error.code, error=str(error))
            self.send_error(502, "Upstream browsing response rejected")
        except (OSError, ValueError) as error:
            entry["error"] = str(error)
            try:
                self.send_error(502, "Upstream browsing failure")
            except OSError:
                pass
        finally:
            entry["seconds"] = time.monotonic() - started


def write_png(path, width, height, pixels):
    """Save actual decoded RGBA pixels, not an escape-count assertion."""

    def chunk(kind, data):
        return (
            struct.pack("!I", len(data))
            + kind
            + data
            + struct.pack("!I", zlib.crc32(kind + data))
        )

    stride = width * 4
    raw = b"".join(b"\0" + pixels[y * stride : (y + 1) * stride] for y in range(height))
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack("!2I5B", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def tracks_in(value):
    """Extract API track objects without treating album metadata as tracks."""
    found = {}
    if isinstance(value, dict):
        if "trackId" in value and "title" in value:
            found[str(value["trackId"])] = value
        for child in value.values():
            found.update(tracks_in(child))
    elif isinstance(value, list):
        for child in value:
            found.update(tracks_in(child))
    return found


def decode_cover_worker(params, image, position, output):
    """CPU-heavy decoding/color inspection in a spawned process, never reader GIL."""
    if position[0] <= 25:
        return None
    if params.get("f") == "100":
        width, height, pixels = decode_png(image)
    elif params.get("f") in ("24", "32"):
        width, height = int(params["s"]), int(params["v"])
        assert 0 < width <= 4096 and 0 < height <= 4096
        channels = int(params["f"]) // 8
        assert len(image) == width * height * channels
        pixels = (
            image
            if channels == 4
            else b"".join(image[i : i + 3] + b"\xff" for i in range(0, len(image), 3))
        )
    else:
        return None
    if min(width, height) < 100:
        return None
    colors = set()
    for index in range(0, len(pixels), 4):
        colors.add(pixels[index : index + 4])
        if len(colors) > 32:
            break
    if len(colors) <= 32:
        return None
    write_png(Path(output), width, height, pixels)
    return {
        "file": Path(output).name,
        "position_cells": position,
        "width": width,
        "height": height,
        "sha256_rgba": hashlib.sha256(pixels).hexdigest(),
    }


class LockedScreen(Screen):
    """Serialize parser mutation and observations without holding locks for I/O."""

    def __init__(self, width=140, height=60):
        super().__init__(width, height)
        self.lock = threading.RLock()

    def feed(self, data):
        with self.lock:
            super().feed(data)

    def text(self):
        with self.lock:
            return super().text()

    def locate(self, label, occurrence=0):
        with self.lock:
            return super().locate(label, occurrence)


class PtyDrain:
    """Continuously receive and timestamp PTY bytes, independent of screenshots."""

    def __init__(self, master, screen, recording, mirror=False, observe=None):
        self.master, self.screen, self.recording = master, screen, recording
        self.mirror, self.observe = mirror, observe
        self.stopping = threading.Event()
        self.received = []
        self.parsed = []
        self.error = None
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.parse_queue = queue.Queue()
        self.mirror_queue = queue.Queue()
        self.parser = threading.Thread(target=self.parse, daemon=True)
        self.forwarder = threading.Thread(target=self.forward, daemon=True)

    def parse(self):
        try:
            while True:
                item = self.parse_queue.get()
                if item is None:
                    return
                received, data = item
                self.screen.feed(data)
                if self.observe:
                    self.observe(received)
                self.parsed.append((received, time.monotonic()))
        except Exception as error:
            self.error = repr(error)

    def forward(self):
        try:
            while True:
                data = self.mirror_queue.get()
                if data is None:
                    return
                remaining = memoryview(data)
                while remaining:
                    remaining = remaining[os.write(1, remaining) :]
        except Exception as error:
            self.error = repr(error)

    def run(self):
        try:
            while not self.stopping.is_set():
                ready, _, _ = select.select(
                    [self.master, 0] if self.mirror else [self.master], [], [], 0.005
                )
                if self.mirror and 0 in ready:
                    data = os.read(0, 65536)
                    if data:
                        os.write(self.master, data)
                if self.master not in ready:
                    continue
                try:
                    data = os.read(self.master, 1024 * 1024)
                except OSError as error:
                    if error.errno == errno.EIO:
                        return
                    raise
                if not data:
                    return
                received = time.monotonic()
                self.recording.extend(data)
                self.received.append((received, len(data)))
                self.parse_queue.put((received, data))
                if self.mirror:
                    self.mirror_queue.put(data)
        except Exception as error:
            self.error = repr(error)

    def start(self):
        self.parser.start()
        if self.mirror:
            self.forwarder.start()
        self.thread.start()

    def close(self):
        self.stopping.set()
        self.thread.join(timeout=2)
        self.parse_queue.put(None)
        self.mirror_queue.put(None)
        self.parser.join(timeout=2)
        if self.mirror:
            self.forwarder.join(timeout=2)


def album_positions(screen, payloads):
    """Locate unique real album titles, excluding controls and ambiguous labels."""
    titles = {}
    for path, payload in payloads:
        if urllib.parse.urlsplit(path).path != "/menu/albums" or not isinstance(
            payload, dict
        ):
            continue
        for album in payload.get("items", []):
            title = album.get("title", "")
            if title:
                titles.setdefault(title, set()).add(str(album["albumId"]))
    positions = {}
    lines = screen.text().splitlines()
    for title, identities in titles.items():
        if len(identities) != 1:
            continue
        matches = [
            (line.find(title), row) for row, line in enumerate(lines) if title in line
        ]
        if len(matches) == 1 and matches[0][0] > 25:
            positions[next(iter(identities))] = matches[0]
    return positions


def albums_moved_up(before, after):
    """Require upward movement of the SAME album, not unrelated frame churn.

    Newly appearing/disappearing labels alone cannot prove a wheel was applied:
    background pagination or partial repaint may also change visibility.
    """
    return any(
        identity in after
        and after[identity][0] == position[0]
        and after[identity][1] < position[1]
        for identity, position in before.items()
    )


def capture_x11(path):
    """Capture the actual dedicated Xvfb framebuffer, including Kitty graphics.

    Only use with an isolated 1280x1100, 24-bit TrueColor Xvfb display; never
    capture the user's desktop. No ffmpeg x11grab or Pillow dependency required.
    """
    x = ctypes.CDLL("libX11.so.6")
    pointer, integer, unsigned = ctypes.c_void_p, ctypes.c_int, ctypes.c_ulong
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XOpenDisplay.restype = pointer
    x.XDefaultRootWindow.argtypes = [pointer]
    x.XDefaultRootWindow.restype = unsigned
    x.XGetImage.argtypes = [
        pointer,
        unsigned,
        integer,
        integer,
        ctypes.c_uint,
        ctypes.c_uint,
        unsigned,
        integer,
    ]
    x.XGetImage.restype = pointer
    x.XGetPixel.argtypes = [pointer, integer, integer]
    x.XGetPixel.restype = unsigned
    x.XDestroyImage.argtypes = [pointer]
    x.XCloseDisplay.argtypes = [pointer]
    display = x.XOpenDisplay(os.environ["DISPLAY"].encode())
    if not display:
        raise OSError("Cannot open isolated X11 screenshot display")
    image = None
    try:
        image = x.XGetImage(
            display, x.XDefaultRootWindow(display), 0, 0, 1280, 1100, 0xFFFFFFFF, 2
        )
        if not image:
            raise OSError("Cannot capture isolated X11 framebuffer")
        pixels = bytearray()
        for y in range(1100):
            for column in range(1280):
                pixel = x.XGetPixel(image, column, y)
                pixels.extend(
                    ((pixel >> 16) & 255, (pixel >> 8) & 255, pixel & 255, 255)
                )
        write_png(path, 1280, 1100, pixels)
    finally:
        if image:
            x.XDestroyImage(image)
        x.XCloseDisplay(display)


def run(executable, artifacts, timeout, kitty_screenshots=False):
    executable = Path(executable).resolve(strict=True)
    artifacts = Path(artifacts).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    with executable.open("rb") as binary:
        identity = hashlib.file_digest(binary, "sha256").hexdigest()
    gateway = Gateway()
    threading.Thread(target=gateway.serve_forever, daemon=True).start()
    outer_modes = None
    if kitty_screenshots:
        if (
            not os.isatty(0)
            or not os.isatty(1)
            or os.environ.get("BMUX_LIVE_ISOLATED_X11") != os.environ.get("DISPLAY")
            or not os.environ.get("DISPLAY")
            or os.environ["DISPLAY"] in (":0", ":0.0")
        ):
            raise ValueError(
                "Screenshots require a Kitty TTY on explicitly marked isolated Xvfb"
            )
    screen = LockedScreen(140, 60)
    master, slave = pty.openpty()
    geometry = struct.pack("HHHH", 60, 140, 1120, 960)
    if kitty_screenshots:
        geometry = fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
        rows, columns, _, _ = struct.unpack("HHHH", geometry)
        screen = LockedScreen(columns, rows)
        outer_modes = termios.tcgetattr(0)
        tty.setraw(0)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, geometry)
    original_modes = termios.tcgetattr(slave)
    recording = bytearray()
    events, failures, checks, decoded, seen_tracks = [], [], {}, [], set()
    process = None
    started = time.monotonic()
    deadline = started + timeout
    image_cursor = 0
    first_cover = None
    album_clicked = None
    observations = []
    image_received = {}
    drain = None
    decode_pool = concurrent.futures.ProcessPoolExecutor(
        max_workers=1, mp_context=multiprocessing.get_context("spawn")
    )
    decode_jobs = []
    measuring_input = False

    def observe(received):
        # This callback is cheap; PNG decoding and disk I/O stay off the drain.
        with screen.lock:
            for index in range(len(image_received), len(screen.images)):
                image_received[index] = received
            positions = album_positions(screen, list(gateway.payloads))
        observations.append((received, positions))

    def event(kind, **data):
        events.append({"t": time.monotonic() - started, "kind": kind, **data})

    def check(name, condition, explanation):
        checks[name] = bool(condition)
        if not condition:
            failures.append(explanation)

    def decode_images():
        nonlocal image_cursor, first_cover
        if measuring_input:
            return
        with screen.lock:
            pending_images = list(enumerate(screen.images[image_cursor:], image_cursor))
            image_cursor = len(screen.images)
        for image_index, (params, image, position) in pending_images:
            future = decode_pool.submit(
                decode_cover_worker,
                params,
                image,
                position,
                str(artifacts / f"decoded-cover-{image_index}.png"),
            )
            decode_jobs.append(
                (future, image_received.get(image_index, time.monotonic()))
            )
        for future, received in list(decode_jobs):
            if not future.done():
                continue
            decode_jobs.remove((future, received))
            try:
                value = future.result()
                if value:
                    first_cover = (
                        received if first_cover is None else min(first_cover, received)
                    )
                    value["received_seconds"] = received - started
                    decoded.append(value)
            except Exception as error:
                failures.append(f"Image decode failed: {error}")

    def pump():
        if drain and drain.error:
            raise OSError("PTY drain failed: " + drain.error)
        time.sleep(0.002)
        decode_images()
        for _, payload in list(gateway.payloads):
            for identity_, track in tracks_in(payload).items():
                if screen.locate(track["title"]):
                    seen_tracks.add(identity_)

    def wait(predicate, description, seconds=10):
        until = min(deadline, time.monotonic() + seconds)
        while time.monotonic() < until:
            pump()
            if predicate():
                return
            if process.poll() is not None:
                raise AssertionError(
                    f"Executable exited while waiting for {description}"
                )
        raise AssertionError(f"Timed out: {description}")

    def settle(seconds):
        until = min(deadline, time.monotonic() + seconds)
        while time.monotonic() < until:
            pump()

    def frame(name):
        (artifacts / f"{name}.txt").write_text(screen.text())
        event("frame", name=name, decoded_covers=len(decoded))
        if kitty_screenshots:
            # Snapshot work is diagnostic and outside input-latency sample intervals.
            termios.tcdrain(1)
            time.sleep(0.05)
            subprocess.run(
                [
                    os.sys.executable,
                    "-c",
                    "import sys; from pathlib import Path; "
                    "sys.path.insert(0, sys.argv[1]); "
                    "from bmux_live_acceptance import capture_x11; "
                    "capture_x11(Path(sys.argv[2]))",
                    str(Path(__file__).resolve().parent),
                    str(artifacts / f"{name}-kitty.png"),
                ],
                check=True,
                timeout=8,
            )

    def click(label):
        position = screen.locate(label)
        assert position, f"Missing clickable label: {label}"
        x, y = position
        # Click inside, not one past a short title's hit rectangle.
        os.write(master, f"\x1b[<0;{x + 1};{y + 1}M\x1b[<0;{x + 1};{y + 1}m".encode())
        event("click", label=label, x=x, y=y)

    with tempfile.TemporaryDirectory(prefix="bmux-live-read-only-") as home:
        database = Path(home) / ".local/moosicbox/app/profiles/master/persistence.db"
        database.parent.mkdir(parents=True)
        connection = {
            "name": "Read-only live acceptance",
            "api_url": f"http://127.0.0.1:{gateway.server_port}",
        }
        with sqlite3.connect(database) as persistence:
            persistence.execute(
                "CREATE TABLE state (key TEXT NOT NULL UNIQUE, value TEXT NOT NULL)"
            )
            for key, value in [
                ("CONNECTION", connection),
                ("CONNECTIONS", [connection]),
            ]:
                persistence.execute(
                    "INSERT INTO state VALUES (?, ?)", (key, json.dumps(value))
                )
        env = {
            key: value
            for key, value in os.environ.items()
            if key in ("PATH", "LANG", "LC_ALL", "LD_LIBRARY_PATH")
        }
        env.update(
            HOME=home,
            XDG_CONFIG_HOME=home + "/config",
            XDG_STATE_HOME=home + "/state",
            XDG_DATA_HOME=home + "/data",
            XDG_CACHE_HOME=home + "/cache",
            TERM="xterm-256color",
            BMUX_IMAGE_PROTOCOL="kitty",
            BMUX_PANE="live-read-only-test",
            RUST_LOG="hyperchad=debug,moosicbox_app_native=debug",
        )
        try:
            process = subprocess.Popen(
                [str(executable), "serve"],
                stdin=slave,
                stdout=slave,
                stderr=slave,
                env=env,
                cwd=home,
                start_new_session=True,
            )
            drain = PtyDrain(master, screen, recording, kitty_screenshots, observe)
            drain.start()
            wait(lambda: screen.locate("Albums"), "startup Albums navigation")
            structure_seconds = time.monotonic() - started
            event("structure", seconds=structure_seconds)
            check(
                "cold_structure",
                structure_seconds <= 0.3,
                f"Cold structure {structure_seconds:.3f}s exceeds 300ms",
            )
            # Startup can replace the initial navigation layout after its first
            # text arrives. Do not click coordinates from that transient frame.
            # Structure timing above remains measured from initial appearance;
            # cover timing below starts only after the actual navigation click.
            settle(0.3)
            click("Albums")
            albums_start = time.monotonic()
            try:
                wait(lambda: bool(decoded), "actual decoded real artwork", 15)
            except AssertionError as error:
                failures.append(str(error))
            checks["decoded_real_artwork"] = bool(decoded)
            cover_seconds = first_cover - albums_start if first_cover else None
            event("first_cover", seconds=cover_seconds)
            # This is stricter than adding server time: report both for interpretation.
            cover_server = max(
                (
                    r.get("seconds", 0)
                    for r in gateway.requests
                    if r["path"].startswith("/files/albums/")
                ),
                default=0,
            )
            check(
                "first_cover",
                cover_seconds is not None and cover_seconds <= 0.3 + cover_server,
                (
                    "No decoded cover within deadline"
                    if cover_seconds is None
                    else f"First decoded cover {cover_seconds:.3f}s exceeds 300ms + cover server time"
                ),
            )
            # Initial viewport has two complete three-column cover rows at this
            # explicit geometry. Require six distinct real image payloads, not icons.
            try:
                wait(
                    lambda: len({image["sha256_rgba"] for image in decoded}) >= 6,
                    "six visible initial-grid covers",
                    4,
                )
            except AssertionError as error:
                failures.append(str(error))
            visible_set = sorted(
                {
                    image["sha256_rgba"]: image["received_seconds"] for image in decoded
                }.values()
            )
            visible_set_seconds = (
                visible_set[5] - (albums_start - started)
                if len(visible_set) >= 6
                else None
            )
            event(
                "visible_cover_set",
                seconds=visible_set_seconds,
                decoded_distinct=len(visible_set),
                expected_complete_covers=6,
            )
            check(
                "visible_cover_set",
                visible_set_seconds is not None and visible_set_seconds <= 1,
                "Six visible real covers not received within 1s",
            )
            frame("albums")
            wait(
                lambda: bool(album_positions(screen, list(gateway.payloads))),
                "real album labels after asynchronous list load",
                12,
            )
            positions_before = album_positions(screen, list(gateway.payloads))
            assert (
                positions_before
            ), "No unambiguous album labels to measure wheel movement"
            scroll_start = time.monotonic()
            os.write(master, b"\x1b[<65;90;30M")
            scroll_seconds = None
            try:
                wait(
                    lambda: albums_moved_up(
                        positions_before,
                        album_positions(screen, list(gateway.payloads)),
                    ),
                    "same album title moving upward after wheel",
                    3,
                )
                scroll_seconds = next(
                    (
                        received - scroll_start
                        for received, positions in list(observations)
                        if received >= scroll_start
                        and albums_moved_up(positions_before, positions)
                    ),
                    time.monotonic() - scroll_start,
                )
            except AssertionError as error:
                failures.append(str(error))
            event(
                "scroll",
                seconds=scroll_seconds,
                before_positions=positions_before,
                after_positions=album_positions(screen, list(gateway.payloads)),
            )
            check(
                "scroll_latency",
                scroll_seconds is not None and scroll_seconds <= 0.1,
                "No confirmed album displacement within 100ms (not a statistical p95)",
            )
            # Sixteen sequential down-wheel samples; only same-identity upward
            # motion timestamps from the drain count, including events during capture.
            # Finish outstanding decode subprocess work before the input window;
            # no screenshots, PNG inspection, or PNG writes occur during samples.
            wait(lambda: not decode_jobs, "decode worker quiescent before input", 5)
            measuring_input = True
            samples = []
            for sample_index in range(16):
                before = album_positions(screen, list(gateway.payloads))
                # Let any previously received frames finish parsing before marking
                # a new input; otherwise stale buffered motion can look like 0ms.
                settle(0.08)
                before = album_positions(screen, list(gateway.payloads))
                sent = time.monotonic()
                os.write(master, b"\x1b[<65;90;30M")
                matched = None
                until = min(deadline, sent + 1)
                while time.monotonic() < until:
                    pump()
                    matched = next(
                        (
                            received
                            for received, positions in list(observations)
                            if received >= sent and albums_moved_up(before, positions)
                        ),
                        None,
                    )
                    if matched is not None:
                        break
                seconds = matched - sent if matched is not None else None
                samples.append(seconds)
                event(
                    "wheel_sample",
                    index=sample_index,
                    seconds=seconds,
                    before_positions=before,
                    after_positions=album_positions(screen, list(gateway.payloads)),
                )
            measuring_input = False
            completed = sorted(value for value in samples if value is not None)
            p95 = (
                completed[math.ceil(0.95 * len(completed)) - 1]
                if len(completed) == 16
                else None
            )
            event(
                "input_p95",
                seconds=p95,
                samples=16,
                completed=len(completed),
                over250ms=sum(value > 0.25 for value in completed),
            )
            check(
                "input_p95",
                p95 is not None and p95 <= 0.1,
                f"16 wheel samples p95={p95!r}s exceeds 100ms or samples missing",
            )
            check(
                "input_stalls",
                len(completed) == 16 and all(value <= 0.25 for value in completed),
                "Wheel samples missing or recurring >250ms stalls",
            )
            frame("scrolled-samples")
            candidates = []
            for path, payload in list(gateway.payloads):
                if urllib.parse.urlsplit(path).path != "/menu/albums":
                    continue
                for album in payload.get("items", []):
                    title = album.get("title", "")
                    position = screen.locate(title) if title else None
                    if position and position[0] > 25:
                        candidates.append((position[1], album))
            assert candidates, "No real album title visible for safe click"
            album_clicked = min(candidates, key=lambda item: item[0])[1]
            preserved = screen.text()
            preserved_positions = album_positions(screen, list(gateway.payloads))
            event("list_checkpoint", positions=preserved_positions)
            click_started = time.monotonic()
            click(album_clicked["title"])
            selected_id = str(album_clicked["albumId"])

            def selected_details():
                for path, payload in list(gateway.payloads):
                    parsed = urllib.parse.urlsplit(path)
                    if (
                        parsed.path in ("/menu/album/versions", "/menu/album/tracks")
                        and urllib.parse.parse_qs(parsed.query).get("albumId")
                        == [selected_id]
                        and tracks_in(payload)
                    ):
                        return True
                return False

            wait(selected_details, "clicked album's actual detail tracks", 12)
            check(
                "correct_album",
                any(
                    urllib.parse.urlsplit(path).path == "/menu/album"
                    and str(payload.get("albumId")) == selected_id
                    for path, payload in gateway.payloads
                    if isinstance(payload, dict)
                ),
                "Detail metadata does not match clicked album",
            )
            wait(lambda: bool(seen_tracks), "rendered real track titles", 5)
            check(
                "detail_metadata_rendered",
                bool(screen.locate(album_clicked["title"]))
                and bool(screen.locate(album_clicked["artist"])),
                "Clicked album title/artist missing from detail frame",
            )
            frame("details")
            expected_tracks = {}
            for path, payload in list(gateway.payloads):
                if urllib.parse.parse_qs(urllib.parse.urlsplit(path).query).get(
                    "albumId"
                ) == [selected_id]:
                    expected_tracks.update(tracks_in(payload))
            for _ in range(20):
                if (
                    set(expected_tracks).issubset(seen_tracks)
                    or time.monotonic() >= deadline
                ):
                    break
                os.write(master, b"\x1b[<65;90;30M")
                settle(0.1)
            check(
                "complete_track_list",
                bool(expected_tracks) and set(expected_tracks).issubset(seen_tracks),
                f"Only {len(seen_tracks)}/{len(expected_tracks)} API track titles observed",
            )
            event(
                "click_details",
                seconds=time.monotonic() - click_started,
                expected_tracks=len(expected_tracks),
                observed_tracks=len(seen_tracks),
            )
            frame("details-scrolled")
            # Compare exact real album identities/coordinates independently of
            # icon/image completion, and retain API evidence even when Back fails.
            back_request_index = len(gateway.requests)
            back_start = time.monotonic()
            os.write(master, b"\x1b[1;3D")
            restored = False
            try:
                wait(
                    lambda: bool(preserved_positions)
                    and album_positions(screen, list(gateway.payloads))
                    == preserved_positions,
                    "Back preserving exact Albums identity coordinates",
                    3,
                )
                restored = True
            except AssertionError as error:
                failures.append(str(error))
            back_seconds = time.monotonic() - back_start
            additional_api = [
                request
                for request in gateway.requests[back_request_index:]
                if not request.get("denied") and request["path"].startswith("/menu/")
            ]
            event(
                "back_result",
                restored=restored,
                seconds=back_seconds,
                expected_positions=preserved_positions,
                actual_positions=album_positions(screen, list(gateway.payloads)),
                additional_browsing_api=additional_api,
                exact_text_match=screen.text() == preserved,
            )
            check(
                "back_position",
                restored,
                "Back did not restore exact album identity coordinates",
            )
            check(
                "back_no_api",
                not additional_api,
                "Back issued additional menu API requests",
            )
            check(
                "cached_back",
                restored and back_seconds <= 0.1,
                "Cached Back exceeds 100ms or failed",
            )
            frame("back")
        except (AssertionError, OSError, ValueError) as error:
            failures.append(str(error))
            checks.setdefault("decoded_real_artwork", bool(decoded))
            checks.setdefault("correct_album", False)
            checks.setdefault("complete_track_list", False)
            checks.setdefault("cached_back", False)
            frame("failure")
        finally:
            if process and process.poll() is None:
                quit_start = time.monotonic()
                os.write(master, b"\x03")
                while process.poll() is None and time.monotonic() - quit_start < 1:
                    pump()
                quit_seconds = time.monotonic() - quit_start
                check(
                    "quit",
                    process.poll() == 0 and quit_seconds <= 1,
                    f"Quit failed 1s budget: {quit_seconds:.3f}s, exit={process.poll()}",
                )
                event("quit", seconds=quit_seconds, returncode=process.poll())
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=2)
            check(
                "terminal_restored",
                termios.tcgetattr(slave) == original_modes,
                "Terminal modes not restored",
            )
            for index, log in enumerate(Path(home).rglob("*.log")):
                (artifacts / f"app-{index}.log").write_bytes(log.read_bytes())
            decode_pool.shutdown(wait=True, cancel_futures=True)
            if drain:
                drain.close()
            gateway.shutdown()
            gateway.server_close()
            os.close(master)
            os.close(slave)
            if outer_modes is not None:
                termios.tcsetattr(0, termios.TCSANOW, outer_modes)
    result = {
        "result": "failed" if failures else "partial_acceptance",
        "executable": str(executable),
        "sha256": identity,
        "upstream": UPSTREAM,
        "checks": checks,
        "failures": failures,
        "events": events,
        "decoded_covers": decoded,
        "clicked_album": album_clicked,
        "observed_track_ids": sorted(seen_tracks),
        "kitty_screenshots": kitty_screenshots,
        "geometry": dict(
            zip(
                ("rows", "columns", "width_pixels", "height_pixels"),
                struct.unpack("HHHH", geometry),
            )
        ),
        "limitations": [
            (
                "Kitty/Xvfb framebuffer captured; bmux host integration unverified"
                if kitty_screenshots
                else "No terminal compositor: placement/clipping unverified"
            ),
            "No statistical input p95 or complete visible-cover set timing",
            "Live WebSocket denied; startup may remain disconnected",
        ],
    }
    (artifacts / "result.json").write_text(json.dumps(result, indent=2))
    (artifacts / "requests.json").write_text(json.dumps(gateway.requests, indent=2))
    (artifacts / "responses.json").write_text(json.dumps(gateway.payloads, indent=2))
    (artifacts / "terminal.bin").write_bytes(recording)
    if drain:
        (artifacts / "pty-receipts.json").write_text(
            json.dumps(
                {
                    "received": [
                        (received - started, size) for received, size in drain.received
                    ],
                    "parse_lag_seconds": [
                        parsed - received for received, parsed in drain.parsed
                    ],
                    "error": drain.error,
                },
                indent=2,
            )
        )
    print(
        json.dumps(
            {
                "result": result["result"],
                "failures": failures,
                "checks": checks,
                "artifacts": str(artifacts),
            }
        )
    )
    return 1 if failures else 0


class SafetyTests(unittest.TestCase):
    def test_allowlist(self):
        for target in (
            "/health",
            "/menu/albums?offset=0&limit=100&sort=name",
            "/menu/album/versions?albumId=1339&source=Library",
            "/files/albums/1339/266x266?source=Library&moosicboxProfile=master",
        ):
            self.assertTrue(allowed_get(target, {}), target)
        for target in (
            "/ws",
            "/session/register-players",
            "/scan",
            "/downloader/download",
            "/files/tracks/1",
            "http://evil/health",
            "//evil/health",
            "/menu/%61lbums",
            "/menu/albums/../album",
            "/menu/albums?url=http://evil",
            "/menu/albums?limit=1&limit=2",
            "/health#x",
        ):
            self.assertFalse(allowed_get(target, {}), target)
        self.assertFalse(allowed_get("/health", {"Upgrade": "websocket"}))
        self.assertFalse(allowed_get("/health", {"Connection": "keep-alive, Upgrade"}))

    def test_denials_never_open_upstream(self):
        gateway = Gateway()
        threading.Thread(target=gateway.serve_forever, daemon=True).start()
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with mock.patch.object(
                gateway.opener, "open", side_effect=AssertionError("unsafe forward")
            ) as upstream:
                for method, path, headers in [
                    ("POST", "/menu/albums", {}),
                    ("PUT", "/health", {}),
                    ("DELETE", "/health", {}),
                    ("PATCH", "/health", {}),
                    ("CONNECT", "/health", {}),
                    ("GET", "/ws", {"Upgrade": "websocket"}),
                    ("GET", "/session/register-connection", {}),
                ]:
                    request = urllib.request.Request(
                        f"http://127.0.0.1:{gateway.server_port}{path}",
                        method=method,
                        headers=headers,
                    )
                    with self.assertRaises(urllib.error.HTTPError) as error:
                        opener.open(request, timeout=2)
                    self.assertEqual(error.exception.code, 403)
                upstream.assert_not_called()
        finally:
            gateway.shutdown()
            gateway.server_close()

    def test_decode_process_does_not_stall_drain(self):
        pixels = b"\xff\0\0\xff" * 128 * 128
        with tempfile.TemporaryDirectory() as directory:
            with concurrent.futures.ProcessPoolExecutor(
                max_workers=1, mp_context=multiprocessing.get_context("spawn")
            ) as pool:
                result = pool.submit(
                    decode_cover_worker,
                    {"f": "32", "s": "128", "v": "128"},
                    pixels,
                    (40, 10),
                    str(Path(directory) / "flat.png"),
                )
                self.assertIsNone(result.result(timeout=5))

    def test_reader_not_blocked_by_parser(self):
        master, slave = pty.openpty()
        tty.setraw(slave)
        blocked, release = threading.Event(), threading.Event()

        class SlowScreen(LockedScreen):
            def feed(self, data):
                blocked.set()
                release.wait(1)
                super().feed(data)

        recording = bytearray()
        drain = PtyDrain(master, SlowScreen(), recording)
        drain.start()
        try:
            os.write(slave, b"first")
            self.assertTrue(blocked.wait(0.3))
            os.write(slave, b"second")
            until = time.monotonic() + 0.3
            while b"second" not in recording and time.monotonic() < until:
                time.sleep(0.005)
            self.assertIn(b"second", recording, "Parser backpressured raw PTY reader")
        finally:
            release.set()
            drain.close()
            os.close(master)
            os.close(slave)

    def test_drain_continues_during_screenshot_pause(self):
        master, slave = pty.openpty()
        tty.setraw(slave)
        screen, recording = LockedScreen(), bytearray()
        observed = threading.Event()
        drain = PtyDrain(
            master, screen, recording, observe=lambda received: observed.set()
        )
        drain.start()
        try:
            # Simulate the main thread spending 1.2s waiting for a framebuffer
            # subprocess. PTY traffic must be consumed before that wait ends.
            writer = threading.Timer(
                0.03, lambda: os.write(slave, b"\x1b[10;40HDuring screenshot")
            )
            writer.start()
            self.assertTrue(observed.wait(0.3), "PTY reader paused behind screenshot")
            self.assertEqual(screen.locate("During screenshot"), (39, 9))
            self.assertIn(b"During screenshot", recording)
            subprocess.run(
                [os.sys.executable, "-c", "import time; time.sleep(.05)"], check=True
            )
            self.assertIsNone(drain.error)
        finally:
            writer.join()
            drain.close()
            os.close(master)
            os.close(slave)

    def test_scroll_requires_album_displacement(self):
        before = {"1339": (83, 19), "1936": (73, 36)}
        self.assertFalse(albums_moved_up(before, dict(before)))
        self.assertFalse(albums_moved_up(before, {"1318": (83, 10)}))
        self.assertFalse(albums_moved_up(before, {"1339": (84, 18)}))
        self.assertFalse(albums_moved_up(before, {"1339": (83, 20)}))
        self.assertTrue(albums_moved_up(before, {"1339": (83, 18)}))

    def test_album_positions_ignore_frame_churn(self):
        screen = Screen()
        payloads = [
            ("/menu/albums", {"items": [{"albumId": 1339, "title": "Unique Album"}]})
        ]
        screen.feed(b"\x1b[20;70HUnique Album")
        before = album_positions(screen, payloads)
        self.assertEqual(before, {"1339": (69, 19)})
        screen.feed(b"\x1b[2;2HPlayer changed\x1b[4;4HFilter changed")
        self.assertFalse(albums_moved_up(before, album_positions(screen, payloads)))
        screen.feed(b"\x1b[20;70H            \x1b[19;70HUnique Album")
        self.assertTrue(albums_moved_up(before, album_positions(screen, payloads)))

    def test_image_roundtrip(self):
        pixels = bytes((255, 0, 0, 255, 0, 255, 0, 255))
        with tempfile.TemporaryDirectory() as directory:
            image = Path(directory) / "decoded.png"
            write_png(image, 2, 1, pixels)
            self.assertEqual(decode_png(image.read_bytes()), (2, 1, pixels))

    def test_track_extraction(self):
        value = {
            "albumId": 1,
            "title": "album",
            "versions": [{"tracks": [{"trackId": 7, "title": "real track"}]}],
        }
        self.assertEqual(set(tracks_in(value)), {"7"})

    def test_redirects_denied(self):
        self.assertIsNone(
            NoRedirect().redirect_request(None, None, 302, "", {}, "http://evil")
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", nargs="?")
    parser.add_argument("--artifacts")
    parser.add_argument("--timeout", type=float, default=50)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--kitty-screenshots",
        action="store_true",
        help="Run inside Kitty on dedicated 1280x1100x24 Xvfb; mark DISPLAY with BMUX_LIVE_ISOLATED_X11",
    )
    arguments = parser.parse_args()
    if arguments.self_test:
        unittest.main(argv=[__file__])
    elif (
        not arguments.executable
        or not arguments.artifacts
        or not 1 <= arguments.timeout <= 90
    ):
        parser.error(
            "provide executable, new --artifacts directory and --timeout in 1..90 seconds"
        )
    else:
        raise SystemExit(
            run(
                arguments.executable,
                arguments.artifacts,
                arguments.timeout,
                arguments.kitty_screenshots,
            )
        )
