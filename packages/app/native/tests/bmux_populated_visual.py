#!/usr/bin/env python3
"""Isolated loopback populated-session compositor diagnostic; never live proof.

Run inside Kitty on explicitly marked isolated Xvfb. No playback is requested.
The fixture sends server-side session snapshots at positions 0/15/39 and saves
actual framebuffer screenshots plus PTY output, HTTP paths and fixture JSON.
"""
import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import threading
import time
import tty
from bmux_pty_acceptance import Mock, Handler, Screen, artwork_fixture
from bmux_visible_frames import FrameSampler
from bmux_live_acceptance import capture_x11, LockedScreen, PtyDrain


def footer_artwork_pixels(raw):
    """Recognize the fixture's three saturated quadrants, not text/placement alone."""
    counts = [0, 0, 0]
    for offset in range(0, len(raw), 4):
        blue, green, red = raw[offset : offset + 3]
        for index, (primary, second, third) in enumerate(
            ((red, green, blue), (green, red, blue), (blue, red, green))
        ):
            counts[index] += primary > 220 and second < 35 and third < 35
    return dict(
        red=counts[0],
        green=counts[1],
        blue=counts[2],
        matched=all(count >= 8 for count in counts),
    )


def session_fixture(position):
    tracks = [
        dict(
            trackId=i,
            number=i + 1,
            title=f"Long current playlist track title {i} — Deluxe Remastered Sessions",
            duration=240.0,
            album="A complete remastered album with bonus tracks",
            albumId=i,
            albumType="LP",
            dateReleased=None,
            dateAdded=None,
            artist="An artist with a realistic long display name",
            artistId=1,
            containsCover=True,
            blur=False,
            format=None,
            bitDepth=None,
            audioBitrate=None,
            overallBitrate=None,
            sampleRate=None,
            channels=None,
            trackSource="LOCAL",
            apiSource="Library",
            sources=[],
        )
        for i in range(40)
    ]
    return dict(
        sessionId=1,
        name="Fixture session",
        active=True,
        playing=False,
        position=position,
        seek=42.0,
        volume=0.7,
        playbackTarget=None,
        playlist=dict(sessionPlaylistId=1, tracks=tracks),
    )


