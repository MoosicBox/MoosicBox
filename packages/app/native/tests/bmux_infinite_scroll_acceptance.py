#!/usr/bin/env python3
"""Independent real-binary infinite-scroll PTY acceptance (no builds).

Launch ONLY after coordinator checkpoint:
  python3 bmux_infinite_scroll_acceptance.py /absolute/app --checkpoint-approved \
      --artifacts /tmp/infinite-evidence --scenario pagination
Optional separate runs: --scenario retry or stale. All upstream traffic uses the
existing deny-by-default read-only gateway. State is seeded ONLY in a disposable
HOME, never in the user's saved configuration. No playback input is generated.
This is text/request acceptance, not compositor, bmux-host, or latency acceptance.
Defaults allow 800 forwarded GETs, including at most 600 artwork GETs (two per
album for three 100-album pages), and 128 MiB aggregate body data. A 16 MiB
reservation per in-flight request prevents concurrent byte-budget overshoot.
Exhaustion fails acceptance; no artwork is silently dropped and history is intact.
"""

import argparse
from collections import Counter
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import threading
import time
import urllib.parse

import bmux_live_acceptance as live

SENTINEL_LABEL = "Auto-load more albums / retry"


def album_query(path):
    parsed = urllib.parse.urlsplit(path)
    if parsed.path != "/menu/albums":
        return None
    query = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)
    return {key: values[0] for key, values in query.items()}


def pages(payloads):
    return [
        (album_query(path), body)
        for path, body in list(payloads)
        if album_query(path) is not None
    ]


def album_id(album):
    # IDs are provider-scoped, not globally unique across music APIs.
    return (str(album.get("apiSource", album.get("source", ""))), str(album["albumId"]))


def validate_pages(payloads):
    """Validate actual page continuity and identity without inventing a final page."""
    result = pages(payloads)
    assert result, "No real album pages received"
    offsets = [int(query.get("offset", 0)) for query, _ in result]
    assert offsets[0] == 0, f"First page is not offset zero: {offsets}"
    assert len(offsets) == len(set(offsets)), f"Duplicate successful offsets: {offsets}"
    identities = []
    baseline = {key: value for key, value in result[0][0].items() if key != "offset"}
    for index, (query, body) in enumerate(result):
        assert isinstance(body, dict) and isinstance(
            body.get("items"), list
        ), "Invalid page shape"
        assert {
            key: value for key, value in query.items() if key != "offset"
        } == baseline, "Page query drift"
        offset, limit = int(body["offset"]), int(body["limit"])
        assert offset == offsets[index], "Response/request offset mismatch"
        assert 0 < limit <= 100, f"Unbounded/nonprogressing page limit: {limit}"
        assert len(body["items"]) <= limit, "Page exceeds its advertised limit"
        if index:
            previous = result[index - 1][1]
            assert offset == int(previous["offset"]) + int(
                previous["limit"]
            ), "Skipped page offset"
        identities.extend(album_id(album) for album in body["items"])
    assert len(identities) == len(
        set(identities)
    ), "Duplicate provider-scoped album identities across pages"
    last = result[-1][1]
    # WithTotal is used by the real endpoint. Unknown-total responses remain a gap.
    total = last.get("total")
    final = total is not None and int(last["offset"]) + len(last["items"]) >= int(total)
    return {
        "offsets": offsets,
        "albums": len(identities),
        "total": total,
        "final": final,
    }


