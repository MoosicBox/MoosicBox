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
    "project": re.compile(r"bmux stages .*?\bproject_us=(\d+)"),
    "controls": re.compile(r"bmux stages .*?\bcontrols_us=(\d+)"),
    "reveal": re.compile(r"bmux stages .*?\breveal_us=(\d+)"),
    "resource": re.compile(r"bmux stages .*?\bresource_us=(\d+)"),
    "paint": re.compile(r"bmux stages .*?\bpaint_us=(\d+)"),
    "prepare": re.compile(r"bmux prepare elapsed_us=(\d+)"),
    "scroll_reuse": re.compile(r"bmux prepare scroll-reuse elapsed_us=(\d+)"),
    "resource_only": re.compile(r"bmux prepare resource-only elapsed_us=(\d+)"),
    "present": re.compile(r"bmux presenter output present_us=(\d+)"),
    "layout": re.compile(r"bmux prepare .*?\blayout_us=(\d+)"),
    "resources": re.compile(r"bmux prepare .*?\bresources_us=(\d+)"),
    "max_write": re.compile(r"bmux presenter output .*?\bmax_write_us=(\d+)"),
    "write": re.compile(r"bmux presenter output .*?\bwrite_us=(\d+)"),
    "flush": re.compile(r"bmux presenter output .*?\bflush_us=(\d+)"),
}


def stage_metrics(lines):
    """Independent stage distributions; nearest rank, not paired input latency."""
    samples = {name: [] for name in STAGES}
    for line in lines:
        for name, pattern in STAGES.items():
            match = pattern.search(line)
            if match and (match.end() == len(line) or line[match.end()].isspace()):
                samples[name].append(int(match[1]))
    return {name: timing_distribution(values) for name, values in samples.items()}


def timing_distribution(values):
    """Nearest-rank milliseconds; missing samples are not zero-duration work."""
    values = sorted(values)
    return dict(
        samples=len(values),
        **{
            key: values[math.ceil(len(values) * q) - 1] / 1000 if values else None
            for key, q in (("p50_ms", 0.5), ("p95_ms", 0.95), ("max_ms", 1))
        },
    )


def duration_histogram_metrics(buckets, sum_us, max_us):
    """Summarize runtime DurationHistogram values, not a serialized log schema.

    Bounds follow packages/tui-runtime/src/stats.rs. Counts and sum saturate;
    any saturation makes rank inference unavailable. Open-ended buckets never
    become exact percentile estimates, even when max_us is available.
    """
    ceiling = (1 << 64) - 1
    if (
        not isinstance(buckets, (list, tuple))
        or len(buckets) != 8
        or any(type(value) is not int or not 0 <= value <= ceiling for value in buckets)
        or any(
            type(value) is not int or not 0 <= value <= ceiling
            for value in (sum_us, max_us)
        )
    ):
        raise ValueError("expected eight u64 buckets and u64 sum_us/max_us")
    count = sum(buckets)
    saturated = ceiling in (*buckets, sum_us, max_us) or count >= ceiling
    quantiles = {}
    bounds = (100, 500, 1000, 4000, 16000, 64000, 256000, None)
    for label, numerator in (("p50", 50), ("p95", 95)):
        interval = None
        if count and not saturated:
            rank = (count * numerator + 99) // 100
            cumulative = 0
            for index, value in enumerate(buckets):
                cumulative += value
                if cumulative >= rank:
                    interval = dict(
                        lower_us=0 if index == 0 else bounds[index - 1] + 1,
                        upper_us=bounds[index],
                    )
                    break
        quantiles[label] = interval
    return dict(
        samples=count,
        sum_us=sum_us,
        max_us=max_us if count else None,
        status=(
            "saturated_unknown_rank"
            if saturated
            else "bounded" if count else "missing_samples"
        ),
        quantile_method="nearest_rank_bucket_bounds_not_exact_percentiles",
        quantile_bounds=quantiles,
        input_causality="unknown",
    )