class PopulatedHandler(Handler):
    def do_GET(self):
        receipt = time.monotonic()
        try:
            return self.handle_get()
        finally:
            try:
                self.wfile.flush()
                self.server.clock_events.append(
                    dict(
                        kind="http_flushed", path=self.path, monotonic=time.monotonic()
                    )
                )
            except OSError:
                pass
            self.server.clock_events.append(
                dict(
                    kind="http_handler_finished",
                    path=self.path,
                    receipt=receipt,
                    finished=time.monotonic(),
                )
            )

    def handle_get(self):
        if self.path.split("?")[0] == "/session/sessions":
            body = json.dumps(
                dict(
                    items=[session_fixture(self.server.initial_position)],
                    offset=0,
                    limit=1,
                    total=1,
                )
            ).encode()
            self.server.requests.append(self.path)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path.split("?")[0] != "/ws":
            return super().do_GET()
        self.server.requests.append(self.path)
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

        def send(value):
            body = json.dumps(value).encode()
            header = (
                bytes([0x81, len(body)])
                if len(body) < 126
                else b"\x81\x7e" + struct.pack("!H", len(body))
            )
            self.connection.sendall(header + body)

        try:
            time.sleep(1)
            send(dict(type="CONNECTION_ID", connectionId="visual-fixture"))
            for position in (self.server.initial_position,):
                send(dict(type="SESSIONS", payload=[session_fixture(position)]))
                self.server.fixture_position = position
                self.server.fixture_sent_at = time.monotonic()
                self.server.clock_events.append(
                    dict(
                        kind="fixture_sent",
                        position=position,
                        monotonic=self.server.fixture_sent_at,
                        wall=time.time(),
                    )
                )
                time.sleep(7)
            self.server.stopping.wait(10)
        except OSError:
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--artifacts", required=True)
    parser.add_argument("--position", type=int, choices=(0, 15, 39), default=0)
    args = parser.parse_args()
    assert (
        os.environ.get("DISPLAY") == os.environ.get("BMUX_LIVE_ISOLATED_X11") == ":97"
    )
    assert os.isatty(0) and os.isatty(1)
    artifacts = Path(args.artifacts).resolve()
    artifacts.mkdir(exist_ok=False)
    executable = str(Path(args.executable).resolve(strict=True))
    server = Mock()
    server.clock_events = []
    server.RequestHandlerClass = PopulatedHandler
    server.artwork_size = 128
    server.artworks = (artwork_fixture(1, 128),)
    server.album_count = 40
    server.fixture_position = None
    server.initial_position = args.position
    threading.Thread(target=server.serve_forever, daemon=True).start()
    modes = termios.tcgetattr(0)
    geometry = fcntl.ioctl(0, termios.TIOCGWINSZ, bytes(8))
    rows, columns, _, _ = struct.unpack("HHHH", geometry)
    screen = LockedScreen(columns, rows)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, geometry)
    recording = bytearray()
    captured = []
    with tempfile.TemporaryDirectory(prefix="bmux-populated-") as home:
        database = Path(home) / ".local/moosicbox/app/profiles/master/persistence.db"
        database.parent.mkdir(parents=True)
        connection = dict(
            name="Local populated fixture",
            api_url=f"http://127.0.0.1:{server.server_port}",
        )
        with sqlite3.connect(database) as db:
            db.execute(
                "CREATE TABLE state (key TEXT NOT NULL UNIQUE, value TEXT NOT NULL)"
            )
            for key, value in (
                ("CONNECTION", connection),
                ("CONNECTIONS", [connection]),
            ):
                db.execute("INSERT INTO state VALUES (?, ?)", (key, json.dumps(value)))
        env = {
            key: value
            for key, value in os.environ.items()
            if key in ("PATH", "LANG", "LD_LIBRARY_PATH")
        }
        env.update(
            HOME=home,
            XDG_CONFIG_HOME=home + "/config",
            XDG_DATA_HOME=home + "/data",
            XDG_CACHE_HOME=home + "/cache",
            XDG_STATE_HOME=home + "/state",
            TERM="xterm-256color",
            BMUX_IMAGE_PROTOCOL="kitty",
            BMUX_PANE="fixture",
            RUST_LOG=os.environ.get(
                "BMUX_VISUAL_RUST_LOG",
                "moosicbox_app_native=debug,moosicbox_app_state=debug,hyperchad_renderer_bmux=debug",
            ),
        )
        tty.setraw(0)
        process = subprocess.Popen(
            [executable, "serve"],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            cwd=home,
            start_new_session=True,
        )
        started = time.monotonic()
        drain = PtyDrain(master, screen, recording, True, lambda *args: None)
        drain.start()
        clicked = False
        footer_ready = False
        footer_ready_evidence = None
        sampler = FrameSampler(os.environ["DISPLAY"])
        _, _, pixel_width, pixel_height = struct.unpack("HHHH", geometry)
        cell_width, cell_height = pixel_width / columns, pixel_height / rows
        try:
            while time.monotonic() - started < 20 and process.poll() is None:
                time.sleep(0.02)
                if not clicked and screen.locate("Albums"):
                    x, y = screen.locate("Albums")
                    os.write(
                        master, f"\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m".encode()
                    )
                    clicked = True
                position = server.fixture_position
                age = time.monotonic() - getattr(
                    server, "fixture_sent_at", time.monotonic()
                )
                stage = 5 if age > 5 else 2 if age > 2 else None
                if not footer_ready:
                    logs = "\n".join(
                        log.read_text(errors="replace")
                        for log in Path(home).rglob("*.log")
                    )
                    installed = 'resource installed key="@0/1/0/0/0/0/0/0/0"' in logs
                    with screen.lock:
                        images = list(screen.images)
                        graphics = list(screen.graphics)
                    footer_images = {
                        params.get("i")
                        for params, pixels, (ix, iy) in images
                        if ix < 9
                        and iy >= rows - 4
                        and int(params.get("s", 0)) >= 32
                        and int(params.get("v", 0)) >= 32
                        and bool(pixels)
                    }
                    placed = [
                        (params, ix, iy)
                        for params, ix, iy in graphics
                        if params.get("a") in ("p", "T")
                        and params.get("i") in footer_images
                        and ix < 9
                        and iy >= rows - 4
                    ]
                    pixel_evidence = None
                    if installed and placed:
                        params, ix, iy = placed[-1]
                        rect = (
                            round(ix * cell_width),
                            round(iy * cell_height),
                            round(int(params.get("c", 4)) * cell_width),
                            round(min(2, rows - iy) * cell_height),
                        )
                        _, _, raw, _ = sampler.grab(rect)
                        pixel_evidence = dict(rect=rect, **footer_artwork_pixels(raw))
                    if installed and placed and pixel_evidence["matched"]:
                        footer_ready = True
                        footer_ready_evidence = dict(
                            monotonic=time.monotonic(),
                            age=age,
                            installed_key="@0/1/0/0/0/0/0/0/0",
                            placement=placed[-1],
                            pixels=pixel_evidence,
                            limitation="Fixture artwork pixels observed in actual dedicated X11 framebuffer",
                        )
                        server.clock_events.append(
                            dict(
                                kind="footer_resource_and_placement",
                                **footer_ready_evidence,
                            )
                        )
                if footer_ready and (position, "ready") not in captured:
                    stage = "ready"
                token = (position, stage)
                if position is not None and stage is not None and token not in captured:
                    termios.tcdrain(1)
                    capture_started = time.monotonic()
                    subprocess.run(
                        [
                            os.sys.executable,
                            "-c",
                            "import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); from bmux_live_acceptance import capture_x11; capture_x11(Path(sys.argv[2]))",
                            str(Path(__file__).resolve().parent),
                            str(artifacts / f"position-{position}-{stage}s-kitty.png"),
                        ],
                        check=True,
                        timeout=8,
                    )
                    server.clock_events.append(
                        dict(
                            kind="screenshot",
                            position=position,
                            stage=stage,
                            begin=capture_started,
                            end=time.monotonic(),
                            albums_visible=bool(screen.locate("Albums")),
                            connecting_visible="Connecting" in screen.text(),
                        )
                    )
                    (artifacts / f"position-{position}-{stage}s.txt").write_text(
                        screen.text()
                    )
                    captured.append(token)
                if footer_ready and (position, "ready") in captured and age > 6:
                    break
            os.write(master, b"q")
            process.wait(timeout=2)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=2)
            sampler.close()
            drain.close()
            termios.tcsetattr(0, termios.TCSANOW, modes)
            (artifacts / "clock-events.json").write_text(
                json.dumps(server.clock_events, indent=2)
            )
            (artifacts / "terminal.bin").write_bytes(recording)
            (artifacts / "requests.json").write_text(
                json.dumps(server.requests, indent=2)
            )
            (artifacts / "fixtures.json").write_text(
                json.dumps([session_fixture(p) for p in (0, 15, 39)], indent=2)
            )
            (artifacts / "result.json").write_text(
                json.dumps(
                    dict(
                        executable=executable,
                        sha256=hashlib.file_digest(
                            Path(executable).open("rb"), "sha256"
                        ).hexdigest(),
                        geometry=struct.unpack("HHHH", geometry),
                        captured_positions=captured,
                        footer_ready=footer_ready,
                        footer_ready_evidence=footer_ready_evidence,
                        footer_readiness_timeout_seconds=20,
                        limitation="Local fixture only; screenshots require visible populated footer review",
                    ),
                    indent=2,
                )
            )
            for i, log in enumerate(Path(home).rglob("*.log")):
                (artifacts / f"app-{i}.log").write_bytes(log.read_bytes())
            server.stopping.set()
            server.shutdown()
            os.close(master)
            os.close(slave)
        if not footer_ready:
            raise SystemExit(
                "Footer resource/artwork readiness timed out; inspect preserved artifacts"
            )


if __name__ == "__main__":
    main()
