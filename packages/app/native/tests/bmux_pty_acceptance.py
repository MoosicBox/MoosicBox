#!/usr/bin/env python3
"""Actual MoosicBox executable acceptance; stdlib only, never builds.

Requires an explicit previously built --no-default-features --features bmux binary.
Uses fresh HOME/XDG roots, a loopback-only mock, and a terminal screen model.
Failures retain the recording/screen/mock requests in --artifacts, not user state.
"""
import argparse
import base64
import errno
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time
import zlib


def png_fixture():
    """Opaque red/green/blue/white 2x2 artwork, checked after protocol decoding."""

    def chunk(kind, data):
        return (
            struct.pack("!I", len(data))
            + kind
            + data
            + struct.pack("!I", zlib.crc32(kind + data))
        )

    pixels = b"\xff\0\0\xff\0\xff\0\xff\0\0\xff\xff\xff\xff\xff\xff"
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack("!2I5B", 2, 2, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"\0" + pixels[:8] + b"\0" + pixels[8:]))
        + chunk(b"IEND", b"")
    )


def decode_png(encoded):
    """Decode bounded 8-bit RGB/RGBA noninterlaced PNG protocol artifacts."""
    assert encoded[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed, width, height, channels = 8, bytearray(), 0, 0, 0
    while offset + 12 <= len(encoded):
        size = int.from_bytes(encoded[offset : offset + 4], "big")
        kind = encoded[offset + 4 : offset + 8]
        data = encoded[offset + 8 : offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack("!2I5B", data)
            assert depth == 8 and color in (2, 6) and interlace == 0
            channels = 4 if color == 6 else 3
            assert 0 < width <= 4096 and 0 < height <= 4096
        elif kind == b"IDAT":
            compressed.extend(data)
        offset += size + 12
    stride = width * channels
    raw = zlib.decompress(compressed)
    assert len(raw) == (stride + 1) * height
    previous, pixels = bytearray(stride), bytearray()
    for y in range(height):
        method = raw[y * (stride + 1)]
        row = bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for i in range(stride):
            a = row[i - channels] if i >= channels else 0
            b = previous[i]
            c = previous[i - channels] if i >= channels else 0
            if method == 1:
                predictor = a
            elif method == 2:
                predictor = b
            elif method == 3:
                predictor = (a + b) // 2
            elif method == 4:
                p = a + b - c
                distances = (abs(p - a), abs(p - b), abs(p - c))
                predictor = (a, b, c)[distances.index(min(distances))]
            else:
                assert method == 0
                predictor = 0
            row[i] = (row[i] + predictor) & 255
        for i in range(0, stride, channels):
            pixels.extend(row[i : i + 3])
            pixels.append(row[i + 3] if channels == 4 else 255)
        previous = row
    return width, height, bytes(pixels)


def decode_sixel(body):
    """Decode emitted fixed-palette Sixel into inspectable RGBA pixels."""
    raster = re.search(rb'"\d+;\d+;(\d+);(\d+)', body)
    assert raster, "missing Sixel raster dimensions"
    width, height = map(int, raster.groups())
    assert 0 < width <= 4096 and 0 < height <= 4096
    pixels = bytearray(width * height * 4)
    palette, color, x, y, i = {}, (0, 0, 0, 255), 0, 0, 0
    while i < len(body):
        byte = body[i]
        if byte in (ord("#"), ord('"')):
            match = re.match(rb"[\d;]+", body[i + 1 :])
            assert match
            values = list(map(int, match[0].split(b";")))
            if byte == ord("#"):
                if len(values) == 5:
                    assert values[1] == 2
                    palette[values[0]] = tuple(
                        round(c * 255 / 100) for c in values[2:]
                    ) + (255,)
                color = palette.get(values[0], color)
            i += len(match[0]) + 1
            continue
        repeat = 1
        if byte == ord("!"):
            match = re.match(rb"\d+", body[i + 1 :])
            assert match
            repeat = int(match[0])
            assert repeat <= 4096
            i += len(match[0]) + 1
            byte = body[i]
        if 63 <= byte <= 126:
            bits = byte - 63
            for column in range(x, min(width, x + repeat)):
                for bit in range(6):
                    if bits & (1 << bit) and y + bit < height:
                        start = ((y + bit) * width + column) * 4
                        pixels[start : start + 4] = bytes(color)
            x += repeat
        elif byte == ord("$"):
            x = 0
        elif byte == ord("-"):
            x, y = 0, y + 6
        i += 1
    return width, height, bytes(pixels)


class Screen:
    """Bounded VT cell model plus decoded Kitty/Sixel/iTerm2 artifacts.

    Images are decoded into pixels, not counted from escape-marker presence.
    SGR is irrelevant to label hit coordinates; geometry is checked separately.
    """

    def __init__(self, width=140, height=60):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0
        self.pending = b""
        self.saved = (0, 0)
        self.transfers = {}
        self.images = []
        self.graphics = []
        self.other_images = []

    def feed(self, data):
        self.pending += data
        while self.pending:
            data = self.pending
            if data.startswith(b"\x1b_G"):
                end = data.find(b"\x1b\\", 3)
                if end < 0:
                    return
                header, _, payload = data[3:end].partition(b";")
                params = dict(
                    part.split("=", 1)
                    for part in header.decode().split(",")
                    if "=" in part
                )
                self.graphics.append((dict(params), self.x, self.y))
                identity = params.get("i", "_")
                # Chunk continuations may omit identity and metadata.
                if not params.get("i") and self.transfers:
                    identity = next(reversed(self.transfers))
                old, encoded, position = self.transfers.get(
                    identity, (params, b"", (self.x, self.y))
                )
                old.update(params)
                encoded += base64.b64decode(payload)
                if params.get("m") == "1":
                    self.transfers[identity] = (old, encoded, position)
                else:
                    self.transfers.pop(identity, None)
                    if encoded and old.get("a", "t") in ("t", "T"):
                        if old.get("o") == "z":
                            encoded = zlib.decompress(encoded)
                        self.images.append((old, encoded, position))
                self.pending = data[end + 2 :]
            elif data.startswith((b"\x1b]", b"\x1bP", b"\x1b_")):
                end = data.find(b"\x1b\\", 2)
                bell = data.find(b"\x07", 2)
                if bell >= 0 and (end < 0 or bell < end):
                    end, terminator = bell, 1
                elif end >= 0:
                    terminator = 2
                else:
                    return
                body = data[2:end]
                if data.startswith(b"\x1b]1337;File="):
                    params, _, encoded = body[len(b"1337;File=") :].partition(b":")
                    dimensions = dict(
                        part.split("=", 1)
                        for part in params.decode().split(";")
                        if "=" in part
                    )
                    self.other_images.append(
                        (
                            "iterm2",
                            decode_png(base64.b64decode(encoded)),
                            (self.x, self.y),
                            dimensions,
                        )
                    )
                elif data.startswith(b"\x1bP") and b"q" in body:
                    self.other_images.append(
                        (
                            "sixel",
                            decode_sixel(body.split(b"q", 1)[1]),
                            (self.x, self.y),
                            {},
                        )
                    )
                self.pending = data[end + terminator :]
            elif data.startswith(b"\x1b["):
                match = re.match(rb"\x1b\[([0-?]*)([ -/]*)([@-~])", data)
                if not match:
                    return
                args, _, final = match.groups()
                values = [
                    int(value or b"0") if (value or b"0").isdigit() else 0
                    for value in args.lstrip(b"?><=").split(b";")
                ]
                n = values[0] or 1
                if final in (b"H", b"f"):
                    self.y = (values[0] or 1) - 1
                    self.x = (values[1] if len(values) > 1 else 1) - 1
                elif final == b"G":
                    self.x = n - 1
                elif final == b"A":
                    self.y -= n
                elif final == b"B":
                    self.y += n
                elif final == b"C":
                    self.x += n
                elif final == b"D":
                    self.x -= n
                elif final == b"J" and values[0] in (2, 3):
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                elif final == b"K" and 0 <= self.y < self.height:
                    start, end = (
                        (0, self.width)
                        if values[0] == 2
                        else (
                            (0, self.x + 1) if values[0] == 1 else (self.x, self.width)
                        )
                    )
                    for x in range(max(0, start), min(self.width, end)):
                        self.cells[self.y][x] = " "
                elif final == b"s":
                    self.saved = (self.x, self.y)
                elif final == b"u":
                    self.x, self.y = self.saved
                self.pending = data[match.end() :]
            elif data.startswith(b"\x1b"):
                if len(data) < 2:
                    return
                if data[1:2] == b"7":
                    self.saved = (self.x, self.y)
                elif data[1:2] == b"8":
                    self.x, self.y = self.saved
                self.pending = data[2:]
            else:
                byte = data[0]
                if byte == 13:
                    self.x = 0
                elif byte == 10:
                    self.y += 1
                elif byte == 8:
                    self.x = max(0, self.x - 1)
                elif byte >= 32:
                    length = (
                        1
                        if byte < 128
                        else (2 if byte < 224 else (3 if byte < 240 else 4))
                    )
                    if len(data) < length:
                        return
                    character = data[:length].decode("utf-8", errors="replace")
                    if 0 <= self.y < self.height and 0 <= self.x < self.width:
                        self.cells[self.y][self.x] = character
                    self.x += 1
                    self.pending = data[length:]
                    continue
                self.pending = data[1:]

    def text(self):
        return "\n".join("".join(row) for row in self.cells)

    def locate(self, label, occurrence=0):
        matches = []
        for y, row in enumerate(self.cells):
            line = "".join(row)
            start = 0
            while (x := line.find(label, start)) >= 0:
                matches.append((x, y))
                start = x + len(label)
        return matches[occurrence] if len(matches) > occurrence else None


class Mock(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.requests = []
        self.stopping = threading.Event()


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_GET(self):
        self.server.requests.append(self.path)
        if (
            self.path.split("?")[0] == "/ws"
            and self.headers.get("Upgrade", "").lower() == "websocket"
        ):
            key = self.headers["Sec-WebSocket-Key"]
            accept = base64.b64encode(
                hashlib.sha1(
                    (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
                ).digest()
            ).decode()
            self.send_response(101)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.end_headers()
            # An upgraded loopback socket establishes Ready. The test does not
            # impersonate a player or promise playback/server push coverage.
            self.connection.settimeout(0.2)
            while not self.server.stopping.is_set():
                try:
                    if not self.connection.recv(65536):
                        break
                except TimeoutError:
                    continue
                except OSError:
                    break
            return
        if "/files/" in self.path:
            body, content_type = png_fixture(), "image/png"
        else:
            path = self.path.split("?")[0]
            # Explicit minimal connection metadata. Unexpected endpoints fail;
            # never hide missing mock coverage behind a universal success object.
            page = lambda items: {
                "items": items,
                "offset": 0,
                "limit": 30,
                "total": len(items),
                "hasMore": False,
            }
            album = {
                "albumId": 1,
                "title": "Acceptance Quadrants",
                "artist": "Loopback Artist",
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
            objects = {
                "/health": {"status": "OK"},
                "/config/connection-id": "bmux-acceptance",
                "/config/profiles": ["master"],
                "/music-api": page([]),
                "/menu/albums": page([album]),
                "/menu/artists": page([]),
                "/session/sessions": page([]),
                "/audio-zone/audio-zones": [],
                "/audio-zone/with-session": page([]),
                "/downloader/download-locations": page([]),
                "/scan/scan-paths": [],
            }
            if path not in objects:
                self.send_error(404, "unmocked endpoint")
                return
            body, content_type = json.dumps(objects[path]).encode(), "application/json"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()
        self.server.requests.append("completed:" + self.path)
        self.close_connection = True


def run(args):
    executable = Path(args.executable).resolve(strict=True)
    assert executable.is_file() and os.access(executable, os.X_OK)
    artifacts = Path(args.artifacts).resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    with executable.open("rb") as file:
        digest = hashlib.file_digest(file, "sha256").hexdigest()
    metadata = {
        "executable": str(executable),
        "sha256": digest,
        "protocol": args.protocol,
    }
    print(json.dumps(metadata), flush=True)
    mock = Mock()
    thread = threading.Thread(target=mock.serve_forever, daemon=True)
    thread.start()
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 60, 140, 1120, 960))
    original = termios.tcgetattr(slave)
    screen, recording = Screen(), bytearray()
    process = None
    with tempfile.TemporaryDirectory(prefix="moosicbox-bmux-acceptance-") as home:
        # Do not inherit app/server/auth/provider configuration or host graphics hints.
        env = {
            key: value
            for key, value in os.environ.items()
            if key in ("PATH", "LANG", "LC_ALL", "LD_LIBRARY_PATH")
        }
        env.update(
            HOME=home,
            XDG_CONFIG_HOME=home + "/config",
            XDG_DATA_HOME=home + "/data",
            XDG_CACHE_HOME=home + "/cache",
            XDG_STATE_HOME=home + "/state",
            TERM="xterm-256color",
            BMUX_PANE="native-app-acceptance",
            BMUX_IMAGE_PROTOCOL="" if args.protocol == "text" else args.protocol,
            RUST_LOG="moosicbox_app_native=debug,moosicbox_app_state=debug,moosicbox_app_ws=debug,hyperchad=debug",
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

            def pump():
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        data = os.read(master, 262144)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            data = b""
                        else:
                            raise
                    recording.extend(data)
                    screen.feed(data)
                assert (
                    process.poll() is None
                ), f"app exited early ({process.returncode})"

            def wait(predicate, description, timeout=15):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    pump()
                    if predicate():
                        return
                raise AssertionError(description + "\n" + screen.text())

            def send(data):
                os.write(master, data)
                for _ in range(3):
                    pump()

            def click(label, occurrence=0, offset=0):
                wait(
                    lambda: screen.locate(label, occurrence),
                    "missing hit target: " + label,
                )
                x, y = screen.locate(label, occurrence)
                x += offset
                send(f"\x1b[<0;{x + 1};{y + 1}M".encode())
                send(f"\x1b[<0;{x + 1};{y + 1}m".encode())

            wait(
                lambda: "Configure a server connection in Settings" in screen.text(),
                "real home not painted",
            )
            # First Tab is the home logo; second is the image-only Settings link.
            send(b"\t\t\r")
            wait(
                lambda: "New Connection" in screen.text(),
                "Settings keyboard navigation did not paint destination",
            )
            click("New Connection", offset=2)
            wait(lambda: "API URL:" in screen.text(), "native POST fragment failed")
            # The right-aligned label and editable input wrap onto separate rows.
            # Clicking after the label would be outside the viewport. Mouse-focus
            # the rendered default connection name, then Tab to the API URL input.
            click("New connection", offset=2)
            send(b"\t")
            endpoint = f"http://127.0.0.1:{mock.server_port}"
            send(endpoint.encode())
            # Save belonging to the connection form (not connection-name form).
            wait(
                lambda: endpoint in screen.text(),
                "edited connection URL missing before submit",
            )
            click("Save", occurrence=1, offset=1)
            # Fragment reload must retain the edited URL, not stale input state.
            wait(lambda: endpoint in screen.text(), "saved connection URL missing")
            # A first connection may be auto-selected by the real app. Only click
            # an actual Select button, never the '(Selected)' status substring.
            if screen.locate("[ Select ]"):
                click("[ Select ]", offset=3)
            wait(
                lambda: "(Selected)" in screen.text(),
                "connection selection did not return a visible fragment",
            )
            wait(
                lambda: any(path.startswith("/ws") for path in mock.requests),
                "selected connection never contacted isolated mock",
            )
            # Navigate the actual library route and require its resource fetch.
            # Focus/reveal may scroll the shared root horizontally while editing
            # right-aligned forms. Keyboard traversal must recover navigation.
            for _ in range(40):
                if screen.locate("Albums"):
                    break
                send(b"\x1b[Z")
            click("Albums", offset=1)
            wait(
                lambda: "Acceptance Quadrants" in screen.text(),
                "actual albums route did not paint mock album",
            )
            wait(
                lambda: any("/files/albums/1/" in path for path in mock.requests),
                "actual album view did not fetch artwork",
            )
            # A live resize must retain content and accept later input.
            fcntl.ioctl(
                slave, termios.TIOCSWINSZ, struct.pack("HHHH", 60, 138, 1104, 960)
            )
            os.killpg(process.pid, signal.SIGWINCH)
            wait(lambda: "Acceptance Quadrants" in screen.text(), "resize lost album")
            if args.protocol == "kitty":
                expected = bytes(
                    [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
                )

                def artwork_pixels(parameters, pixels):
                    width, height = int(parameters.get("s", 0)), int(
                        parameters.get("v", 0)
                    )
                    if width < 2 or height < 2 or len(pixels) != width * height * 4:
                        return False
                    # The presenter resamples into cell-sized, padded textures;
                    # sample inside each solid quadrant, away from filter edges.
                    samples = [
                        (width // 8, height // 8),
                        (7 * width // 8, height // 8),
                        (width // 8, 7 * height // 8),
                        (7 * width // 8, 7 * height // 8),
                    ]
                    return (
                        b"".join(
                            pixels[(y * width + x) * 4 : (y * width + x) * 4 + 4]
                            for x, y in samples
                        )
                        == expected
                    )

                wait(
                    lambda: any(
                        p.get("f") == "32" and artwork_pixels(p, b)
                        for p, b, _ in screen.images
                    ),
                    "actual mock artwork quadrant pixels absent",
                )
                rgba = [
                    (p, b, pos) for p, b, pos in screen.images if p.get("f") == "32"
                ]
                assert rgba, "no inspectable RGBA image payloads"
                assert any(
                    any(b[i + 3] > 0 for i in range(0, len(b) - 3, 4))
                    for _, b, _ in rgba
                ), "all icon payloads transparent"
                expected = bytes(
                    [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
                )
                assert any(
                    artwork_pixels(p, b) for p, b, _ in rgba
                ), "actual mock artwork quadrant pixels absent"
                placements = [
                    (p, x, y) for p, x, y in screen.graphics if p.get("a") == "p"
                ]
                assert placements, "image payloads have no protocol placements"
                for p, x, y in placements:
                    assert (
                        0 <= x < 140 and 0 <= y < 60
                    ), f"off-screen image placement: {p} at {x},{y}"
                    assert (
                        int(p.get("c", "1")) > 0 and int(p.get("r", "1")) > 0
                    ), f"empty placement: {p}"
                white_ids = {
                    p.get("i")
                    for p, b, _ in rgba
                    if any(
                        b[i] > 200
                        and b[i + 1] > 200
                        and b[i + 2] > 200
                        and b[i + 3] > 200
                        for i in range(0, len(b) - 3, 4)
                    )
                }
                assert any(
                    p.get("i") in white_ids and x < 36 and y < 10
                    for p, x, y in placements
                ), "white sidebar icon not painted at sidebar position"
            elif args.protocol in ("sixel", "iterm2"):
                wait(lambda: bool(screen.other_images), "no decodable protocol images")
                expected_colors = {
                    (255, 0, 0, 255),
                    (0, 255, 0, 255),
                    (0, 0, 255, 255),
                    (255, 255, 255, 255),
                }
                wait(
                    lambda: any(
                        protocol == args.protocol
                        and expected_colors <= set(zip(*[iter(image[2])] * 4))
                        for protocol, image, _, _ in screen.other_images
                    ),
                    "mock artwork colors absent from decoded protocol pixels",
                )
                decoded = [
                    (image, position)
                    for protocol, image, position, _ in screen.other_images
                    if protocol == args.protocol
                ]
                assert decoded, "wrong terminal protocol selected"
                expected_colors = {
                    (255, 0, 0, 255),
                    (0, 255, 0, 255),
                    (0, 0, 255, 255),
                    (255, 255, 255, 255),
                }
                assert any(
                    expected_colors <= set(zip(*[iter(image[2])] * 4))
                    for image, _ in decoded
                ), "mock artwork colors absent from decoded protocol pixels"
                assert any(
                    position[0] < 36 and position[1] < 10 for _, position in decoded
                ), "sidebar image placement missing"
                assert (
                    not screen.images
                ), "unexpected Kitty transmission in another protocol"
            else:
                assert (
                    not screen.images and not screen.other_images
                ), "text terminal unexpectedly received graphics"
            send(b"\x11")
            deadline = time.monotonic() + 10
            while process.poll() is None and time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        recording.extend(os.read(master, 262144))
                    except OSError:
                        break
            assert process.wait(timeout=2) == 0, "unclean app shutdown"
            assert termios.tcgetattr(slave) == original, "terminal modes not restored"
            assert b"\x1b[?1049l" in recording, "alternate screen not restored"
            metadata["result"] = "passed"
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            (artifacts / "terminal.bin").write_bytes(recording)
            (artifacts / "screen.txt").write_text(screen.text())
            (artifacts / "requests.json").write_text(
                json.dumps(mock.requests, indent=2)
            )
            if isinstance(metadata.get("result"), str):
                metadata["checks"] = [
                    "home",
                    "keyboard-settings-link",
                    "mouse-native-forms",
                    "connection-selection",
                    "actual-albums-route",
                    "loopback-artwork",
                    "protocol-pixels",
                    "resize",
                    "terminal-restoration",
                ]
            (artifacts / "artifact.json").write_text(json.dumps(metadata, indent=2))
            for file in Path(home).rglob("*.log"):
                # Isolated app logs only; no host configuration is read.
                (artifacts / ("app-" + file.name)).write_bytes(file.read_bytes())
            os.close(master)
            os.close(slave)
            mock.stopping.set()
            mock.shutdown()
            mock.server_close()
            thread.join(timeout=2)
    print(
        "PASS actual app navigation/forms/selection/resize/shutdown; inspect artifacts for graphics",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument(
        "--protocol", choices=("text", "kitty", "sixel", "iterm2"), default="kitty"
    )
    parser.add_argument("--artifacts", required=True)
    run(parser.parse_args())


if __name__ == "__main__":
    main()
