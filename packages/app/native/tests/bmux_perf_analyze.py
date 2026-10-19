"""Offline capture analysis; never launches a process or contacts a server.

Kitty hashes identify reassembled, base64-decoded transport content plus format,
size and compression metadata. They are NOT decoded-pixel/artwork identities:
PNG encoder differences and changing crops can hide visually repeated sources.
Only direct (t=d, the default) transmissions are hashed. Memory is bounded per
APC command, not per recording/upload; summaries retain one entry per hash.
"""

import argparse
import base64
import binascii
import collections
import hashlib
import json
import math
from pathlib import Path
import re


class KittyAnalyzer:
    """Incrementally inspect ESC_G ... ESC\\ commands, including split markers."""

    def __init__(self, max_command_bytes=1024 * 1024):
        self.limit = max_command_bytes
        self.buffer = bytearray()
        self.inside = False
        self.discard = False
        self.active = None
        self.counts = collections.Counter()
        self.sources = {}

    def feed(self, data):
        self.counts["terminal_bytes"] += len(data)
        self.buffer.extend(data)
        while True:
            marker = b"\x1b\\" if self.inside else b"\x1b_G"
            end = self.buffer.find(marker)
            if end < 0:
                if not self.inside:
                    self.buffer[:] = self.buffer[-2:]
                elif len(self.buffer) > self.limit:
                    if not self.discard:
                        self.counts["oversized_commands"] += 1
                    self.discard = True
                    self.buffer[:] = self.buffer[-1:]
                return
            command = bytes(self.buffer[:end])
            del self.buffer[: end + len(marker)]
            if self.inside:
                if not self.discard and len(command) <= self.limit:
                    self.command(command)
                else:
                    if not self.discard:
                        self.counts["oversized_commands"] += 1
                    if self.active:
                        self.active["valid"] = False
                self.discard = False
            self.inside = not self.inside

    def command(self, command):
        self.counts["kitty_commands"] += 1
        header, _, payload = command.partition(b";")
        try:
            fields = dict(part.split(b"=", 1) for part in header.split(b",") if part)
        except ValueError:
            self.counts["malformed_commands"] += 1
            if self.active:
                self.active["valid"] = False
            return
        action = fields.get(b"a", b"t")
        continuation = self.active is not None and action in (None, b"t", b"T")
        if action not in (b"t", b"T") and not continuation:
            self.counts[
                {b"p": "placements", b"d": "deletions"}.get(action, "other_commands")
            ] += 1
            return
        self.counts["transmit_chunks"] += 1
        self.counts["transmit_payload_bytes"] += len(payload)
        if not continuation:
            metadata = {
                k.decode(): fields.get(k, default).decode(errors="replace")
                for k, default in (
                    (b"f", b"32"),
                    (b"s", b"0"),
                    (b"v", b"0"),
                    (b"o", b""),
                )
            }
            digest = hashlib.sha256(
                json.dumps(metadata, sort_keys=True).encode() + b"\0"
            )
            self.active = dict(
                digest=digest,
                encoded=0,
                decoded=0,
                valid=fields.get(b"t", b"d") == b"d",
                image_id=fields.get(b"i"),
            )
            self.counts["upload_starts"] += 1
            if not self.active["valid"]:
                self.counts["unsupported_uploads"] += 1
        upload = self.active
        if fields.get(b"i", upload["image_id"]) != upload["image_id"]:
            upload["valid"] = False
            self.counts["conflicting_chunk_ids"] += 1
        upload["encoded"] += len(payload)
        try:
            decoded = base64.b64decode(payload, validate=True)
            upload["digest"].update(decoded)
            upload["decoded"] += len(decoded)
        except binascii.Error:
            upload["valid"] = False
            self.counts["invalid_base64_chunks"] += 1
        if fields.get(b"m", b"0") != b"0":
            return
        self.counts["completed_uploads"] += 1
        if upload["valid"]:
            self.counts["hashed_uploads"] += 1
            identity = upload["digest"].hexdigest()
            if identity in self.sources:
                self.sources[identity]["uploads"] += 1
                self.counts["repeat_uploads"] += 1
                self.counts["repeat_encoded_payload_bytes"] += upload["encoded"]
                self.counts["repeat_decoded_transport_bytes"] += upload["decoded"]
            else:
                self.sources[identity] = dict(
                    uploads=1, decoded_bytes=upload["decoded"]
                )
        self.active = None

    def finish(self):
        return dict(
            **{
                key: self.counts[key]
                for key in (
                    "terminal_bytes",
                    "kitty_commands",
                    "transmit_chunks",
                    "transmit_payload_bytes",
                    "upload_starts",
                    "completed_uploads",
                    "hashed_uploads",
                    "repeat_uploads",
                    "repeat_encoded_payload_bytes",
                    "repeat_decoded_transport_bytes",
                    "placements",
                    "deletions",
                    "other_commands",
                    "oversized_commands",
                    "malformed_commands",
                    "invalid_base64_chunks",
                    "conflicting_chunk_ids",
                    "unsupported_uploads",
                )
            },
            unique_source_hashes=len(self.sources),
            incomplete_command=self.inside,
            incomplete_upload=self.active is not None,
            hash_upload_multiplicities=dict(
                sorted(
                    collections.Counter(
                        value["uploads"] for value in self.sources.values()
                    ).items()
                )
            ),
            top_repeated_hashes=[
                dict(sha256=key, **value)
                for key, value in sorted(
                    self.sources.items(),
                    key=lambda item: (-item[1]["uploads"], item[0]),
                )[:10]
            ],
            hash_semantics="format/size/compression metadata + decoded transport bytes, not pixels",
        )


