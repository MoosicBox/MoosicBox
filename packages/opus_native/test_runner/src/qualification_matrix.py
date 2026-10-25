"""Explicit native-vs-pinned-C qualification. Every mismatch/unsupported case fails strict mode.

--diagnostic permits collecting codec failures, never claims acceptance.
Inputs are deterministic but synthetic; strict real media uses --media-directory.
No fixture rewriting, C fallback, provider access or external dependencies.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import queue
import threading
import struct
import subprocess
import tempfile
import sys
import wave
import platform
import time

from qualification_inputs import (
    MANIFEST,
    accepted,
    quality_status,
    verify_build_receipt,
    verify_inputs,
    verify_reference,
)

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "libopus" / "src"))
from oracle import Oracle
import importlib.util

_reference_helpers = (
    Path(__file__).resolve().parents[2] / "libopus" / "src" / "qualification_matrix.py"
)
_spec = importlib.util.spec_from_file_location(
    "opus_reference_qualification", _reference_helpers
)
_helpers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_helpers)
digest, pcm_bytes, tone, verify_frozen = (
    _helpers.digest,
    _helpers.pcm_bytes,
    _helpers.tone,
    _helpers.verify_frozen,
)


class Native:
    """Explicit native image with fail-closed requests and bounded output/time."""

    def __init__(self, executable):
        self.manifest = json.loads(
            subprocess.check_output([str(executable), "provenance"], timeout=15)
        )
        assert self.manifest["schema"] == "moosicbox.opus.native-adapter.v1"
        self.errors = tempfile.TemporaryFile()
        self.process = subprocess.Popen(
            [str(executable)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.errors,
        )

    REQUEST_TIMEOUT = 15

    def _abort(self):
        """End a failed exchange before closing pipes or allowing another request."""
        self._failed = True
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=2)

    def request(self, op, **fields):
        if getattr(self, "_failed", False):
            raise RuntimeError("native transport is unusable after a failed request")
        payload = (
            json.dumps(dict(op=op, **fields), separators=(",", ":")) + "\n"
        ).encode()
        if len(payload) > 1048576:
            raise ValueError("native request exceeds 1 MiB")
        result = queue.Queue(maxsize=1)

        def exchange():
            # Windows selectors cannot register subprocess pipes. A dedicated
            # bounded exchange also covers blocked writes and partial-line reads.
            try:
                self.process.stdin.write(payload)
                self.process.stdin.flush()
                reply = self.process.stdout.readline(1048577)
                result.put((reply, None))
            except Exception as error:
                result.put((None, error))

        worker = threading.Thread(
            target=exchange, name="opus-native-exchange", daemon=True
        )
        worker.start()
        try:
            try:
                reply, error = result.get(timeout=self.REQUEST_TIMEOUT)
            except queue.Empty as error:
                raise TimeoutError(
                    "native request exceeded bounded exchange deadline"
                ) from error
            if error is not None:
                raise RuntimeError("native request pipe failed") from error
            if not reply or len(reply) > 1048576 or not reply.endswith(b"\n"):
                raise RuntimeError(
                    "native rejected request, truncated reply or exceeded output bound"
                )
            return json.loads(reply)
        except Exception:
            # Killing/reaping unblocks both Windows and Unix synchronous pipe IO.
            self._abort()
            raise
        finally:
            worker.join(timeout=2)

    def __enter__(self):
        return self

    def __exit__(self, *unused):
        try:
            self.process.stdin.close()
        except (BrokenPipeError, OSError):
            pass
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.process.stdout.close()
        self.errors.close()


def frozen(native_executable):
    root = Path(__file__).resolve().parents[2] / "test_vectors" / "frozen"
    verify_frozen(root)
    manifest_bytes = (root / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    rows = []
    for case in manifest["cases"]:
        row = dict(
            id=case["id"],
            status="pass",
            packets=0,
            mismatched_samples=0,
            max_abs_error=0,
            actual_pcm_sha256=None,
        )
        h = hashlib.sha256()
        try:
            with Native(native_executable) as native:
                for index in range(case["packet_count"]):
                    directory = root / "corpus" / case["path"]
                    if index:
                        directory /= f"packet_{index:03}"
                    meta = json.loads((directory / "meta.json").read_bytes())
                    if not index:
                        native.request(
                            "decoder",
                            rate=meta["sample_rate"],
                            channels=meta["channels"],
                        )
                    packet = list((directory / "packet.bin").read_bytes())
                    expected_bytes = (directory / "expected.pcm").read_bytes()
                    expected = struct.unpack(
                        "<" + "h" * (len(expected_bytes) // 2), expected_bytes
                    )
                    output = native.request(
                        "decode", frame_size=meta["frame_samples"], packet=packet
                    )
                    actual = output["pcm"]
                    assert output["samples"] == meta["frame_samples"] and len(
                        actual
                    ) == len(expected)
                    errors = [abs(x - y) for x, y in zip(actual, expected)]
                    row["mismatched_samples"] += sum(e != 0 for e in errors)
                    row["max_abs_error"] = max(
                        row["max_abs_error"], max(errors, default=0)
                    )
                    h.update(pcm_bytes(actual))
                    row["packets"] += 1
                row["actual_pcm_sha256"] = h.hexdigest()
                if row["mismatched_samples"]:
                    row["status"] = "pcm_mismatch"
        except (RuntimeError, AssertionError, BrokenPipeError, TimeoutError) as error:
            row["status"] = "codec_failure"
            row["error"] = str(error)[:300]
        rows.append(row)
    return dict(manifest_sha256=digest(manifest_bytes), cases=rows)


def packet_mode(packet):
    if not packet:
        raise AssertionError("empty encoded Opus packet")
    config = packet[0] >> 3
    return 1000 if config < 12 else (1001 if config < 16 else 1002)


def compare(native_executable, reference_executable, frames):
    rows = []
    with Oracle(reference_executable) as reference:
        for rate in (8000, 12000, 16000, 24000, 48000):
            for channels in (1, 2):
                for mode in (1000, 1001, 1002):
                    if mode == 1001 and rate < 24000:
                        continue
                    durations = (
                        (10, 20, 40, 60)
                        if mode == 1000
                        else ((10, 20) if mode == 1001 else (2.5, 5, 10, 20))
                    )
                    for ms in durations:
                        size = int(rate * ms / 1000)
                        bitrate = (
                            96000
                            if mode == 1000
                            else (128000 if mode == 1001 else 64000 * channels)
                        )
                        row = dict(
                            rate=rate,
                            channels=channels,
                            mode=mode,
                            duration_ms=ms,
                            bitrate=bitrate,
                            requested_frames=frames,
                            decoded_frames=0,
                            encoded_frames=0,
                            mismatched_samples=0,
                            max_abs_error=0,
                            status="pass",
                            actual_modes=[],
                            output_samples_per_channel=[],
                        )
                        try:
                            with Native(native_executable) as native:
                                for engine in (reference, native):
                                    engine.request(
                                        "encoder", rate=rate, channels=channels
                                    )
                                    engine.request(
                                        "decoder", rate=rate, channels=channels
                                    )
                                    engine.request(
                                        "configure",
                                        controls=[
                                            [11002, mode],
                                            [4002, bitrate],
                                            [4006, 0],
                                        ],
                                    )
                                encoder_hashes = []
                                for replay in range(2):
                                    h = hashlib.sha256()
                                    # Independent decoder states prevent cross-direction contamination.
                                    for engine in (reference, native):
                                        engine.request("reset", state="decoder")
                                    for frame in range(frames):
                                        pcm = tone(rate, channels, size, frame)
                                        c_packet = reference.request(
                                            "encode", frame_size=size, pcm=pcm
                                        )["packet"]
                                        expected = reference.request(
                                            "decode", frame_size=size, packet=c_packet
                                        )["pcm"]
                                        actual = native.request(
                                            "decode", frame_size=size, packet=c_packet
                                        )["pcm"]
                                        assert (
                                            len(expected)
                                            == len(actual)
                                            == size * channels
                                        )
                                        errors = [
                                            abs(x - y) for x, y in zip(expected, actual)
                                        ]
                                        row["mismatched_samples"] += sum(
                                            e != 0 for e in errors
                                        )
                                        row["max_abs_error"] = max(
                                            row["max_abs_error"], max(errors, default=0)
                                        )
                                        row["decoded_frames"] += 1
                                    # Reset the C decoder before testing the native encoder stream.
                                    reference.request("reset", state="decoder")
                                    for frame in range(frames):
                                        pcm = tone(rate, channels, size, frame)
                                        packet = native.request(
                                            "encode", frame_size=size, pcm=pcm
                                        )["packet"]
                                        actual_mode = packet_mode(packet)
                                        if actual_mode not in row["actual_modes"]:
                                            row["actual_modes"].append(actual_mode)
                                        assert (
                                            actual_mode == mode
                                        ), "native TOC mode differs from requested mode"
                                        decoded = reference.request(
                                            "decode", frame_size=size, packet=packet
                                        )
                                        if (
                                            decoded["samples"]
                                            not in row["output_samples_per_channel"]
                                        ):
                                            row["output_samples_per_channel"].append(
                                                decoded["samples"]
                                            )
                                        assert (
                                            decoded["samples"] == size
                                            and len(decoded["pcm"]) == size * channels
                                        )
                                        h.update(bytes(packet))
                                        h.update(pcm_bytes(decoded["pcm"]))
                                        row["encoded_frames"] += 1
                                    encoder_hashes.append(h.hexdigest())
                                    for engine in (reference, native):
                                        engine.request("reset", state="encoder")
                                assert (
                                    encoder_hashes[0] == encoder_hashes[1]
                                ), "native encoder not deterministic after reset"
                                row["native_packet_and_c_pcm_sha256"] = encoder_hashes[
                                    0
                                ]
                                row["encoder_replay_exact"] = True
                                if row["mismatched_samples"]:
                                    row["status"] = "pcm_mismatch"
                        except (
                            RuntimeError,
                            AssertionError,
                            BrokenPipeError,
                            TimeoutError,
                        ) as error:
                            row["status"] = (
                                "unsupported_requested_config"
                                if "Unsupported(" in str(error)
                                else "codec_failure"
                            )
                            row["error"] = str(error)[:300]
                        rows.append(row)
    return rows


def quality(native_executable, reference_executable, filename, label):
    with wave.open(str(filename), "rb") as wav:
        rate, channels = wav.getframerate(), wav.getnchannels()
        assert wav.getsampwidth() == 2 and wav.getcomptype() == "NONE"
        assert rate in (8000, 12000, 16000, 24000, 48000) and channels in (1, 2)
        raw = wav.readframes(rate * 30)
    source = list(struct.unpack("<" + "h" * (len(raw) // 2), raw))
    size = rate // 50
    assert len(source) >= size * channels * 10 and any(source)
    rows = []
    # Opus hybrid requires a 24/48-kHz API rate; 8-kHz speech cannot exercise it.
    modes = (0, 1000, 1001, 1002) if rate >= 24000 else (0, 1000, 1002)
    for mode in modes:
        for bitrate in sorted(
            set((16000 * channels, 32000 * channels, 64000 * channels, 128000, 256000))
        ):
            row = dict(
                mode=mode,
                bitrate=bitrate,
                status="pass",
                actual_modes=[],
                output_samples_per_channel=[],
            )
            try:
                with Native(native_executable) as native, Oracle(
                    reference_executable
                ) as reference:
                    native.request("encoder", rate=rate, channels=channels)
                    native.request(
                        "configure",
                        controls=([[11002, mode]] if mode else [])
                        + [[4002, bitrate], [4006, 0]],
                    )
                    reference.request("decoder", rate=rate, channels=channels)
                    native.request("decoder", rate=rate, channels=channels)
                    decoded = []
                    maximum_parity_error = 0
                    encoded_bytes = 0
                    delay = None
                    for offset in range(
                        0, len(source) + size * channels * 2, size * channels
                    ):
                        block = source[offset : offset + size * channels]
                        block += [0] * (size * channels - len(block))
                        packet = native.request("encode", frame_size=size, pcm=block)[
                            "packet"
                        ]
                        actual_mode = packet_mode(packet)
                        if actual_mode not in row["actual_modes"]:
                            row["actual_modes"].append(actual_mode)
                        assert actual_mode in (
                            1000,
                            1001,
                            1002,
                        ), "invalid native TOC mode"
                        if mode:
                            assert (
                                actual_mode == mode
                            ), "native TOC mode differs from requested mode"
                        encoded_bytes += len(packet)
                        if delay is None:
                            delay = native.request("lookahead")["samples"]
                        output = reference.request(
                            "decode", frame_size=size, packet=packet
                        )
                        if output["samples"] not in row["output_samples_per_channel"]:
                            row["output_samples_per_channel"].append(output["samples"])
                        assert output["samples"] == size
                        actual = native.request(
                            "decode", packet=packet, frame_size=size
                        )
                        assert actual["samples"] == output["samples"]
                        assert (
                            len(actual["pcm"]) == len(output["pcm"]) == size * channels
                        )
                        maximum_parity_error = max(
                            maximum_parity_error,
                            max(
                                (
                                    abs(a - b)
                                    for a, b in zip(actual["pcm"], output["pcm"])
                                ),
                                default=0,
                            ),
                        )
                        decoded.extend(output["pcm"])
                    aligned = decoded[delay * channels : delay * channels + len(source)]
                    assert len(aligned) == len(source)
                    energy = sum(x * x for x in source)
                    error = sum((x - y) ** 2 for x, y in zip(source, aligned))
                    row.update(
                        max_native_reference_error=maximum_parity_error,
                        snr_db=10 * math.log10(energy / max(1, error)),
                        lookahead_samples=delay,
                        actual_bits_per_second=encoded_bytes
                        * 8
                        * rate
                        / (len(source) // channels + size * 2),
                        clipping_samples=sum(abs(x) >= 32767 for x in aligned),
                        decoded_sha256=digest(pcm_bytes(aligned)),
                    )
                    # Waveform gates are not perceptual transparency certification.
                    row["status"] = quality_status(row["snr_db"], maximum_parity_error)
            except (
                RuntimeError,
                AssertionError,
                BrokenPipeError,
                TimeoutError,
            ) as error:
                row.update(status="codec_failure", error=str(error)[:300])
            rows.append(row)
    return dict(
        label=label,
        source_sha256=digest(raw),
        rate=rate,
        channels=channels,
        metrics=rows,
        limitation="SNR is not a perceptual score; no listening/PESQ/POLQA claim",
    )


def silence_matrix(native_executable, reference_executable):
    """Fresh-state and sustained encoded silence, with native/C parity and duration."""
    rows = []
    for rate, modes in ((16000, (0, 1000, 1002)), (48000, (0, 1001, 1002))):
        for channels in (1, 2):
            for mode in modes:
                bitrates = (
                    (32000, 64000, 128000) if channels == 2 else (16000, 32000, 128000)
                )
                for bitrate in bitrates:
                    row = dict(
                        rate=rate,
                        channels=channels,
                        mode=mode,
                        bitrate=bitrate,
                        frames=12,
                        status="pass",
                        max_abs_sample=0,
                        max_native_reference_error=0,
                        actual_modes=[],
                    )
                    try:
                        with Native(native_executable) as native, Oracle(
                            reference_executable
                        ) as reference:
                            native.request("encoder", rate=rate, channels=channels)
                            native.request(
                                "configure",
                                controls=(
                                    ([[11002, mode]] if mode else [])
                                    + [[4002, bitrate], [4006, 0]]
                                ),
                            )
                            native.request("decoder", rate=rate, channels=channels)
                            reference.request("decoder", rate=rate, channels=channels)
                            size = rate // 50
                            for _ in range(row["frames"]):
                                packet = native.request(
                                    "encode",
                                    frame_size=size,
                                    pcm=[0] * (size * channels),
                                )["packet"]
                                actual_mode = packet_mode(packet)
                                assert actual_mode in (1000, 1001, 1002)
                                assert not mode or actual_mode == mode
                                if actual_mode not in row["actual_modes"]:
                                    row["actual_modes"].append(actual_mode)
                                actual = native.request(
                                    "decode", frame_size=size, packet=packet
                                )
                                expected = reference.request(
                                    "decode", frame_size=size, packet=packet
                                )
                                assert actual["samples"] == expected["samples"] == size
                                assert (
                                    len(actual["pcm"])
                                    == len(expected["pcm"])
                                    == size * channels
                                )
                                row["max_abs_sample"] = max(
                                    row["max_abs_sample"],
                                    max(
                                        abs(x) for x in actual["pcm"] + expected["pcm"]
                                    ),
                                )
                                row["max_native_reference_error"] = max(
                                    row["max_native_reference_error"],
                                    max(
                                        abs(a - b)
                                        for a, b in zip(actual["pcm"], expected["pcm"])
                                    ),
                                )
                            if row["max_abs_sample"] > 1:
                                row["status"] = "silence_above_1lsb"
                            if row["max_native_reference_error"] > 2:
                                row["status"] = "native_reference_error_above_2lsb"
                    except (
                        RuntimeError,
                        AssertionError,
                        BrokenPipeError,
                        TimeoutError,
                        KeyError,
                    ) as error:
                        row.update(status="codec_failure", error=str(error)[:300])
                    rows.append(row)
    return rows


def gain_matrix(native_executable, reference_executable):
    rows = []
    with Oracle(reference_executable) as reference:
        for channels in (1, 2):
            reference.request("encoder", rate=48000, channels=channels)
            reference.request(
                "configure", controls=[[11002, 1002], [4002, 128000], [4006, 0]]
            )
            packets = [
                reference.request(
                    "encode", frame_size=960, pcm=tone(48000, channels, 960, frame)
                )["packet"]
                for frame in range(4)
            ]
            for gain in (-32768, -1536, 0, 1536, 32767):
                row = dict(channels=channels, gain_q8=gain, status="pass")
                try:
                    with Native(native_executable) as native:
                        for engine in (reference, native):
                            engine.request("decoder", rate=48000, channels=channels)
                            engine.request("decoder_configure", controls=[[4034, gain]])
                        h = hashlib.sha256()
                        for replay in range(2):
                            for packet in packets:
                                expected = reference.request(
                                    "decode", frame_size=960, packet=packet
                                )["pcm"]
                                actual = native.request(
                                    "decode", frame_size=960, packet=packet
                                )["pcm"]
                                assert actual == expected, "nonzero gain PCM mismatch"
                                h.update(pcm_bytes(actual))
                            for engine in (reference, native):
                                engine.request("reset", state="decoder")
                        row["pcm_sha256"] = h.hexdigest()
                except (
                    RuntimeError,
                    AssertionError,
                    BrokenPipeError,
                    TimeoutError,
                ) as error:
                    row.update(status="codec_failure", error=str(error)[:300])
                rows.append(row)
    return rows


def decode_matrix(native_executable, reference_executable, frames):
    """Qualify decoding independently of whether the native encoder supports that configuration."""
    rows = []
    with Oracle(reference_executable) as reference:
        for rate in (8000, 12000, 16000, 24000, 48000):
            for channels in (1, 2):
                for mode in (1000, 1001, 1002):
                    if mode == 1001 and rate < 24000:
                        continue
                    for ms in (
                        (10, 20, 40, 60)
                        if mode == 1000
                        else ((10, 20) if mode == 1001 else (2.5, 5, 10, 20))
                    ):
                        size = int(rate * ms / 1000)
                        row = dict(
                            rate=rate,
                            channels=channels,
                            mode=mode,
                            duration_ms=ms,
                            status="pass",
                            mismatched_samples=0,
                            max_abs_error=0,
                            actual_modes=[],
                            output_samples_per_channel=[],
                        )
                        try:
                            with Native(native_executable) as native:
                                reference.request(
                                    "encoder", rate=rate, channels=channels
                                )
                                reference.request(
                                    "configure",
                                    controls=[
                                        [11002, mode],
                                        [4002, 64000 * channels],
                                        [4006, 0],
                                    ],
                                )
                                reference.request(
                                    "decoder", rate=rate, channels=channels
                                )
                                native.request("decoder", rate=rate, channels=channels)
                                h = hashlib.sha256()
                                for frame in range(frames):
                                    packet = reference.request(
                                        "encode",
                                        frame_size=size,
                                        pcm=tone(rate, channels, size, frame),
                                    )["packet"]
                                    actual_mode = packet_mode(packet)
                                    if actual_mode not in row["actual_modes"]:
                                        row["actual_modes"].append(actual_mode)
                                    expected_output = reference.request(
                                        "decode", frame_size=size, packet=packet
                                    )
                                    expected = expected_output["pcm"]
                                    actual_output = native.request(
                                        "decode", frame_size=size, packet=packet
                                    )
                                    actual = actual_output["pcm"]
                                    if (
                                        actual_output["samples"]
                                        not in row["output_samples_per_channel"]
                                    ):
                                        row["output_samples_per_channel"].append(
                                            actual_output["samples"]
                                        )
                                    assert (
                                        actual_output["samples"]
                                        == expected_output["samples"]
                                        == size
                                    )
                                    assert (
                                        len(actual) == len(expected) == size * channels
                                    )
                                    errors = [
                                        abs(x - y) for x, y in zip(actual, expected)
                                    ]
                                    row["mismatched_samples"] += sum(
                                        x != 0 for x in errors
                                    )
                                    row["max_abs_error"] = max(
                                        row["max_abs_error"], max(errors, default=0)
                                    )
                                    h.update(pcm_bytes(actual))
                                row["pcm_sha256"] = h.hexdigest()
                                if row["mismatched_samples"]:
                                    row["status"] = "pcm_mismatch"
                        except (
                            RuntimeError,
                            AssertionError,
                            BrokenPipeError,
                            TimeoutError,
                        ) as error:
                            row.update(status="codec_failure", error=str(error)[:300])
                        rows.append(row)
    return rows


def long_stream(native_executable, reference_executable, frames):
    """One-minute stereo stream by default, with burst PLC and deterministic replay."""
    row = dict(
        frames=frames, duration_seconds=frames / 50, status="pass", loss_frames=0
    )
    hashes = []
    try:
        for replay in range(2):
            with Native(native_executable) as native, Oracle(
                reference_executable
            ) as reference:
                reference.request("encoder", rate=48000, channels=2)
                reference.request(
                    "configure", controls=[[11002, 1002], [4002, 128000], [4006, 0]]
                )
                native.request("decoder", rate=48000, channels=2)
                h = hashlib.sha256()
                for frame in range(frames):
                    packet = reference.request(
                        "encode", frame_size=960, pcm=tone(48000, 2, 960, frame)
                    )["packet"]
                    lost = frame % 97 in (30, 31, 32)
                    decoded = native.request(
                        "decode", frame_size=960, packet=None if lost else packet
                    )
                    assert decoded["samples"] == 960 and len(decoded["pcm"]) == 1920
                    h.update(pcm_bytes(decoded["pcm"]))
                    if replay == 0 and lost:
                        row["loss_frames"] += 1
                hashes.append(h.hexdigest())
        assert hashes[0] == hashes[1], "long stream replay differs"
        row.update(replay_exact=True, sha256=hashes[0])
    except (RuntimeError, AssertionError, BrokenPipeError, TimeoutError) as error:
        row.update(status="codec_failure", error=str(error)[:300])
    return row


def adversarial(native_executable, cases):
    import random

    rng = random.Random(2457)
    counts = dict(accepted=0, rejected=0, crashes=0, timeouts=0)
    h = hashlib.sha256()
    for index in range(cases):
        packet = [rng.randrange(256) for _ in range(index % 97)]
        text = json.dumps(dict(op="decoder", rate=48000, channels=2)) + "\n"
        text += json.dumps(dict(op="decode", frame_size=5760, packet=packet)) + "\n"
        try:
            result = subprocess.run(
                [str(native_executable)],
                input=text.encode(),
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                timeout=15,
            )
            if result.returncode == 0:
                output = json.loads(result.stdout.splitlines()[1])
                assert (
                    0 <= output["samples"] <= 5760
                    and len(output["pcm"]) == output["samples"] * 2
                )
                counts["accepted"] += 1
            elif result.returncode == 1:
                counts["rejected"] += 1
            else:
                counts["crashes"] += 1
            h.update(bytes(packet))
            h.update(result.stdout)
        except subprocess.TimeoutExpired:
            counts["timeouts"] += 1
    return dict(
        cases=cases,
        seed=2457,
        sha256=h.hexdigest(),
        status=(
            "pass"
            if not counts["crashes"] and not counts["timeouts"]
            else "codec_failure"
        ),
        **counts,
    )


def native_source_identity():
    root = Path(__file__).resolve().parents[2]
    h = hashlib.sha256()
    paths = list((root / "src").rglob("*.rs"))
    paths += list((root / "test_runner" / "examples").rglob("*.rs"))
    workspace = root.parents[1]
    for path in sorted(
        paths
        + [
            root / "Cargo.toml",
            root / "test_runner" / "Cargo.toml",
            workspace / "Cargo.toml",
            workspace / "Cargo.lock",
        ]
    ):
        h.update(str(path.relative_to(workspace)).encode() + b"\0")
        h.update(path.read_bytes())
    return h.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--long-frames", type=int, default=3000)
    parser.add_argument("--adversarial-cases", type=int, default=128)
    parser.add_argument("--transient-wav", type=Path)
    parser.add_argument("--frames", type=int, default=12)
    parser.add_argument("--diagnostic", action="store_true")
    parser.add_argument("--speech-wav", type=Path)
    parser.add_argument("--music-wav", type=Path)
    parser.add_argument(
        "--media-directory",
        type=Path,
        help="directory containing checksum-reviewed manifest WAVs",
    )
    parser.add_argument(
        "--build-receipt",
        type=Path,
        help="coherent source-before/after and executable build identities",
    )
    args = parser.parse_args()
    started = time.perf_counter()
    inputs = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if args.media_directory:
        if args.speech_wav or args.music_wav:
            parser.error(
                "--media-directory cannot be combined with individual WAV paths"
            )
        inputs, media = verify_inputs(args.media_directory)
        args.speech_wav, args.music_wav = media["speech"], media["music"]
    if not args.diagnostic and (not args.media_directory or not args.build_receipt):
        parser.error(
            "strict qualification requires --media-directory and --build-receipt"
        )
    assert 1 <= args.frames <= 3000 and 1 <= args.long_frames <= 30000
    assert 0 <= args.adversarial_cases <= 4096
    native, reference = args.native.resolve(strict=True), args.reference.resolve(
        strict=True
    )
    source_before = native_source_identity()
    native_hash = digest(native.read_bytes())
    reference_hash = digest(reference.read_bytes())
    build_receipt = (
        verify_build_receipt(
            args.build_receipt, source_before, native_hash, reference_hash
        )
        if args.build_receipt
        else None
    )
    with Oracle(reference) as oracle:
        reference_manifest = oracle.manifest
        verify_reference(reference_manifest, inputs)
    report = dict(
        schema="moosicbox.opus.native-qualification.v1",
        native_sha256=native_hash,
        reference_provenance=reference_manifest,
        input_manifest_sha256=digest(MANIFEST.read_bytes()),
        build_receipt=build_receipt,
        platform=dict(
            system=platform.system(),
            release=platform.release(),
            machine=platform.machine(),
            python=platform.python_version(),
            other_platforms_executed=False,
        ),
        reference_sha256=reference_hash,
        runtime_source_sha256=source_before,
        script_sha256=digest(Path(__file__).read_bytes()),
        frozen=frozen(native),
        matrix=compare(native, reference, args.frames),
        silence_matrix=silence_matrix(native, reference),
        gain_matrix=gain_matrix(native, reference),
        decoder_matrix=decode_matrix(native, reference, args.frames),
        long_stream=long_stream(native, reference, args.long_frames),
        adversarial=adversarial(native, args.adversarial_cases),
        real_media=[],
    )
    for label in ("speech", "music"):
        filename = getattr(args, label + "_wav")
        if filename:
            report["real_media"].append(quality(native, reference, filename, label))
            # Persist incremental evidence even if a later media case hits the outer execution bound.
            args.output.with_suffix(".partial.json").write_text(
                json.dumps(report, sort_keys=True, indent=2) + "\n"
            )
    if args.transient_wav:
        report["transient_diagnostic"] = quality(
            native, reference, args.transient_wav, "transient-not-speech-or-music"
        )
    rows = (
        report["frozen"]["cases"]
        + report["matrix"]
        + report["silence_matrix"]
        + report["gain_matrix"]
        + report["decoder_matrix"]
        + [report["long_stream"], report["adversarial"]]
    )
    rows += [r for media in report["real_media"] for r in media["metrics"]]
    failures = sum(row["status"] != "pass" for row in rows)
    # Explicit real media is required for strict qualification; missing media never silently passes.
    media_missing = len(report["real_media"]) != 2 or args.media_directory is None
    if args.media_directory:
        # Recheck whole-file media identities, not only the consumed PCM prefix.
        verify_inputs(args.media_directory)
    source_after = native_source_identity()
    artifact_stable = native_hash == digest(
        native.read_bytes()
    ) and reference_hash == digest(reference.read_bytes())
    report.update(
        runtime_source_after_sha256=source_after,
        runtime_source_changed=source_before != source_after,
        artifact_stable=artifact_stable,
    )
    report.update(
        failures=failures,
        media_missing=media_missing,
        accepted=accepted(
            failures,
            media_missing,
            artifact_stable,
            source_before != source_after,
            build_receipt is not None,
        ),
        performance=dict(
            wall_seconds=time.perf_counter() - started,
            scope="whole harness including IPC and C; not codec throughput",
            codec_throughput_verified=False,
            memory_budget_verified=False,
        ),
        limitation="no perceptual or cross-platform acceptance; exact decoder comparisons are uncompensated",
    )
    payload = json.dumps(report, sort_keys=True, indent=2) + "\n"
    args.output.write_text(payload)
    print(
        json.dumps(
            dict(
                output=str(args.output),
                sha256=digest(payload.encode()),
                failures=failures,
                media_missing=media_missing,
                accepted=report["accepted"],
            )
        )
    )
    return 0 if args.diagnostic or report["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