class FaultHandler(live.GatewayHandler):
    """One bounded local fault; shared allowlist and fixed upstream stay authoritative."""

    def do_GET(self):
        query = album_query(self.path)
        if not live.allowed_get(self.path, self.headers):
            return self.deny()
        gateway = self.server
        with gateway.fault_lock:
            if query is not None:
                gateway.album_attempts += 1
                if gateway.album_attempts > gateway.max_album_requests:
                    self.deny()
                    return
            fault = bool(
                query
                and int(query.get("offset", 0)) > 0
                and gateway.scenario != "pagination"
                and not gateway.fault_used
            )
            if fault:
                gateway.fault_used = True
        if fault:
            gateway.fault_path = self.path
            gateway.fault_at = time.monotonic()
            if gateway.scenario == "retry":
                gateway.requests.append(
                    {
                        "t": gateway.fault_at - gateway.started,
                        "method": "GET",
                        "path": self.path,
                        "status": 503,
                        "injected": True,
                    }
                )
                gateway.fault_seen.set()
                self.send_error(503, "Acceptance local one-shot failure")
                return
            # Hold this authorized request before forwarding; after navigation it
            # still returns the unmodified REAL upstream response, never mock data.
            gateway.fault_seen.set()
            if not gateway.release.wait(20):
                self.send_error(504, "Acceptance delay release timed out")
                return
        self.forward_bounded()
        if fault and gateway.scenario == "stale":
            gateway.delivered.set()

    def forward_bounded(self):
        """Reserve request and byte budgets under lock before any upstream IO.

        Reuses allowed_get, NoRedirect, MAX_BODY and the fixed upstream. Every
        attempt stays in history. No artwork is silently skipped or cached.
        """
        gateway = self.server
        artwork = urllib.parse.urlsplit(self.path).path.startswith("/files/albums/")
        started = time.monotonic()
        with gateway.fault_lock:
            reason = None
            if gateway.forward_attempts >= gateway.max_total_requests:
                reason = "total_requests"
            elif artwork and gateway.artwork_attempts >= gateway.max_artwork_requests:
                reason = "artwork_requests"
            elif (
                gateway.forwarded_bytes + gateway.reserved_bytes + live.MAX_BODY
                > gateway.max_bytes
            ):
                reason = "bytes"
            if reason:
                gateway.budget_failures.append({"path": self.path, "budget": reason})
                self.deny()
                return
            gateway.forward_attempts += 1
            gateway.artwork_attempts += int(artwork)
            gateway.reserved_bytes += live.MAX_BODY
            entry = {"t": started - gateway.started, "method": "GET", "path": self.path}
            gateway.requests.append(entry)
        request = urllib.request.Request(
            live.UPSTREAM + self.path,
            headers={"moosicbox-profile": "master"},
            method="GET",
        )
        try:
            with gateway.opener.open(request, timeout=5) as response:
                body = response.read(live.MAX_BODY + 1)
                if len(body) > live.MAX_BODY:
                    raise ValueError("upstream response exceeds per-body bound")
                with gateway.fault_lock:
                    gateway.forwarded_bytes += len(body)
                entry.update(status=response.status, bytes=len(body))
                if self.path.startswith("/menu/"):
                    gateway.payloads.append((self.path, json.loads(body)))
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
            error.close()
            try:
                self.send_error(502, "Upstream browsing response rejected")
            except OSError:
                pass
        except (OSError, ValueError) as error:
            entry["error"] = str(error)
            try:
                self.send_error(502, "Upstream browsing failure")
            except OSError:
                pass
        finally:
            with gateway.fault_lock:
                gateway.reserved_bytes -= live.MAX_BODY
            entry["seconds"] = time.monotonic() - started


class Gateway(live.Gateway):
    def __init__(
        self,
        scenario,
        max_album_requests=6,
        max_total_requests=800,
        max_artwork_requests=600,
        max_bytes=128 * 1024 * 1024,
    ):
        super().__init__()
        self.max_total_requests = max_total_requests
        self.max_artwork_requests = max_artwork_requests
        self.max_bytes = max_bytes
        self.forward_attempts = 0
        self.artwork_attempts = 0
        self.reserved_bytes = 0
        self.budget_failures = []
        self.max_album_requests = max_album_requests
        self.album_attempts = 0
        self.RequestHandlerClass = FaultHandler
        self.scenario = scenario
        self.fault_lock = threading.Lock()
        self.fault_used = False
        self.fault_path = None
        self.fault_at = None
        self.fault_seen = threading.Event()
        self.release = threading.Event()
        self.delivered = threading.Event()


