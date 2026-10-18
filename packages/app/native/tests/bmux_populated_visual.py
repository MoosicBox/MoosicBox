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
from bmux_live_acceptance import capture_x11, LockedScreen, PtyDrain


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
            RUST_LOG="moosicbox_app_native=debug,moosicbox_app_state=debug",
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
        try:
            while time.monotonic() - started < 8 and process.poll() is None:
                time.sleep(0.02)
                if not clicked and screen.locate("Albums"):
                    x, y = screen.locate("Albums")
                    os.write(
                        master, f"\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m".encode()
                    )
                    clicked = True
                position = server.fixture_position
                if (
                    position is not None
                    and position not in captured
                    and time.monotonic() - server.fixture_sent_at > 2
                ):
                    termios.tcdrain(1)
                    subprocess.run(
                        [
                            os.sys.executable,
                            "-c",
                            "import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); from bmux_live_acceptance import capture_x11; capture_x11(Path(sys.argv[2]))",
                            str(Path(__file__).resolve().parent),
                            str(artifacts / f"position-{position}-kitty.png"),
                        ],
                        check=True,
                        timeout=8,
                    )
                    (artifacts / f"position-{position}.txt").write_text(screen.text())
                    captured.append(position)
            os.write(master, b"q")
            process.wait(timeout=2)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=2)
            drain.close()
            termios.tcsetattr(0, termios.TCSANOW, modes)
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


if __name__ == "__main__":
    main()
