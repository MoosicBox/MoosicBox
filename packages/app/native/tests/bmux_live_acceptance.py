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

from collections import deque
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
    # Artwork identity is still established by decoded rich pixel content.
    # Compact terminal cards are 96px, unlike the former 200px desktop cards.
    if position[0] < 18:
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
    if min(width, height) < 96:
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
        "image_id": params.get("i"),
        "placement_columns": int(params.get("c", 0)),
        "placement_rows": int(params.get("r", 0)),
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


class QueueTelemetry:
    """Bounded in-memory per-stage windows; each stage has a single producer."""

    def __init__(self, limit=8192):
        self.windows = {
            stage: deque(maxlen=limit)
            for stage in (
                "receipt",
                "parse_enqueue",
                "parse_finish",
                "mirror_enqueue",
                "mirror_dequeue",
                "forward_start",
                "forward_end",
            )
        }
        self.counts = dict.fromkeys(self.windows, 0)
        self.bytes = dict.fromkeys(self.windows, 0)

    def record(self, stage, begin, end, timestamp=None):
        self.windows[stage].append(
            (begin, end, time.monotonic() if timestamp is None else timestamp)
        )
        self.counts[stage] += 1
        self.bytes[stage] += end - begin

    def snapshot(self):
        return {
            "coordinate": "cumulative application PTY bytes, half-open offsets",
            "counts": dict(self.counts),
            "bytes": dict(self.bytes),
            "windows": {
                stage: list(records) for stage, records in self.windows.items()
            },
        }


class PtyDrain:
    """Continuously receive and timestamp PTY bytes, independent of screenshots."""

    def __init__(self, master, screen, recording, mirror=False, observe=None):
        self.master, self.screen, self.recording = master, screen, recording
        self.mirror, self.observe = mirror, observe
        self.stopping = threading.Event()
        self.received = []
        self.parsed = []
        self.error = None
        self.telemetry = QueueTelemetry()
        self.byte_offset = 0
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
                received, data, begin, end = item
                self.screen.feed(data)
                if self.observe:
                    self.observe(received)
                finished = time.monotonic()
                self.parsed.append((received, finished))
                self.telemetry.record("parse_finish", begin, end, finished)
        except Exception as error:
            self.error = repr(error)

    def forward(self):
        try:
            while True:
                item = self.mirror_queue.get()
                if item is None:
                    return
                data, begin, end = item
                self.telemetry.record("mirror_dequeue", begin, end)
                remaining = memoryview(data)
                self.telemetry.record("forward_start", begin, end)
                while remaining:
                    remaining = remaining[os.write(1, remaining) :]
                self.telemetry.record("forward_end", begin, end)
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
                begin = self.byte_offset
                self.byte_offset += len(data)
                end = self.byte_offset
                self.telemetry.record("receipt", begin, end, received)
                self.telemetry.record("parse_enqueue", begin, end)
                self.parse_queue.put((received, data, begin, end))
                if self.mirror:
                    self.telemetry.record("mirror_enqueue", begin, end)
                    self.mirror_queue.put((data, begin, end))
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
            (match.start(), row)
            for row, line in enumerate(lines)
            for match in re.finditer(r"(?<!\w)" + re.escape(title) + r"(?!\w)", line)
        ]
        if len(matches) == 1 and matches[0][0] >= 18:
            positions[next(iter(identities))] = matches[0]
    return positions


def elapsed_receipt(received, origin):
    """Exclude pre-input receipts rather than clamping negative elapsed values."""
    return received - origin if received is not None and received >= origin else None


def current_cover_placements(graphics):
    """Replay placement/deletion commands; never select a deleted historical cover.

    Fail closed on deletion modes not modeled here rather than resurrecting stale
    placements. The returned IDs are terminal protocol identities, not album IDs.
    """
    active = {}
    for params, x, y in graphics:
        action = params.get("a")
        if action == "d":
            mode = params.get("d", "a").lower()
            if mode == "i":
                active = {
                    key: value
                    for key, value in active.items()
                    if key[0] != params.get("i")
                }
            elif mode == "p":
                active.pop((params.get("i"), params.get("p", "0")), None)
            else:
                active.clear()
        elif action in ("p", "T") and params.get("i"):
            active[(params["i"], params.get("p", "0"))] = (params, x, y)
    return list(active.values())


def ascii_display_label(value, width):
    """Mirror whole-ASCII-glyph ellipsize; unknown Unicode widths fail closed."""
    if not value.isascii() or width < 1:
        return None
    return value if len(value) <= width else value[: width - 1] + "…"


