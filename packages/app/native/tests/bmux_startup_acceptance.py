#!/usr/bin/env python3
"""Persisted unreachable-server startup, navigation recovery and prompt Ctrl-C acceptance."""

import argparse
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

from bmux_pty_acceptance import Mock, Screen


def run(executable, artifacts):
    executable = Path(executable).resolve(strict=True)
    artifacts = Path(artifacts)
    artifacts.mkdir(parents=True, exist_ok=True)
    mock = Mock()
    mock.album_count = 1
    mock.hold_websocket = True
    # These requests accept TCP and then deliberately never return a response.
    original = mock.RequestHandlerClass.do_GET

    def stalled(handler):
        if handler.path.startswith(("/downloader/", "/menu/", "/music-api")):
            handler.server.requests.append(handler.path)
            handler.server.stopping.wait(60)
            return
        original(handler)

    mock.RequestHandlerClass = type(
        "Stalled", (mock.RequestHandlerClass,), {"do_GET": stalled}
    )
    threading.Thread(target=mock.serve_forever, daemon=True).start()
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 60, 140, 1120, 960))
    original_modes = termios.tcgetattr(slave)
    screen, recording = Screen(), bytearray()
    process = None
    with tempfile.TemporaryDirectory(prefix="bmux-persisted-startup-") as home:
        db = Path(home) / ".local/moosicbox/app/profiles/master/persistence.db"
        db.parent.mkdir(parents=True)
        connection = {
            "name": "Stalled server",
            "api_url": f"http://127.0.0.1:{mock.server_port}",
        }
        with sqlite3.connect(db) as persistence:
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
            k: v
            for k, v in os.environ.items()
            if k in ("PATH", "LANG", "LC_ALL", "LD_LIBRARY_PATH")
        }
        env.update(
            HOME=home,
            XDG_CONFIG_HOME=home + "/config",
            XDG_STATE_HOME=home + "/state",
            XDG_DATA_HOME=home + "/data",
            XDG_CACHE_HOME=home + "/cache",
            TERM="xterm-256color",
            BMUX_IMAGE_PROTOCOL="kitty",
            BMUX_PANE="startup-test",
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

            def pump():
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        data = os.read(master, 262144)
                    except OSError:
                        data = b""
                    recording.extend(data)
                    screen.feed(data)

            def wait(predicate, description, timeout=8):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    pump()
                    assert process.poll() is None, f"early exit: {process.returncode}"
                    if predicate():
                        return
                raise AssertionError(description + "\n" + screen.text())

            def click(label):
                wait(lambda: screen.locate(label), "missing " + label)
                x, y = screen.locate(label)
                os.write(master, f"\x1b[<0;{x+2};{y+1}M\x1b[<0;{x+2};{y+1}m".encode())

            wait(lambda: screen.locate("Albums"), "startup navigation missing")
            wait(lambda: bool(screen.images), "startup icons missing")
            # Click the actual gear image in this fixed 140x60 viewport. This
            # exercises nested image -> anchor targeting, not just keyboard focus.
            started = time.monotonic()
            os.write(master, b"\x1b[<0;18;2M\x1b[<0;18;2m")
            wait(
                lambda: "New Connection" in screen.text()
                and "API URL:" in screen.text(),
                "Settings blocked behind unreachable music API",
                3,
            )
            settings_recovery = time.monotonic() - started
            assert settings_recovery < 3, "local Settings navigation was not prompt"
            click("Downloads")
            wait(
                lambda: any(
                    p.startswith("/downloader/download-tasks") for p in mock.requests
                ),
                "route did not reach stalled server",
            )
            started = time.monotonic()
            click("Home")
            wait(
                lambda: "Connecting to server" in screen.text()
                and screen.locate("Albums"),
                "Home blocked behind stalled Downloads",
                3,
            )
            recovery = time.monotonic() - started
            # Stall an in-document load effect before quit as well.
            click("Albums")
            wait(
                lambda: any(p.startswith("/menu/albums") for p in mock.requests),
                "album fragment did not reach stalled server",
            )
            started = time.monotonic()
            os.write(master, b"\x1b[<0;18;2M\x1b[<0;18;2m")
            wait(
                lambda: "New Connection" in screen.text()
                and "API URL:" in screen.text(),
                "Settings blocked behind stalled album load",
                3,
            )
            settings_from_albums = time.monotonic() - started
            started = time.monotonic()
            os.write(master, b"\x03")
            while process.poll() is None and time.monotonic() - started < 3:
                pump()
            assert (
                process.poll() == 0
            ), "Ctrl-C did not promptly exit while HTTP was stalled"
            assert (
                termios.tcgetattr(slave) == original_modes
            ), "terminal modes not restored"
            assert b"\x1b[?1049l" in recording, "alternate screen not restored"
            metadata = {
                "sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                "settings_recovery_seconds": settings_recovery,
                "settings_from_stalled_albums_seconds": settings_from_albums,
                "home_recovery_seconds": recovery,
                "quit_seconds": time.monotonic() - started,
                "result": "passed",
            }
            (artifacts / "result.json").write_text(json.dumps(metadata, indent=2))
            print(json.dumps(metadata))
        finally:
            if process and process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            mock.stopping.set()
            mock.shutdown()
            (artifacts / "terminal.bin").write_bytes(recording)
            (artifacts / "screen.txt").write_text(screen.text())
            for log in Path(home).rglob("*.log"):
                (artifacts / log.name).write_bytes(log.read_bytes())
            os.close(master)
            os.close(slave)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--artifacts", required=True)
    args = parser.parse_args()
    run(args.executable, args.artifacts)