def phase_metrics(lines):
    """Classify existing logs only; timestamps never imply input/frame linkage."""
    samples = {
        outcome: {name: [] for name in STAGES}
        for outcome in ("successful", "failed", "unknown")
    }
    # Keep only admission records, not the entire log or unrelated payloads.
    admission_lines = []
    for line in lines:
        outcome = re.search(r"\bsuccess=(true|false)(?=\s|$)", line)
        key = (
            ("successful" if outcome[1] == "true" else "failed")
            if outcome
            else "unknown"
        )
        for name, pattern in STAGES.items():
            match = pattern.search(line)
            if match and (match.end() == len(line) or line[match.end()].isspace()):
                samples[key][name].append(int(match[1]))
        if re.search(r"\bbmux input admission_(attempt|result|cancelled)\b", line):
            admission_lines.append(line)
    phases = {
        outcome: {name: timing_distribution(values) for name, values in fields.items()}
        for outcome, fields in samples.items()
    }
    coverage = {
        name: (
            "observed_unpaired"
            if any(fields[name] for fields in samples.values())
            else "missing_or_invalid"
        )
        for name in STAGES
    }
    # Admission sequence is independent of stage/frame sequence. Duplicate IDs
    # (including concatenated runs) cannot be safely paired without a run scope.
    attempts, results = collections.defaultdict(list), collections.defaultdict(list)
    cancelled = collections.Counter()
    malformed = 0
    for line in admission_lines:
        marker = re.search(r"\bbmux input admission_(attempt|result|cancelled)\b", line)
        if not marker:
            continue
        sequence = re.search(r"\bsequence=Some\((\d+)\)(?=\s|$)", line)
        if not sequence:
            malformed += 1
            continue
        identity = int(sequence[1])
        if marker[1] == "cancelled":
            cancelled[identity] += 1
        else:
            (attempts if marker[1] == "attempt" else results)[identity].append(line)
    counts = collections.Counter()
    samples = {"pre_enqueue": [], "admission_wait": []}
    for identity in attempts.keys() | results.keys() | cancelled.keys():
        before, after = attempts[identity], results[identity]
        if (
            len(before) > 1
            or len(after) > 1
            or cancelled[identity] > 1
            or (after and cancelled[identity])
        ):
            counts["ambiguous"] += 1
            continue
        if cancelled[identity]:
            counts["cancelled" if before else "missing_attempt"] += 1
            continue
        if not before or not after:
            counts["missing_result" if before else "missing_attempt"] += 1
            continue
        observed = re.search(r"\bobserved_us=(\d+)(?=\s|$)", before[0])
        enqueue = re.search(r"\benqueue_us=(\d+)(?=\s|$)", before[0])
        completion = re.search(r"\bcompletion_us=(\d+)(?=\s|$)", after[0])
        accepted = re.search(r"\baccepted=(true|false)(?=\s|$)", after[0])
        if not all((observed, enqueue, completion, accepted)):
            counts["invalid"] += 1
            continue
        start, queued, end = int(observed[1]), int(enqueue[1]), int(completion[1])
        if not start <= queued <= end:
            counts["invalid"] += 1
            continue
        if accepted[1] == "false":
            counts["rejected"] += 1
            continue
        counts["matched"] += 1
        samples["pre_enqueue"].append(queued - start)
        samples["admission_wait"].append(end - queued)
    return dict(
        quantile_method="nearest_rank",
        aggregate_scope="all_logged_outcomes_unpaired_not_end_to_end",
        coverage=coverage,
        by_outcome=phases,
        admission=dict(
            scope="managed_reader_observation_to_fifo_admission_only",
            correlation="sequence_matched_within_log_run_context_epoch_unverified",
            counts={
                key: counts[key]
                for key in (
                    "matched",
                    "missing_attempt",
                    "missing_result",
                    "cancelled",
                    "rejected",
                    "invalid",
                    "ambiguous",
                )
            },
            malformed_records=malformed,
            distributions={
                name: timing_distribution(values) for name, values in samples.items()
            },
        ),
        input_frame_correlation="unavailable_no_explicit_batch_linkage",
        dispatch_and_commit="not_observed",
        telemetry_loss="unknown_no_drop_accounting",
    )