def duplicate_visible_titles(text, payloads, card_columns):
    """Detect repeated unambiguous title/artist pairs in observed card columns."""
    labels = {}
    for x, width in set(card_columns):
        pairs = {}
        for _, body in pages(payloads):
            for album in body["items"]:
                pair = (
                    live.centered_ascii_label(album.get("title", ""), width),
                    live.centered_ascii_label(album.get("artist", ""), width),
                )
                if all(pair) and album.get("title"):
                    pairs.setdefault(pair, set()).add(album_id(album))
        lines = text.splitlines()
        for row in range(8, len(lines) - 1):
            pair = (
                lines[row][x : x + width].ljust(width),
                lines[row + 1][x : x + width].ljust(width),
            )
            identities = pairs.get(pair, set())
            if len(identities) == 1:
                labels.setdefault(next(iter(identities)), set()).add((x, row))
    return [
        {"identity": identity, "positions": sorted(positions)}
        for identity, positions in labels.items()
        if len(positions) > 1
    ]


def run(args):
    executable = Path(args.executable).resolve(strict=True)
    artifacts = Path(args.artifacts).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    with executable.open("rb") as binary:
        sha256 = hashlib.file_digest(binary, "sha256").hexdigest()
    gateway = Gateway(
        args.scenario,
        args.max_pages + 1,
        args.max_total_requests,
        args.max_artwork_requests,
        args.max_bytes_mib * 1024 * 1024,
    )
    threading.Thread(target=gateway.serve_forever, daemon=True).start()
    started = time.monotonic()
    deadline = started + args.timeout
    master, slave = pty.openpty()
    fcntl.ioctl(
        slave,
        termios.TIOCSWINSZ,
        struct.pack("HHHH", args.rows, args.columns, args.columns * 8, args.rows * 16),
    )
    original_modes = termios.tcgetattr(slave)
    screen = live.LockedScreen(args.columns, args.rows)
    recording = bytearray()
    drain = live.PtyDrain(master, screen, recording)
    events, failures, checks, gaps = [], [], {}, []
    observed_ids = set()
    seen_by_page = set()
    process = None
    last_frame = None
    wheel_count = 0

    def event(kind, **values):
        events.append({"t": time.monotonic() - started, "kind": kind, **values})

    def sample():
        nonlocal last_frame
        assert time.monotonic() < deadline, "Overall acceptance deadline exceeded"
        assert not drain.error, f"PTY drain error: {drain.error}"
        assert (
            process is None or process.poll() is None
        ), "Application exited unexpectedly"
        text = screen.text()
        if text != last_frame:
            number = sum(item["kind"] == "text" for item in events)
            name = f"frame-{number:05d}.txt"
            (artifacts / name).write_text(text)
            event("text", file=name)
            last_frame = text
        positions = live.album_positions(screen, list(gateway.payloads))
        observed_ids.update(positions)
        # The existing helper understands ASCII ellipsized card labels as well.
        with screen.lock:
            placements = live.current_cover_placements(list(screen.graphics))
        columns = [
            (x, int(params.get("c", 0)))
            for params, x, y in placements
            if x >= 18 and int(params.get("c", 0)) >= 4 and 0 <= y < args.rows - 3
        ]
        positions.update(
            live.displayed_album_positions(screen, list(gateway.payloads), columns)
        )
        for query, body in pages(gateway.payloads):
            if any(str(album["albumId"]) in positions for album in body["items"]):
                seen_by_page.add(int(query.get("offset", 0)))
        duplicates = duplicate_visible_titles(text, gateway.payloads, columns)
        assert (
            not duplicates
        ), f"Duplicate visible unambiguous album titles: {duplicates}"
        assert not any(
            request.get("denied") and album_query(request["path"]) is not None
            for request in gateway.requests
        ), "Gateway bound/allowlist denied album pagination"
        assert (
            not gateway.budget_failures
        ), f"Gateway budget exhausted: {gateway.budget_failures}"
        assert len(requests()) <= args.max_pages + 1, "Album-request budget exceeded"
        time.sleep(0.03)

    def wait(predicate, description, seconds=12):
        until = min(deadline, time.monotonic() + seconds)
        while time.monotonic() < until:
            sample()
            if predicate():
                return
        raise AssertionError(f"Timed out: {description}")

    def idle(seconds):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            sample()

    def click(label):
        position = screen.locate(label)
        assert position, f"Missing navigation label {label}"
        x, y = position
        os.write(master, f"\x1b[<0;{x + 1};{y + 1}M\x1b[<0;{x + 1};{y + 1}m".encode())
        event("click", label=label, x=x, y=y)

    def wheel(up=False, count=1):
        nonlocal wheel_count
        assert (
            wheel_count + count <= args.max_wheels
        ), "Wheel-event drive budget exhausted"
        wheel_count += count
        x, y = args.columns * 3 // 4, args.rows // 2
        data = f"\x1b[<{64 if up else 65};{x};{y}M".encode()
        for _ in range(count):
            os.write(master, data)
            event("wheel", direction="up" if up else "down", x=x, y=y)
            sample()

    def requests():
        return [
            request
            for request in list(gateway.requests)
            if album_query(request["path"]) is not None
        ]

    with tempfile.TemporaryDirectory(prefix="bmux-infinite-read-only-") as home:
        database = Path(home) / ".local/moosicbox/app/profiles/master/persistence.db"
        database.parent.mkdir(parents=True)
        connection = {
            "name": "Read-only infinite acceptance",
            "api_url": f"http://127.0.0.1:{gateway.server_port}",
        }
        with sqlite3.connect(database) as persistence:
            persistence.execute(
                "CREATE TABLE state (key TEXT NOT NULL UNIQUE, value TEXT NOT NULL)"
            )
            persistence.executemany(
                "INSERT INTO state VALUES (?, ?)",
                [
                    (key, json.dumps(value))
                    for key, value in [
                        ("CONNECTION", connection),
                        ("CONNECTIONS", [connection]),
                    ]
                ],
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
            BMUX_PANE="infinite-read-only-test",
            RUST_LOG="hyperchad=debug,moosicbox_app_native=debug",
        )
        try:
            process = subprocess.Popen(
                [str(executable), "serve"],
                stdin=slave,
                stdout=slave,
                stderr=slave,
                cwd=home,
                env=env,
                start_new_session=True,
            )
            drain.start()
            wait(lambda: screen.locate("Albums"), "startup navigation")
            idle(0.4)
            click("Albums")
            wait(lambda: len(pages(gateway.payloads)) >= 1, "real initial album page")
            wait(lambda: 0 in seen_by_page, "initial real album title rendered")
            idle(args.idle)
            assert (
                len(requests()) == 1
            ), "Initial viewport drained/requested successor without scrolling"
            checks["initial_no_drain"] = True
            event("initial_no_drain", requests=len(requests()))
            if args.scenario == "pagination":
                while True:
                    summary = validate_pages(gateway.payloads)
                    if summary["final"] or len(summary["offsets"]) >= args.max_pages:
                        break
                    wheel()
                    # Give asynchronous responses a bounded chance to arrive;
                    # wheel input continues until a page is actually rendered.
                    idle(0.07)
                summary = validate_pages(gateway.payloads)
                while summary["offsets"][-1] not in seen_by_page:
                    wheel()
                    idle(0.07)
                summary = validate_pages(gateway.payloads)
                assert (
                    len(summary["offsets"]) >= 3
                ), "Library/run did not exercise two successor pages"
                checks["two_successors"] = True
                requested_offsets = [
                    int(album_query(request["path"]).get("offset", 0))
                    for request in requests()
                ]
                assert len(requested_offsets) == len(
                    set(requested_offsets)
                ), "Duplicate attempted offsets"
                checks["unique_offsets_and_response_identities"] = True
                checks["observed_each_page"] = all(
                    offset in seen_by_page for offset in summary["offsets"]
                )
                assert checks[
                    "observed_each_page"
                ], "Some fetched pages were never observed rendered"
                if summary["final"]:
                    for _ in range(8):
                        wheel()
                    checks["actual_final_page"] = True
                else:
                    gaps.append(
                        f"Actual final page not reached within max_pages={args.max_pages}"
                    )
            else:
                while not gateway.fault_seen.is_set():
                    wheel()
                    idle(0.07)
                event("fault_observed", path=gateway.fault_path, scenario=args.scenario)
                if args.scenario == "retry":
                    before = len(requests())
                    idle(args.idle)
                    assert (
                        len(requests()) == before
                    ), "Failed stationary sentinel hot-looped"
                    checks["failed_idle_no_retry_loop"] = True
                    # Leave the reveal band, then re-enter it using actual scroll.
                    wheel(up=True, count=max(12, args.rows))
                    idle(0.3)
                    while len(pages(gateway.payloads)) < 2:
                        wheel()
                        idle(0.07)
                    validate_pages(gateway.payloads)
                    counts = Counter(request["path"] for request in requests())
                    assert (
                        counts[gateway.fault_path] == 2
                    ), "Expected one failed attempt and one retry"
                    while len(seen_by_page) < 2:
                        wheel()
                        idle(0.07)
                    checks["retry_reentry_real_page"] = True
                else:
                    before_navigation = screen.text()
                    click("Home")
                    wait(
                        lambda: screen.text() != before_navigation
                        and not live.album_positions(screen, list(gateway.payloads))
                        and not screen.locate(SENTINEL_LABEL),
                        "leave album sentinel document",
                    )
                    idle(0.5)
                    home_text = screen.text()
                    event("stale_navigation_baseline", text=home_text)
                    gateway.release.set()
                    wait(
                        gateway.delivered.is_set,
                        "delayed real upstream response delivered",
                    )
                    wait(
                        lambda: len(pages(gateway.payloads)) >= 2,
                        "real stale page response recorded",
                    )
                    assert any(
                        request.get("status") == 200
                        and request["path"] == gateway.fault_path
                        for request in requests()
                    ), "Delayed request did not deliver a successful real response"
                    idle(args.idle)
                    stale = pages(gateway.payloads)[-1][1]
                    unique = [
                        album.get("title", "")
                        for album in stale["items"]
                        if album.get("title") and album["title"] not in home_text
                    ]
                    assert (
                        unique
                    ), "No discriminating stale album titles; cannot prove rejection"
                    assert not any(
                        title in screen.text() for title in unique
                    ), "Stale albums appended after Home navigation"
                    assert not screen.locate(
                        SENTINEL_LABEL
                    ), "Stale response restored album sentinel"
                    checks["stale_response_not_rendered"] = True
            before = len(requests())
            idle(args.idle)
            assert (
                len(requests()) == before
            ), "Idle after scrolling scheduled extra pagination"
            checks["idle_no_extra_album_requests"] = True
        except Exception as error:
            failures.append(f"{type(error).__name__}: {error}")
        finally:
            gateway.release.set()
            if process and process.poll() is None:
                event("quit", input="ctrl-c")
                os.write(master, b"\x03")
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    failures.append(
                        "Graceful Ctrl-C quit timed out; fallback termination required"
                    )
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=3)
                checks["graceful_quit"] = process.returncode == 0
                if not checks["graceful_quit"]:
                    failures.append(f"Application quit returned {process.returncode}")
            if drain.thread.ident:
                drain.close()
            checks["terminal_restored"] = termios.tcgetattr(slave) == original_modes
            if not checks["terminal_restored"]:
                failures.append("Terminal modes not restored")
            for index, log in enumerate(Path(home).rglob("*.log")):
                (artifacts / f"app-{index}.log").write_bytes(log.read_bytes())
            gateway.shutdown()
            gateway.server_close()
            if gateway.budget_failures:
                failures.append(f"Gateway budget exhausted: {gateway.budget_failures}")
            os.close(master)
            os.close(slave)
    gaps.extend(
        [
            "No compositor/bmux-host placement or performance acceptance",
            "No complete DOM/card multiplicity proof: response identities and unambiguous visible titles only",
            "Hidden/nested scrollports require focused renderer tests",
            "Other scenarios require separate checkpoint-approved runs",
        ]
    )
    result = {
        "result": "failed" if failures else "partial_acceptance",
        "checks": checks,
        "failures": failures,
        "gaps": gaps,
        "executable": str(executable),
        "sha256": sha256,
        "upstream": live.UPSTREAM,
        "scenario": args.scenario,
        "geometry": {"rows": args.rows, "columns": args.columns},
        "observed_album_ids": sorted(observed_ids),
        "observed_page_offsets": sorted(seen_by_page),
        "events": events,
        "wheel_events": wheel_count,
        "budgets": {
            "max_wheels": args.max_wheels,
            "album_requests": args.max_pages + 1,
            "total_requests": args.max_total_requests,
            "artwork_requests": args.max_artwork_requests,
            "bytes": args.max_bytes_mib * 1024 * 1024,
            "timeout_seconds": args.timeout,
        },
        "gateway_usage": {
            "forward_attempts": gateway.forward_attempts,
            "artwork_attempts": gateway.artwork_attempts,
            "forwarded_bytes": gateway.forwarded_bytes,
            "budget_failures": gateway.budget_failures,
        },
        "elapsed_seconds": time.monotonic() - started,
    }
    for name, value in [
        ("result.json", result),
        ("requests.json", gateway.requests),
        ("responses.json", gateway.payloads),
    ]:
        (artifacts / name).write_text(json.dumps(value, indent=2))
    (artifacts / "terminal.bin").write_bytes(recording)
    (artifacts / "final.txt").write_text(screen.text())
    print(json.dumps(result, indent=2))
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("--artifacts", required=True)
    parser.add_argument(
        "--checkpoint-approved",
        action="store_true",
        help="Coordinator has approved this binary capture run",
    )
    parser.add_argument(
        "--scenario", choices=("pagination", "retry", "stale"), default="pagination"
    )
    parser.add_argument("--max-pages", type=int, default=5)
    parser.add_argument("--max-wheels", type=int, default=400)
    parser.add_argument("--max-total-requests", type=int, default=800)
    parser.add_argument("--max-artwork-requests", type=int, default=600)
    parser.add_argument("--max-bytes-mib", type=int, default=128)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--idle", type=float, default=1.5)
    parser.add_argument("--rows", type=int, default=40)
    parser.add_argument("--columns", type=int, default=140)
    args = parser.parse_args()
    if not args.checkpoint_approved:
        parser.error(
            "Capture launch requires coordinator checkpoint and --checkpoint-approved"
        )
    if not (
        3 <= args.max_pages <= 20
        and 20 <= args.max_wheels <= 1000
        and 1 <= args.idle <= 10
        and 10 <= args.timeout <= 300
        and 20 <= args.max_total_requests <= 2000
        and 0 <= args.max_artwork_requests <= args.max_total_requests
        and 32 <= args.max_bytes_mib <= 256
        and 16 <= args.rows <= 100
        and 60 <= args.columns <= 240
    ):
        parser.error("Invalid bounded page/time/geometry parameters")
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