def centered_ascii_label(value, width):
    label = ascii_display_label(value, width)
    if label is None:
        return None
    spare = width - len(label)
    return " " * (spare // 2) + label + " " * (spare - spare // 2)


def connection_select_target(text, name, endpoint, main_bottom):
    """Resolve B's bracketed button only with its visible name and URL above it."""
    lines = text.splitlines()[:main_bottom]
    targets = []
    for y, line in enumerate(lines):
        for match in re.finditer(r"\[ Select \]", line):
            preceding = lines[max(0, y - 8) : y]
            names = [i for i, value in enumerate(preceding) if value.strip() == name]
            urls = [i for i, value in enumerate(preceding) if value.strip() == endpoint]
            if len(names) == len(urls) == 1 and names[0] < urls[0]:
                # A second form name between B and its button makes association unsafe.
                if not any(
                    "Fixture " in value and value.strip() != name
                    for value in preceding[names[0] + 1 :]
                ):
                    targets.append((match.start() + len("[ Select ]") // 2, y))
    return targets[0] if len(targets) == 1 else None


def selected_connection_visible(text, name, endpoint, main_bottom):
    """Require the selected marker, name and endpoint in one visible form."""
    lines = [line[18:].strip() for line in text.splitlines()[:main_bottom]]
    matches = []
    for y, value in enumerate(lines):
        if value != name:
            continue
        before = lines[max(0, y - 4) : y]
        after = lines[y + 1 : y + 5]
        if before.count("(Selected)") == 1 and after.count(endpoint) == 1:
            matches.append(y)
    return len(matches) == 1


def exact_sidebar_target(text, label, main_bottom):
    targets = [
        (line.index(label) + len(label) // 2, y)
        for y, line in enumerate(text.splitlines()[:main_bottom])
        if line[:18].strip() == label
    ]
    return targets[0] if len(targets) == 1 else None


def album_click_candidates(screen, payloads, card_columns=None):
    """Resolve exact displayed title/artist pairs uniquely across ALL loaded IDs."""
    albums = {}
    for path, payload in payloads:
        if urllib.parse.urlsplit(path).path == "/menu/albums":
            for album in payload.get("items", []):
                albums[str(album["albumId"])] = album
    lines = screen.text().splitlines()
    candidates = []
    if card_columns is None:
        # Full-label-only callers still use conservative complete identity lookup.
        positions = album_positions(screen, payloads)
        card_columns = {
            (x, max(len(a["title"]), len(a.get("artist", ""))))
            for identity, a in albums.items()
            if identity in positions
            for x, _ in [positions[identity]]
        }
    for x, width in sorted(set(card_columns)):
        displayed = {
            identity: (
                centered_ascii_label(a["title"], width),
                centered_ascii_label(a.get("artist", ""), width),
            )
            for identity, a in albums.items()
        }
        for y in range(8, len(lines) - 1):
            title = lines[y][x : x + width].ljust(width)
            artist = lines[y + 1][x : x + width].ljust(width)
            matching = [
                identity
                for identity, pair in displayed.items()
                if pair == (title, artist) and title and artist
            ]
            title_matches = [
                identity for identity, pair in displayed.items() if pair[0] == title
            ]
            if len(matching) == 1 and len(title_matches) == 1:
                candidates.append((y, albums[matching[0]], (x, y)))
    return sorted(candidates, key=lambda value: (value[0], value[2][0]))


def displayed_album_positions(screen, payloads, card_columns):
    """Use the safe click identity matcher for restoration, rejecting duplicate IDs."""
    candidates = album_click_candidates(screen, payloads, card_columns)
    positions = {}
    for _, album, position in candidates:
        identity = str(album["albumId"])
        if identity in positions:
            return {}
        positions[identity] = position
    return positions


def detail_metadata_matches(text, title, artist, main_left=18, footer_rows=4):
    """Match a complete contiguous wrapped heading and artist, never across cards."""
    lines = text.splitlines()[:-footer_rows]
    title = " ".join(title.split())
    for y, line in enumerate(lines):
        heading = line[main_left:]
        if not heading.strip():
            continue
        x = main_left + len(heading) - len(heading.lstrip())
        for count in range(1, 4):
            block = lines[y : y + count]
            if len(block) != count or any(
                value[main_left:x].strip() for value in block
            ):
                continue
            if any(
                not value[x:].strip() or value[x : x + 1].isspace() for value in block
            ):
                continue
            if " ".join(" ".join(value[x:].split()) for value in block) != title:
                continue
            if y + count < len(lines):
                following = lines[y + count]
                if (
                    not following[main_left:x].strip()
                    and following[x:].strip() == artist
                ):
                    return True
    return False


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


def settled_album_positions(settle, screen, payloads):
    """Read the input baseline AFTER old frames finish settling, never before."""
    settle(0.08)
    return album_positions(screen, payloads)


def clock_anchor():
    """Bracket UTC with monotonic readings; retain uncertainty, never subtract it."""
    before = time.monotonic_ns()
    wall = time.time_ns()
    after = time.monotonic_ns()
    return {"monotonic_before_ns": before, "wall_ns": wall, "monotonic_after_ns": after}


def classify_wheel_observations(before, observations, sent, until):
    """Classify same-identity motion without treating lost overlap as success.

    Receipt times are PTY drain times, not application admission or physical
    presentation. Empty/clipped/disjoint labels leave latency inconclusive; they
    must remain missing samples in the acceptance calculation. Stale receipts
    never establish a response to this input.
    """
    fresh = [
        (received, positions)
        for received, positions in observations
        if sent <= received <= until
    ]
    matched = next(
        (
            received
            for received, positions in fresh
            if albums_moved_up(before, positions)
        ),
        None,
    )
    if matched is not None:
        status = "matched"
    elif not fresh:
        status = "no_response_observed"
    elif not before or any(
        not before.keys() & positions.keys() for _, positions in fresh
    ):
        status = "inconclusive_overlap_lost"
    else:
        status = "no_upward_movement_observed"
    return {
        "status": status,
        "matched_received": matched,
        "seconds": matched - sent if matched is not None else None,
    }


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


class SettingsLoopback(http.server.ThreadingHTTPServer):
    """Explicit isolated connection fixture; never forwards to the live gateway."""

    daemon_threads = True

    def __init__(self, name, hold_artwork=False):
        from bmux_pty_acceptance import artwork_fixture

        super().__init__(("127.0.0.1", 0), SettingsLoopbackHandler)
        self.name = name
        self.hold_websocket = False
        self.requests = []
        self.stopping = threading.Event()
        self.art_requested = threading.Event()
        self.art_completed = threading.Event()
        self.art_release = threading.Event()
        if not hold_artwork:
            self.art_release.set()
        self.artwork = artwork_fixture(1 if name == "A" else 2, 128)

    def response(self, target):
        path = urllib.parse.urlsplit(target).path
        page = lambda items: {
            "items": items,
            "offset": 0,
            "limit": 100,
            "total": len(items),
            "hasMore": False,
        }
        if path == "/menu/albums":
            album = {
                "albumId": 1 if self.name == "A" else 2,
                "title": "Loopback " + self.name,
                "artist": "Artist " + self.name,
                "artistId": 1,
                "albumType": "LP",
                "dateReleased": None,
                "dateAdded": None,
                "containsCover": True,
                "blur": False,
                "versions": [],
                "albumSource": "Local",
                "apiSource": "Library",
                "artistSources": [],
                "albumSources": [],
            }
            query = urllib.parse.parse_qs(urllib.parse.urlsplit(target).query)
            return page([album] if int(query.get("offset", ["0"])[0]) == 0 else [])
        values = {
            "/health": {"status": "OK"},
            "/config/connection-id": "isolated-" + self.name,
            "/config/profiles": [{"name": "master"}],
            "/music-api": {"items": [], "offset": 0, "limit": 0, "total": 0},
            "/menu/artists": page([]),
            "/session/sessions": page([]),
            "/audio-zone/audio-zones": [],
            "/audio-zone/with-session": page([]),
            "/downloader/download-locations": {
                "items": [{"id": 1, "path": f"/fixture/{self.name}-download"}],
                "offset": 0,
                "limit": 1,
                "total": 1,
            },
            "/scan/scan-paths": [{"path": f"/fixture/{self.name}-scan"}],
        }
        if path not in values:
            raise ValueError("unexpected isolated endpoint " + path)
        return values[path]


class SettingsLoopbackHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        self.server.requests.append(
            {"method": "GET", "path": self.path, "monotonic": time.monotonic()}
        )
        if urllib.parse.urlsplit(self.path).path == "/ws":
            from bmux_pty_acceptance import Handler

            return Handler.do_GET(self)
        try:
            if re.fullmatch(r"/files/albums/[12]/[^?]+(?:\?.*)?", self.path):
                self.server.art_requested.set()
                if not self.server.art_release.wait(30):
                    self.send_error(504, "fixture artwork barrier expired")
                    return
                body, kind = self.server.artwork, "image/png"
            else:
                body = json.dumps(self.server.response(self.path)).encode()
                kind = "application/json"
            self.send_response(200)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            if kind == "image/png":
                self.server.art_completed.set()
        except (ValueError, KeyError):
            self.send_error(404)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def reject_mutation(self):
        self.server.requests.append(
            {"method": self.command, "path": self.path, "denied": True}
        )
        self.send_error(403, "fixture forbids playback, scans and server writes")

    do_POST = do_PATCH = do_DELETE = do_PUT = reject_mutation


def run_settings_loopback(executable, artifacts):
    """Actual Settings A->B UI and delayed-A artwork isolation; no live access.

    Requires the dedicated Kitty display. This is functional evidence, not a
    performance sample; restart/edit/error coverage remains a separate gap.
    """
    from bmux_visible_frames import FrameSampler

    assert os.isatty(0) and os.isatty(1)
    assert os.environ.get("BMUX_LIVE_ISOLATED_X11") == os.environ.get("DISPLAY")
    executable = Path(executable).resolve(strict=True)
    artifacts = Path(artifacts).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    a, b = SettingsLoopback("A", True), SettingsLoopback("B")
    for server in (a, b):
        threading.Thread(target=server.serve_forever, daemon=True).start()
    geometry = fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
    rows, columns, width, height = struct.unpack("HHHH", geometry)
    screen = LockedScreen(columns, rows)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, geometry)
    modes = termios.tcgetattr(0)
    tty.setraw(0)
    recording = bytearray()
    events, failures = [], []
    process = drain = sampler = None
    started = time.monotonic()

    def wait(predicate, label):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            if drain and drain.error:
                raise AssertionError(drain.error)
            if predicate():
                return
            time.sleep(0.01)
        raise AssertionError(label)

    def click(label):
        wait(lambda: screen.locate(label), "missing UI label " + label)
        x, y = screen.locate(label)
        os.write(master, f"\x1b[<0;{x+2};{y+1}M\x1b[<0;{x+2};{y+1}m".encode())
        events.append(
            {
                "kind": "click",
                "label": label,
                "coords": [x, y],
                "monotonic": time.monotonic(),
            }
        )

    def frame(name):
        (artifacts / (name + ".txt")).write_text(screen.text())
        capture_x11(artifacts / (name + ".png"))

    try:
        with tempfile.TemporaryDirectory(prefix="bmux-settings-loopback-") as home:
            connections = [
                {
                    "name": "Fixture " + server.name,
                    "api_url": f"http://127.0.0.1:{server.server_port}",
                }
                for server in (a, b)
            ]
            database = (
                Path(home) / ".local/moosicbox/app/profiles/master/persistence.db"
            )
            database.parent.mkdir(parents=True)
            with sqlite3.connect(database) as db:
                db.execute(
                    "CREATE TABLE state (key TEXT NOT NULL UNIQUE, value TEXT NOT NULL)"
                )
                for key, value in [
                    ("CONNECTION", connections[0]),
                    ("CONNECTIONS", connections),
                ]:
                    db.execute(
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
                RUST_LOG="hyperchad=debug,moosicbox_app_native=debug",
            )
            process = subprocess.Popen(
                [str(executable), "serve"],
                stdin=slave,
                stdout=slave,
                stderr=slave,
                cwd=home,
                env=env,
                start_new_session=True,
            )
            drain = PtyDrain(master, screen, recording, True)
            drain.start()
            click("Albums")
            wait(lambda: screen.locate("Loopback A"), "A actual library missing")
            wait(a.art_requested.is_set, "A delayed artwork request missing")
            frame("connection-a")
            click("Settings")
            wait(lambda: screen.locate("Connections"), "full Settings page missing")
            frame("settings-two-connections")
            # Exactly one saved non-selected connection: B. Never match Selected.
            b_image_start = len(screen.images)
            target = None
            for scroll_index in range(12):
                target = connection_select_target(
                    screen.text(),
                    connections[1]["name"],
                    connections[1]["api_url"],
                    rows - 4,
                )
                if target:
                    break
                before_scroll = screen.text()
                os.write(master, f"\x1b[<65;{columns*3//4};{rows//2}M".encode())
                time.sleep(0.2)
                events.append(
                    {
                        "kind": "settings_scroll",
                        "index": scroll_index,
                        "before": before_scroll,
                        "after": screen.text(),
                        "monotonic": time.monotonic(),
                    }
                )
            assert target, "B form exact [ Select ] not visible with name/API context"
            frame("settings-b-select-visible")
            target = connection_select_target(
                screen.text(),
                connections[1]["name"],
                connections[1]["api_url"],
                rows - 4,
            )
            assert target, "B target moved before click"
            x, y = target
            os.write(master, f"\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m".encode())
            events.append(
                {
                    "kind": "select_connection",
                    "name": connections[1]["name"],
                    "endpoint": connections[1]["api_url"],
                    "coords": target,
                    "monotonic": time.monotonic(),
                }
            )
            selected_at = events[-1]["monotonic"]
            wait(
                lambda: any(
                    "/ws" in entry.get("path", "")
                    and entry.get("monotonic", 0) >= selected_at
                    for entry in b.requests
                    if isinstance(entry, dict)
                ),
                "B websocket connection missing in selection epoch",
            )
            stable_text, stable_since = None, time.monotonic()

            def selected_page_settled():
                nonlocal stable_text, stable_since
                current = screen.text()
                if current != stable_text:
                    stable_text, stable_since = current, time.monotonic()
                return (
                    selected_connection_visible(
                        current,
                        connections[1]["name"],
                        connections[1]["api_url"],
                        rows - 4,
                    )
                    and time.monotonic() - stable_since >= 0.3
                )

            wait(selected_page_settled, "B selected Settings form did not settle")
            frame("settings-b-selected-settled")
            nav_target = exact_sidebar_target(screen.text(), "Albums", rows - 4)
            assert nav_target, "Albums sidebar link ambiguous or missing"
            x, y = nav_target
            os.write(master, f"\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m".encode())
            events.append(
                {
                    "kind": "click",
                    "label": "Albums",
                    "coords": nav_target,
                    "selection_epoch": selected_at,
                    "monotonic": time.monotonic(),
                }
            )
            wait(lambda: screen.locate("Loopback B"), "B actual library missing")
            wait(b.art_requested.is_set, "B artwork request missing")
            wait(b.art_completed.is_set, "B artwork response incomplete")
            wait(
                lambda: len(screen.images) > b_image_start,
                "B terminal artwork upload missing",
            )
            time.sleep(0.5)
            frame("connection-b-before-stale-a")
            sampler = FrameSampler(os.environ["DISPLAY"])
            # First cover interior from current protocol placement, not route-only evidence.
            with screen.lock:
                placements = current_cover_placements(list(screen.graphics))
            covers = [
                (params, x, y)
                for params, x, y in placements
                if params.get("a") in ("p", "T")
                and x >= 18
                and int(params.get("c", 0)) >= 4
            ]
            assert len(covers) == 1, "B cover placement not uniquely visible"
            params, x, y = covers[0]
            cw, ch = width / columns, height / rows
            rect = (
                round(x * cw) + 2,
                round(y * ch) + 2,
                round(int(params["c"]) * cw) - 4,
                min(round(int(params["c"]) * cw) - 4, round((rows - 3 - y) * ch) - 4),
            )
            assert rect[2] > 0 and rect[3] > 0
            _, _, before, stride = sampler.grab(rect)
            assert (
                len(set(before[::4])) > 16
            ), "B cover ROI empty/uniform; cannot prove stale-image isolation"
            a.art_release.set()
            wait(
                a.art_completed.is_set,
                "A delayed completion not delivered; stale-image test inconclusive",
            )
            time.sleep(0.7)
            _, _, after, after_stride = sampler.grab(rect)
            (artifacts / "b-cover-before.bgra").write_bytes(before)
            (artifacts / "b-cover-after-stale-a.bgra").write_bytes(after)
            assert (
                before == after and stride == after_stride
            ), "B cover changed after delayed A completion"
            assert screen.locate("Loopback B") and not screen.locate(
                "Loopback A"
            ), "stale A metadata visible"
            frame("connection-b-after-stale-a")
            with sqlite3.connect(database) as db:
                selected = json.loads(
                    db.execute(
                        "SELECT value FROM state WHERE key='CONNECTION'"
                    ).fetchone()[0]
                )
            assert (
                selected == connections[1]
            ), "isolated selected B connection was not persisted"
            events.append(
                {
                    "kind": "stale_a_rejected",
                    "rect": rect,
                    "stride": stride,
                    "selected": selected,
                    "monotonic": time.monotonic(),
                }
            )
    except Exception as error:
        failures.append(str(error))
        frame("failure")
    finally:
        a.art_release.set()
        if sampler:
            sampler.close()
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        if drain:
            drain.close()
        termios.tcsetattr(0, termios.TCSANOW, modes)
        os.close(master)
        os.close(slave)
        for server in (a, b):
            server.stopping.set()
            server.shutdown()
            server.server_close()
        (artifacts / "terminal.bin").write_bytes(recording)
        (artifacts / "requests.json").write_text(
            json.dumps({"A": a.requests, "B": b.requests}, indent=2)
        )
        result = {
            "result": "failed" if failures else "passed",
            "failures": failures,
            "events": events,
            "seconds": time.monotonic() - started,
            "sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            "geometry": [columns, rows, width, height],
            "limitations": [
                "Kitty/Xvfb not user-host proof",
                "no restart/edit/error workflow coverage",
                "exact B cover invariance, not timing acceptance",
            ],
        }
        (artifacts / "result.json").write_text(json.dumps(result, indent=2))
    return bool(failures)


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
    rows, columns, width_pixels, height_pixels = struct.unpack("HHHH", geometry)
    # Keep input inside the main content at the observed terminal geometry.
    # The old (90, 30) coordinate was outside an 80x24 terminal entirely.
    wheel_x, wheel_y = max(1, columns * 3 // 4), max(1, rows // 2)
    wheel_down = f"\x1b[<65;{wheel_x};{wheel_y}M".encode()
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
            albums_start = time.monotonic()
            click("Albums")
            frame("initial-shell")
            try:
                wait(
                    lambda: any(
                        image["received_seconds"] >= albums_start - started
                        for image in decoded
                    ),
                    "actual decoded real artwork after navigation",
                    15,
                )
            except AssertionError as error:
                failures.append(str(error))
            checks["decoded_real_artwork"] = bool(decoded)
            cover_receipt = min(
                (
                    image["received_seconds"] + started
                    for image in decoded
                    if image["received_seconds"] >= albums_start - started
                ),
                default=None,
            )
            cover_seconds = elapsed_receipt(cover_receipt, albums_start)
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
                    image["sha256_rgba"]: image["received_seconds"]
                    for image in decoded
                    if image["received_seconds"] >= albums_start - started
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
            hover_positions = album_positions(screen, list(gateway.payloads))
            frame("before-hover")
            for x, y in ((wheel_x, wheel_y), (columns - 2, 3), (2, 4)):
                os.write(master, f"\x1b[<35;{x};{y}M".encode())
                settle(0.15)
            frame("after-hover")
            check(
                "hover_album_positions",
                bool(hover_positions)
                and album_positions(screen, list(gateway.payloads)) == hover_positions,
                "No stable visible album label geometry across mouse movement; inspect screenshots",
            )
            if (
                kitty_screenshots
                and os.environ.get("BMUX_VISIBLE_FRAME_SAMPLES") == "1"
            ):
                from bmux_visible_frames import FrameSampler

                # Select first-column second-row card in compact terminal UI.
                # 12 virtual columns x 6 virtual rows of artwork, followed by
                # metadata: capture both, using observed host cell pixels.
                cw, ch = width_pixels / columns, height_pixels / rows
                # Lower portion of the first cover plus its title/artist rows:
                # fits even 80x24, and excludes the fixed footer on both scroll states.
                rect = (round(20 * cw), round(7 * ch), round(12 * cw), round(7 * ch))
                sampler = FrameSampler(os.environ["DISPLAY"])
                try:
                    telemetry_before = {
                        str(p): p.stat().st_size for p in Path(home).rglob("*.log")
                    }
                    result = sampler.translated_samples(
                        lambda direction: os.write(
                            master, f"\x1b[<{direction};{wheel_x};{wheel_y}M".encode()
                        ),
                        rect,
                        round(3 * ch),
                        artifacts,
                        sequential_down=os.environ.get("BMUX_VISIBLE_SEQUENTIAL_DOWN")
                        == "1",
                    )
                    telemetry = ""
                    for path in Path(home).rglob("*.log"):
                        with path.open("rb") as log:
                            log.seek(telemetry_before.get(str(path), 0))
                            telemetry += log.read().decode(errors="replace")
                    (artifacts / "visible-window-app.log").write_text(telemetry)
                    result["scroll_reuse_records"] = telemetry.count(
                        "bmux prepare scroll-reuse"
                    )
                    result["telemetry_scope"] = (
                        "Log byte offsets immediately bracketing exact 16 visible samples"
                    )
                    (artifacts / "visible-performance.json").write_text(
                        json.dumps(result, indent=2)
                    )
                    event("visible_frames", **result)
                    check(
                        "visible_input_p95",
                        result["completed"] == 16
                        and result["p95_upper_seconds"] is not None
                        and result["p95_upper_seconds"] <= 0.1,
                        "Visible pixel samples missing or p95 exceeds 100ms",
                    )
                finally:
                    sampler.close()
            wait(
                lambda: bool(album_positions(screen, list(gateway.payloads))),
                "real album labels after asynchronous list load",
                12,
            )
            positions_before = album_positions(screen, list(gateway.payloads))
            # Label stability is not proof of host graphics stability.
            assert (
                positions_before
            ), "No unambiguous album labels to measure wheel movement"
            scroll_start = time.monotonic()
            os.write(master, wheel_down)
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
                log_before = {
                    str(p): p.stat().st_size for p in Path(home).rglob("*.log")
                }
                # Drain old frames before taking the baseline. Filesystem work
                # stays before this settle/baseline/input sequence.
                before = settled_album_positions(settle, screen, list(gateway.payloads))
                anchor = clock_anchor()
                sent_wall = time.time()
                sent = time.monotonic()
                os.write(master, wheel_down)
                write_completed = time.monotonic()
                until = min(deadline, sent + 1)
                while time.monotonic() < until:
                    pump()
                    observation = classify_wheel_observations(
                        before, list(observations), sent, until
                    )
                    if observation["matched_received"] is not None:
                        break
                fresh = [
                    (received, positions)
                    for received, positions in list(observations)
                    if sent <= received <= until
                ]
                observation = classify_wheel_observations(before, fresh, sent, until)
                samples.append(observation["seconds"])
                log_after = {
                    str(p): p.stat().st_size for p in Path(home).rglob("*.log")
                }
                event(
                    "wheel_sample",
                    index=sample_index,
                    input_id=f"pty-wheel-{sample_index}",
                    clock_anchor=anchor,
                    **observation,
                    input_monotonic=sent,
                    input_seconds=sent - started,
                    input_wall_before_monotonic=sent_wall,
                    write_completed_monotonic=write_completed,
                    matched_received_seconds=(
                        observation["matched_received"] - started
                        if observation["matched_received"] is not None
                        else None
                    ),
                    observations=[
                        {"received_seconds": received - started, "positions": positions}
                        for received, positions in fresh
                    ],
                    log_byte_ranges={
                        path: [log_before.get(path, 0), size]
                        for path, size in log_after.items()
                    },
                    metric="input write start to matching PTY drain receipt; not admission/presentation",
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
            settle(0.3)
            wait(
                lambda: not decode_jobs,
                "decoded current covers before cover-center click",
                5,
            )
            frame("scrolled-samples")
            with screen.lock:
                current_placements = current_cover_placements(list(screen.graphics))
            card_columns = [
                (x, int(params.get("c", 0)))
                for params, x, y in current_placements
                if params.get("a") in ("p", "T")
                and x >= 18
                and int(params.get("c", 0)) >= 4
                and 0 <= y < rows - 3
            ]
            candidates = album_click_candidates(
                screen, list(gateway.payloads), card_columns
            )
            assert (
                candidates
            ), "No unambiguous full title/artist identity visible for safe click"
            _, album_clicked, title_position = candidates[0]
            preserved = screen.text()

            def current_display_positions():
                columns_now = [
                    (x, int(params.get("c", 0)))
                    for params, x, y in current_cover_placements(list(screen.graphics))
                    if params.get("a") in ("p", "T")
                    and x >= 18
                    and int(params.get("c", 0)) >= 4
                    and 0 <= y < rows - 3
                ]
                return displayed_album_positions(
                    screen, list(gateway.payloads), columns_now
                )

            preserved_positions = current_display_positions()
            event("list_checkpoint", positions=preserved_positions)
            tx, ty = title_position
            artwork = {image["image_id"]: image for image in decoded}
            artwork_ids = set(artwork)

            def placement_rows(params):
                if params.get("r"):
                    return int(params["r"])
                image = artwork.get(params.get("i"))
                if not image or not width_pixels or not height_pixels:
                    return 0
                # Kitty infers omitted rows from source aspect ratio, specified
                # columns and the ACTUAL host cell pixels, not virtual CSS metrics.
                return math.ceil(
                    int(params.get("c", 0))
                    * (width_pixels / columns)
                    * image["height"]
                    / image["width"]
                    / (height_pixels / rows)
                )

            with screen.lock:
                placements = current_cover_placements(list(screen.graphics))
            covers = [
                (params, x, y)
                for params, x, y in placements
                if params.get("i") in artwork_ids
                and params.get("a") in ("p", "T")
                and x <= tx < x + int(params.get("c", 0))
                and 0 <= ty - (y + placement_rows(params)) <= 1
                and placement_rows(params) > 0
                and 0 <= x < columns
                and 0 <= y < rows - 3
            ]
            assert (
                len(covers) == 1
            ), "No unique CURRENT decoded cover aligned with full album label"
            params, x, y = covers[0]
            cx = x + int(params["c"]) // 2 + 1
            cy = y + placement_rows(params) // 2 + 1
            assert 1 <= cx <= columns and 1 <= cy <= rows
            event(
                "cover_click_target",
                album_id=str(album_clicked["albumId"]),
                title=album_clicked["title"],
                artist=album_clicked["artist"],
                label_coords=title_position,
                displayed_title=screen.text()
                .splitlines()[ty][tx : tx + int(params["c"])]
                .rstrip(),
                displayed_artist=screen.text()
                .splitlines()[ty + 1][tx : tx + int(params["c"])]
                .rstrip(),
                placement=params,
                placement_coords=[x, y],
                click_coords=[cx, cy],
                expected_route=f"/menu/album?albumId={album_clicked['albumId']}",
            )
            click_started = time.monotonic()
            back_sampler = None
            if (
                kitty_screenshots
                and os.environ.get("BMUX_VISIBLE_FRAME_SAMPLES") == "1"
            ):
                from bmux_visible_frames import FrameSampler

                back_sampler = FrameSampler(os.environ["DISPLAY"])
                # Matched cover plus its unique API title/metadata, bounded to
                # the actually visible content above the footer.
                back_rect = (
                    round(x * width_pixels / columns),
                    round(y * height_pixels / rows),
                    round(int(params["c"]) * width_pixels / columns),
                    round(
                        min(placement_rows(params) + 3, rows - 4 - y)
                        * height_pixels
                        / rows
                    ),
                )
                _, _, back_pixels, back_stride = back_sampler.grab(back_rect)
            os.write(master, f"\x1b[<0;{cx};{cy}M\x1b[<0;{cx};{cy}m".encode())
            event(
                "cover_center_click",
                album_id=str(album_clicked["albumId"]),
                x=cx,
                y=cy,
                placement=params,
            )
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
                        event(
                            "detail_route_observed",
                            expected_album_id=selected_id,
                            route=path,
                            observed_track_ids=sorted(tracks_in(payload)),
                        )
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
                detail_metadata_matches(
                    screen.text(), album_clicked["title"], album_clicked["artist"]
                ),
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
                os.write(master, wheel_down)
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
            if back_sampler is not None:
                try:
                    event(
                        "visible_back",
                        **back_sampler.cached_back(
                            lambda: os.write(master, b"\x1b[1;3D"),
                            back_rect,
                            back_pixels,
                            back_stride,
                            artifacts,
                        ),
                    )
                finally:
                    back_sampler.close()
            else:
                os.write(master, b"\x1b[1;3D")
            restored = False
            try:
                wait(
                    lambda: bool(preserved_positions)
                    and current_display_positions() == preserved_positions,
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
                actual_positions=current_display_positions(),
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
            "Input timings are PTY label observations, not actual visible compositor frames",
            "Navigation uses decoded artwork placement matched to a unique API album title",
            "Hover injects SGR motion; inspect screenshots independently of label checks",
            "Six-cover expectation is geometry-specific; small viewports may fail it",
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
                    "queue_telemetry": drain.telemetry.snapshot(),
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
    parser.add_argument("--settings-loopback", action="store_true")
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
    elif arguments.settings_loopback:
        if not arguments.kitty_screenshots:
            parser.error(
                "--settings-loopback requires --kitty-screenshots dedicated display"
            )
        raise SystemExit(
            run_settings_loopback(arguments.executable, arguments.artifacts)
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