def capture_provenance(capture):
    """Preserve reported provenance; never derive build/source identity from HEAD."""
    fields = ("sha256", "source_id", "build_id", "geometry")
    values = {key: capture.get(key) for key in fields}
    missing = [
        key
        for key in ("sha256", "source_id", "build_id")
        if not isinstance(values[key], str) or not values[key].strip()
    ]
    geometry = values["geometry"]
    if not isinstance(geometry, (dict, list)) or not geometry:
        missing.append("geometry")
    digest = values["sha256"]
    if digest and (
        not isinstance(digest, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", digest)
    ):
        missing.append("valid_sha256")
    return dict(
        reported=values,
        missing=missing,
        verified=False,
        status="incomplete" if missing else "reported_unverified",
        comparison="unavailable_workload_and_environment_identity_not_recorded",
    )


def compare_captures(reports):
    """Explain identity differences without blessing unmatched workload evidence."""
    identities = [report["provenance"]["reported"] for report in reports]
    fields = ("sha256", "source_id", "build_id", "geometry")
    return dict(
        comparable=False,
        reason="workload_environment_and_build_compatibility_not_established",
        missing=[report["provenance"]["missing"] for report in reports],
        identity_fields={
            key: (
                "missing"
                if any(not item[key] for item in identities)
                else (
                    "equal"
                    if all(item[key] == identities[0][key] for item in identities)
                    else "different"
                )
            )
            for key in fields
        },
        speedup="not_evaluated",
    )


WORK_FIELDS = {
    "stages": (
        "project_visited",
        "retained_nodes",
        "controls_visited",
        "reveal_visited",
        "resource_visited",
        "paint_visited",
    ),
    "work": (
        "visited_nodes",
        "scroll_candidates",
        "action_candidates",
        "revealed_nodes",
    ),
    "input": ("drained", "pending", "barrier_seq"),
}


def work_metrics(lines):
    """Raw count distributions, never milliseconds or inferred missing zeroes.

    Input counters require coordinator runtime instrumentation. An empty series
    is unknown, not proof that a queue was empty or an input barrier completed.
    These unpaired distributions cannot establish click/movement ordering.
    """
    samples = {
        f"{kind}.{field}": []
        for kind, fields in WORK_FIELDS.items()
        for field in fields
    }
    for line in lines:
        for kind, fields in WORK_FIELDS.items():
            if not re.search(rf"\bbmux {kind} ", line):
                continue
            for field in fields:
                match = re.search(rf"\b{field}=(\d+)(?=\s|$)", line)
                if match:
                    samples[f"{kind}.{field}"].append(int(match[1]))
    result = {}
    for name, values in samples.items():
        values.sort()
        result[name] = dict(
            samples=len(values),
            **{
                key: values[math.ceil(len(values) * q) - 1] if values else None
                for key, q in (("p50", 0.5), ("p95", 0.95), ("max", 1))
            },
        )
    return result


def fixture_sink_mode(metadata):
    """Normalize only the known legacy recording semantics; never guess a sink."""
    mode = metadata.get("sink_mode")
    semantics = metadata.get("sink_timing_semantics")
    if (
        "sink_mode" not in metadata
        and semantics == "in_memory_copy_and_lock_not_output_blocking"
    ):
        return "recording"
    if (mode, semantics) in (
        ("recording", "in_memory_copy_and_lock_not_output_blocking"),
        ("counting", "in_memory_count_and_lock_not_output_blocking"),
    ):
        return mode
    return None


def fixture_buffering(metadata):
    """Accept explicit buffering or the exact known legacy sink semantics."""
    if "buffering_mode" not in metadata and "buffer_capacity" not in metadata:
        return "unbuffered" if fixture_sink_mode(metadata) is not None else None
    mode, capacity = metadata.get("buffering_mode"), metadata.get("buffer_capacity")
    if type(capacity) is int and (mode, capacity) in (
        ("unbuffered", 0),
        ("buffered_64k", 65536),
    ):
        return mode
    return None


def fixture_metrics(lines):
    """Validate the fixed matrix structure; missing provenance stays explicit.

    Does not assert geometry equivalence, viewport-bounded work, runtime queue
    ordering or output-blocking behavior from in-memory timing evidence.
    """
    cases = {}
    errors = []
    for line in lines:
        try:
            record = json.loads(line)
        except (ValueError, TypeError):
            continue  # Cargo/test-runner diagnostics are not fixture records.
        if not isinstance(record, dict):
            continue
        if "fixture" not in record and "frame" not in record:
            continue
        pages, mode = record.get("pages"), record.get("mode")
        if (
            type(pages) is not int
            or pages not in (1, 3, 10)
            or mode not in ("text", "cold", "warm")
        ):
            errors.append("invalid case identity")
            continue
        key = f"{pages}/{mode}"
        case = cases.setdefault(key, dict(metadata=None, frames=[]))
        if "fixture" in record:
            if case["metadata"] is not None:
                errors.append(f"{key}: duplicate metadata")
            case["metadata"] = record
        else:
            case["frames"].append(record)
    for pages in (1, 3, 10):
        for mode in ("text", "cold", "warm"):
            key = f"{pages}/{mode}"
            case = cases.get(key)
            if case is None:
                errors.append(f"{key}: missing case")
                continue
            metadata, frames = case["metadata"], case["frames"]
            if metadata is None:
                errors.append(f"{key}: missing metadata")
            elif any(
                metadata.get(field) != value
                for field, value in (
                    ("fixture", "production_albums_v1"),
                    ("wheel_count", 64),
                    ("wheel_point", [105, 20]),
                    ("viewport", [140, 40]),
                    ("sink", "in_memory_no_parser"),
                )
            ):
                errors.append(f"{key}: incompatible trace metadata")
            sink_mode = fixture_sink_mode(metadata) if metadata is not None else None
            if metadata is not None and fixture_buffering(metadata) is None:
                errors.append(f"{key}: unknown buffering mode/capacity")
            if metadata is not None and sink_mode is None:
                errors.append(f"{key}: unknown sink mode/semantics")
            if metadata is not None and sink_mode == "counting":
                if any(
                    metadata.get(field) is not None
                    for field in ("prefix_locator", "delta_locator")
                ):
                    errors.append(f"{key}: counting sink cannot have stream locators")
                if (
                    metadata.get("stream_offset_semantics")
                    != "accepted_byte_counts_no_stream"
                ):
                    errors.append(f"{key}: counting sink must label count-only offsets")
            if metadata is not None and "prefix_bytes" in metadata:
                prefix = metadata["prefix_bytes"]
                if type(prefix) is not int or prefix < 0:
                    errors.append(f"{key}: invalid prefix_bytes")
                for field, suffix in (
                    ("prefix_locator", ".init.bin"),
                    ("delta_locator", ".bin"),
                ):
                    locator = metadata.get(field)
                    expected = f"pages-{pages}-{mode}{suffix}"
                    if locator is not None and locator != expected:
                        errors.append(f"{key}: invalid {field}")
                if (metadata.get("prefix_locator") is None) != (
                    metadata.get("delta_locator") is None
                ):
                    errors.append(f"{key}: incomplete stream locators")
            if [frame.get("frame") for frame in frames] != list(range(64)) or any(
                type(frame.get("frame")) is not int for frame in frames
            ):
                errors.append(f"{key}: expected 64 ordered frames")
            previous_end = 0
            for index, frame in enumerate(frames):
                if frame.get("direction") != ("down" if index < 32 else "up"):
                    errors.append(f"{key}: wrong direction at {index}")
                byte_range = frame.get("byte_range")
                if (
                    not isinstance(byte_range, list)
                    or len(byte_range) != 2
                    or any(type(value) is not int or value < 0 for value in byte_range)
                    or byte_range[0] != previous_end
                    or byte_range[1] < byte_range[0]
                    or frame.get("bytes") != byte_range[1] - byte_range[0]
                ):
                    errors.append(f"{key}: invalid byte range at {index}")
                else:
                    previous_end = byte_range[1]
                    if sink_mode == "counting":
                        if frame.get("stream_byte_range") is not None:
                            errors.append(
                                f"{key}: counting sink cannot have stream byte ranges"
                            )
                    elif metadata is not None and "prefix_bytes" in metadata:
                        prefix = metadata["prefix_bytes"]
                        if (
                            type(prefix) is int
                            and prefix >= 0
                            and frame.get("stream_byte_range")
                            != [prefix + value for value in byte_range]
                        ):
                            errors.append(
                                f"{key}: invalid stream byte range at {index}"
                            )
                for field in ("update_us", "prepare_present_us"):
                    if type(frame.get(field)) is not int or frame[field] < 0:
                        errors.append(f"{key}: invalid {field} at {index}")
    missing_identity = [
        key
        for key, case in cases.items()
        if not case["metadata"]
        or any(
            not isinstance(case["metadata"].get(field), str)
            or not case["metadata"][field].strip()
            for field in ("source_id", "build_id")
        )
    ]
    identities = {
        (case["metadata"].get("source_id"), case["metadata"].get("build_id"))
        for case in cases.values()
        if case["metadata"]
        and all(
            isinstance(case["metadata"].get(field), str)
            for field in ("source_id", "build_id")
        )
    }
    if len(identities) > 1:
        errors.append("inconsistent source/build identity within matrix")
    sink_modes = {
        fixture_sink_mode(case["metadata"])
        for case in cases.values()
        if case["metadata"] is not None
    }
    buffering_modes = {
        fixture_buffering(case["metadata"])
        for case in cases.values()
        if case["metadata"] is not None
    }
    if len(buffering_modes) > 1:
        errors.append("incompatible buffering modes within matrix")
    if len(sink_modes) > 1:
        errors.append("incompatible sink modes within matrix")
    for case in cases.values():
        case["sink_mode"] = (
            fixture_sink_mode(case["metadata"]) if case["metadata"] else None
        )
        case["buffering_mode"] = (
            fixture_buffering(case["metadata"]) if case["metadata"] else None
        )
        frames = case["frames"]
        known_layouts = [
            frame["full_layouts"]
            for frame in frames
            if type(frame.get("full_layouts")) is int and frame["full_layouts"] >= 0
        ]
        known_projection = [
            frame["projection_used"]
            for frame in frames
            if type(frame.get("projection_used")) is bool
        ]
        complete = len(frames) == len(known_layouts) == len(known_projection) == 64
        case["fallback"] = dict(
            full_layout_samples=len(known_layouts),
            projection_samples=len(known_projection),
            full_layout_frames=(
                sum(value > 0 for value in known_layouts) if known_layouts else None
            ),
            full_layouts=sum(known_layouts) if known_layouts else None,
            zero_full_layout_frames=(
                sum(value == 0 for value in known_layouts) if known_layouts else None
            ),
            projection_frames=sum(known_projection) if known_projection else None,
            complete=complete,
            all_frames_projected_without_full_layout=(
                (all(value == 0 for value in known_layouts) and all(known_projection))
                if complete
                else None
            ),
        )
        case["work"] = {}
        for field in WORK_FIELDS["stages"]:
            values = sorted(
                frame[field]
                for frame in case["frames"]
                if type(frame.get(field)) is int and frame[field] >= 0
            )
            case["work"][field] = dict(
                samples=len(values),
                **{
                    key: values[math.ceil(len(values) * q) - 1] if values else None
                    for key, q in (("p50", 0.5), ("p95", 0.95), ("max", 1))
                },
            )
        case["timings"] = {}
        for field in (
            "update_us",
            "prepare_present_us",
            "project_us",
            "controls_us",
            "reveal_us",
            "resource_us",
            "paint_us",
            "sink_write_ns",
            "sink_max_write_ns",
            "sink_flush_ns",
        ):
            values = sorted(
                frame[field]
                for frame in case["frames"]
                if type(frame.get(field)) is int and frame[field] >= 0
            )
            scale = 1_000_000 if field.endswith("_ns") else 1000
            case["timings"][field] = dict(
                samples=len(values),
                **{
                    key: (
                        values[math.ceil(len(values) * q) - 1] / scale
                        if values
                        else None
                    )
                    for key, q in (("p50_ms", 0.5), ("p95_ms", 0.95), ("max_ms", 1))
                },
            )
    scaling = {}
    for field in WORK_FIELDS["stages"]:
        small = cases.get("1/warm", {}).get("work", {}).get(field, {})
        large = cases.get("10/warm", {}).get("work", {}).get(field, {})
        complete = (
            small.get("samples") == 64
            and large.get("samples") == 64
            and len(sink_modes) == 1
            and None not in sink_modes
            and len(buffering_modes) == 1
            and None not in buffering_modes
        )
        scaling[field] = dict(
            criterion="10-page warm p95 <= 1-page warm p95 * 1.25 + 8",
            complete=complete,
            within_bound=(
                (large["p95"] <= small["p95"] * 1.25 + 8) if complete else None
            ),
        )
    return dict(
        cases=cases,
        errors=errors,
        missing_identity=sorted(missing_identity),
        warm_work_scaling=scaling,
        structure_valid=not errors,
        provenance_complete=bool(cases) and not missing_identity,
        acceptance="not_evaluated_geometry_and_runtime_evidence_required",
    )


def analyze_capture(directory):
    """Read a saved capture with bounded binary reads; no acceptance verdict."""
    directory = Path(directory)
    analyzer = KittyAnalyzer()
    with (directory / "terminal.bin").open("rb") as stream:
        while data := stream.read(65536):
            analyzer.feed(data)
    with (directory / "app-0.log").open() as stream:
        stages = stage_metrics(stream)
    with (directory / "app-0.log").open() as stream:
        work = work_metrics(stream)
    with (directory / "app-0.log").open() as stream:
        phases = phase_metrics(stream)
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
        phases=phases,
        provenance=capture_provenance(capture),
        work=work,
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
        "--compare",
        action="store_true",
        help="classify provenance compatibility, not a speedup verdict",
    )
    parser.add_argument(
        "--replay", type=Path, help="separate fixed-byte helper CPU replay, no renderer"
    )
    parser.add_argument(
        "--fixture-log",
        type=Path,
        help="validate saved fixed-input matrix JSONL; no renderer execution",
    )
    args = parser.parse_args()
    if args.compare and (args.fixture_log or args.replay or len(args.captures) != 2):
        parser.error("--compare requires exactly two capture directories")
    if args.fixture_log and (args.replay or args.captures):
        parser.error("--fixture-log cannot be combined with captures or --replay")
    if args.fixture_log:
        with args.fixture_log.open() as stream:
            print(json.dumps(fixture_metrics(stream), indent=2))
    elif args.replay:
        print(json.dumps(replay_profile(args.replay), indent=2))
    else:
        if not args.captures:
            parser.error("provide capture directories or --replay terminal.bin")
        reports = [analyze_capture(path) for path in args.captures]
        print(
            json.dumps(
                (
                    dict(reports=reports, comparison=compare_captures(reports))
                    if args.compare
                    else reports
                ),
                indent=2,
            )
        )
