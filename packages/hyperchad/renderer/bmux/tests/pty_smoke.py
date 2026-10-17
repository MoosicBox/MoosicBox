#!/usr/bin/env python3
"""Live executable PTY acceptance, standard library only; never builds anything.

python3 tests/pty_smoke.py /absolute/path/to/workflows --protocol text
Repeat with kitty/sixel/iterm2 for binaries built with the matching feature.
The PTY emulates advertised capability, not visual decoder correctness.
"""
import argparse
import errno
import fcntl
import hashlib
import os
import pty
import re
import select
import signal
import struct
import subprocess
import termios
import time


def winsize(fd, columns, rows):
    fcntl.ioctl(
        fd,
        termios.TIOCSWINSZ,
        struct.pack("HHHH", rows, columns, columns * 8, rows * 16),
    )


def run(executable, workflow, protocol, exit_key="timed"):
    master, slave = pty.openpty()
    winsize(slave, 120, 60)
    original = termios.tcgetattr(slave)
    env = os.environ.copy()
    for key in (
        "TERM_PROGRAM",
        "KITTY_WINDOW_ID",
        "LC_TERMINAL",
        "WEZTERM_PANE",
        "BMUX_IMAGE_PROTOCOL",
    ):
        env.pop(key, None)
    # Exercise bmux's published emulated-pane capability boundary, selecting
    # exactly one protocol instead of inheriting the real host's capabilities.
    env.update(
        TERM="xterm-256color",
        BMUX_PANE="hyperchad-acceptance",
        BMUX_IMAGE_PROTOCOL="" if protocol == "text" else protocol,
    )
    env.pop("HYPERCHAD_BMUX_IMAGE_PROTOCOL", None)
    process = subprocess.Popen(
        [executable, workflow] + (["--smoke"] if exit_key == "timed" else []),
        stdin=slave,
        stdout=slave,
        stderr=slave,
        env=env,
        start_new_session=True,
    )
    output = bytearray()
    started = time.monotonic()
    sent = False
    resized = False
    quit_sent = False
    try:
        while time.monotonic() - started < 12:
            elapsed = time.monotonic() - started
            if elapsed > 2 and exit_key != "timed" and not quit_sent:
                os.write(master, b"\x03" if exit_key == "ctrl-c" else b"\x11")
                quit_sent = True
            if elapsed > 0.6 and not sent:
                # Keyboard-only edit/navigation + bracketed paste. The malicious
                # source must not become an OSC52 write in terminal output.
                os.write(
                    master,
                    b"\t\t\t\tquery\x1b[200~safe\x1b]52;c;SHOULD_NOT_EXECUTE\x07\x1b[201~\t \t\r\x1b[B\r",
                )
                sent = True
            if elapsed > 1.5 and not resized:
                winsize(slave, 80, 32)
                os.kill(process.pid, signal.SIGWINCH)
                resized = True
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                try:
                    data = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
                if not data:
                    break
                output.extend(data)
                if len(output) > 16 * 1024 * 1024:
                    raise AssertionError("terminal output exceeds acceptance budget")
            if process.poll() is not None and not ready:
                break
        if process.poll() is None:
            raise AssertionError("--smoke did not perform graceful timed shutdown")
        assert process.wait() == 0, f"example failed: {bytes(output[-2000:])!r}"
        assert termios.tcgetattr(slave) == original, "terminal attributes not restored"
        data = bytes(output)
        assert (
            b"\x1b[?1049h" in data and b"\x1b[?1049l" in data
        ), "alternate screen not restored"
        assert b"\x1b]52;" not in data, "hostile clipboard escape reached terminal"
        assert b"NEVER_PAINT_PASSWORD_7e9a" not in data
        expected_title = {
            "dashboard": b"Dashboard",
            "documents": b"Documents",
            "music": b"Music",
        }[workflow]
        # Terminal diffing may split ordinary strings with cursor/style escapes;
        # remove CSI and OSC sequences only for this textual assertion.
        visible = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data)
        assert expected_title in visible, f"workflow title absent: {workflow}"
        kitty = b"\x1b_G" in data
        sixel = re.search(rb"\x1bP[0-9;?]*q", data) is not None
        iterm = b"\x1b]1337;File=" in data
        if protocol == "text":
            assert not (kitty or sixel or iterm), "graphics emitted to a text-only host"
            assert (
                b"Demo album" in visible
            ), f"missing text fallback; visible tail={visible[-1200:]!r}"
        elif protocol == "kitty":
            assert kitty and not sixel and not iterm, "wrong/missing Kitty protocol"
            assert b"a=d" in data, "Kitty cleanup/delete not emitted"
        elif protocol == "sixel":
            assert sixel and not kitty and not iterm, "wrong/missing Sixel protocol"
        else:
            assert iterm and not kitty and not sixel, "wrong/missing iTerm2 protocol"
        print(
            f"PASS {workflow} {protocol}: {len(data)} output bytes, restored terminal"
        )
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        os.close(master)
        os.close(slave)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "executable", help="explicit previously built workflows example"
    )
    parser.add_argument(
        "--protocol", choices=["text", "kitty", "sixel", "iterm2"], default="text"
    )
    parser.add_argument(
        "--exit-key", choices=("timed", "ctrl-c", "ctrl-q"), default="timed"
    )
    args = parser.parse_args()
    executable = os.path.realpath(args.executable)
    if not os.path.isfile(executable) or not os.access(executable, os.X_OK):
        parser.error(
            "executable must exist and be executable; this script does not build"
        )
    with open(executable, "rb") as file:
        digest = hashlib.file_digest(file, "sha256").hexdigest()
    print(f"artifact={executable} sha256={digest} capability={args.protocol}")
    failures = []
    for workflow in ("dashboard", "documents", "music"):
        try:
            run(executable, workflow, args.protocol, args.exit_key)
        except (AssertionError, OSError) as error:
            failures.append(f"{workflow} {args.protocol}: {error}")
            print(f"FAIL {failures[-1]}", flush=True)
    if failures:
        raise SystemExit("\n".join(failures))


if __name__ == "__main__":
    main()