STAGES = {
    "prepare": re.compile(r"bmux prepare elapsed_us=(\d+)"),
    "scroll_reuse": re.compile(r"bmux prepare scroll-reuse elapsed_us=(\d+)"),
    "resource_only": re.compile(r"bmux prepare resource-only elapsed_us=(\d+)"),
    "present": re.compile(r"bmux presenter output present_us=(\d+)"),
    "write": re.compile(r"bmux presenter output .*?\bwrite_us=(\d+)"),
    "flush": re.compile(r"bmux presenter output .*?\bflush_us=(\d+)"),
}


def stage_metrics(lines):
    """Independent stage distributions; nearest rank, not paired input latency."""
    samples = {name: [] for name in STAGES}
    for line in lines:
        for name, pattern in STAGES.items():
            match = pattern.search(line)
            if match:
                samples[name].append(int(match[1]))
    result = {}
    for name, values in samples.items():
        values.sort()
        result[name] = dict(
            samples=len(values),
            **{
                key: values[math.ceil(len(values) * q) - 1] / 1000 if values else None
                for key, q in (("p50_ms", 0.5), ("p95_ms", 0.95), ("max_ms", 1))
            },
        )
    return result


def analyze_capture(directory):
    """Read a saved capture with bounded binary reads; no acceptance verdict."""
    directory = Path(directory)
    analyzer = KittyAnalyzer()
    with (directory / "terminal.bin").open("rb") as stream:
        while data := stream.read(65536):
            analyzer.feed(data)
    with (directory / "app-0.log").open() as stream:
        stages = stage_metrics(stream)
    with (directory / "result.json").open() as stream:
        capture = json.load(stream)
    return dict(
        directory=str(directory),
        capture={
            key: capture.get(key)
            for key in (
                "sha256",
                "result",
                "elapsed_seconds",
                "wheel_events",
                "geometry",
                "observed_page_offsets",
                "gaps",
            )
        },
        stages=stages,
        kitty=analyzer.finish(),
        limitations=[
            "No visible latency or CPU-only inference from stage wall times",
            "Repeated wire content is not proof of avoidable source uploads",
            "No frame/page stage correlation without explicit frame markers",
        ],
    )


def replay_profile(path):
    """Replay identical saved bytes through analyzer and acceptance parser separately.

    File reads are outside feed timings. This diagnoses helper CPU cost, not PTY
    backpressure or renderer latency. Screen retains decoded graphics as usual.
    """
    import time
    from bmux_pty_acceptance import Screen

    results = {}
    for name, factory in (
        ("stream_analyzer", KittyAnalyzer),
        ("acceptance_screen", Screen),
    ):
        consumer = factory()
        elapsed_ns = total = 0
        digest = hashlib.sha256()
        with Path(path).open("rb") as stream:
            while data := stream.read(65536):
                total += len(data)
                digest.update(data)
                started = time.perf_counter_ns()
                consumer.feed(data)
                elapsed_ns += time.perf_counter_ns() - started
        results[name] = dict(bytes=total, sha256=digest.hexdigest(), feed_ns=elapsed_ns)
        if isinstance(consumer, KittyAnalyzer):
            results[name]["analysis"] = consumer.finish()
    assert (
        results["stream_analyzer"]["sha256"] == results["acceptance_screen"]["sha256"]
    )
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", nargs="*", type=Path)
    parser.add_argument(
        "--replay", type=Path, help="separate fixed-byte helper CPU replay, no renderer"
    )
    args = parser.parse_args()
    if args.replay:
        print(json.dumps(replay_profile(args.replay), indent=2))
    else:
        if not args.captures:
            parser.error("provide capture directories or --replay terminal.bin")
        print(json.dumps([analyze_capture(path) for path in args.captures], indent=2))
