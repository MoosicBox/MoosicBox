"""Opt-in bounded live reference qualification; never rewrites frozen fixtures.

python3 -B qualification_matrix.py /absolute/reference_oracle [--output /tmp/report.json]
Optional --speech-wav/--music-wav accept real, uncompressed signed 16-bit WAV.
Synthetic results are diagnostics, not perceptual quality certification.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import struct
import sys
import wave

sys.path.insert(0, str(Path(__file__).resolve().parent))
from oracle import Oracle


def digest(data):
    return hashlib.sha256(data).hexdigest()


def pcm_bytes(pcm):
    return struct.pack("<" + "h" * len(pcm), *pcm)


def verify_frozen(root):
    manifest_hash = digest((root / "manifest.json").read_bytes())
    if (
        manifest_hash
        != "8361b967ee0ac2fc4385e3b52d46dafd96921077aea3b23928b45f3d6251e8c3"
    ):
        raise ValueError("frozen manifest identity changed")
    for line in (root / "SHA256SUMS").read_text().splitlines():
        expected, relative = line.split("  ", 1)
        path = (root / relative).resolve(strict=True)
        if not path.is_relative_to(root.resolve()):
            raise ValueError("frozen checksum path escaped root")
        if digest(path.read_bytes()) != expected:
            raise ValueError("frozen file changed: " + relative)


def frozen(executable):
    root = Path(__file__).resolve().parents[2] / "test_vectors" / "frozen"
    verify_frozen(root)
    manifest_bytes = (root / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    packets = samples = 0
    aggregate = hashlib.sha256()
    with Oracle(executable) as oracle:
        for case in manifest["cases"]:
            base = root / "corpus" / case["path"]
            dirs = [
                base if i == 0 else base / f"packet_{i:03}"
                for i in range(case["packet_count"])
            ]
            assert len(dirs) == case["packet_count"], case["id"]
            meta = json.loads((dirs[0] / "meta.json").read_text())
            oracle.request(
                "decoder", rate=meta["sample_rate"], channels=meta["channels"]
            )
            for directory in dirs:
                info = json.loads((directory / "meta.json").read_text())
                packet = (directory / "packet.bin").read_bytes()
                expected = (directory / "expected.pcm").read_bytes()
                output = oracle.request(
                    "decode", frame_size=info["frame_samples"], packet=list(packet)
                )
                actual = pcm_bytes(output["pcm"])
                assert (
                    actual == expected
                ), f"frozen mismatch: {case['id']} {directory.name}"
                assert output["samples"] == info["frame_samples"]
                aggregate.update(actual)
                packets += 1
                samples += len(output["pcm"])
    assert len(manifest["cases"]) == 43 and packets == 58
    return dict(
        cases=43,
        packets=packets,
        interleaved_samples=samples,
        manifest_sha256=digest(manifest_bytes),
        pcm_sha256=aggregate.hexdigest(),
        exact=True,
    )


def tone(rate, channels, size, frame):
    return [
        int(10000 * math.sin(2 * math.pi * (220 + 137 * c) * (frame * size + i) / rate))
        for i in range(size)
        for c in range(channels)
    ]


def matrix(executable, frames):
    """Independent live generated cases, with reset/replay and sustained loss checks."""
    rows = []
    with Oracle(executable) as oracle:
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
                        oracle.request("encoder", rate=rate, channels=channels)
                        oracle.request("decoder", rate=rate, channels=channels)
                        oracle.request(
                            "configure",
                            controls=[
                                [11002, mode],
                                [4002, 32000 * channels],
                                [4006, 0],
                            ],
                        )
                        hashes = []
                        for replay in range(2):
                            h = hashlib.sha256()
                            for frame in range(frames):
                                source = tone(rate, channels, size, frame)
                                packet = oracle.request(
                                    "encode", frame_size=size, pcm=source
                                )["packet"]
                                output = oracle.request(
                                    "decode", frame_size=size, packet=packet
                                )
                                assert output["samples"] == size
                                assert len(output["pcm"]) == size * channels
                                h.update(bytes(packet))
                                h.update(pcm_bytes(output["pcm"]))
                            for _ in range(8):
                                loss = oracle.request(
                                    "decode", frame_size=size, packet=None
                                )
                                assert loss["samples"] == size
                                h.update(pcm_bytes(loss["pcm"]))
                            hashes.append(h.hexdigest())
                            oracle.request("reset", state="encoder")
                            oracle.request("reset", state="decoder")
                        assert hashes[0] == hashes[1], (rate, channels, mode, ms)
                        rows.append(
                            dict(
                                rate=rate,
                                channels=channels,
                                mode=mode,
                                duration_ms=ms,
                                frames=frames,
                                loss_frames=8,
                                replay_exact=True,
                                sha256=hashes[0],
                            )
                        )
    return rows


def multistream(executable):
    rows = []
    layouts = [
        (1, 1, 0, [0]),
        (2, 1, 1, [0, 1]),
        (6, 4, 2, [0, 4, 1, 2, 3, 5]),
        (4, 2, 1, [2, 0, 1, 255]),
    ]
    with Oracle(executable) as oracle:
        for channels, streams, coupled, mapping in layouts:
            spec = dict(
                rate=48000,
                channels=channels,
                streams=streams,
                coupled_streams=coupled,
                mapping=mapping,
            )
            oracle.request("multistream_encoder", **spec)
            oracle.request("multistream_decoder", **spec)
            hashes = []
            first = None
            for _ in range(2):
                h = hashlib.sha256()
                for frame in range(12):
                    packet = oracle.request(
                        "multistream_encode",
                        frame_size=960,
                        pcm=tone(48000, channels, 960, frame),
                    )["packet"]
                    if frame == 0:
                        first = packet
                    decoded = oracle.request(
                        "multistream_decode", frame_size=960, packet=packet
                    )
                    assert (
                        decoded["samples"] == 960
                        and len(decoded["pcm"]) == 960 * channels
                    )
                    for c, mapped in enumerate(mapping):
                        if mapped == 255:
                            assert not any(decoded["pcm"][c::channels])
                    h.update(bytes(packet))
                    h.update(pcm_bytes(decoded["pcm"]))
                oracle.request("multistream_decode", frame_size=960, packet=None)
                hashes.append(h.hexdigest())
                for state in ("encoder", "decoder"):
                    oracle.request("multistream_reset", state=state)
            assert hashes[0] == hashes[1]
            # Gain applies to every stream. Reset retains the configured gain.
            oracle.request("multistream_gain", gain_q8=-1536)
            quiet = oracle.request("multistream_decode", frame_size=960, packet=first)[
                "pcm"
            ]
            oracle.request("multistream_reset", state="decoder")
            oracle.request("multistream_gain", gain_q8=0)
            normal = oracle.request("multistream_decode", frame_size=960, packet=first)[
                "pcm"
            ]
            assert sum(x * x for x in quiet) < sum(x * x for x in normal)
            rows.append(
                dict(
                    channels=channels,
                    streams=streams,
                    coupled_streams=coupled,
                    mapping=mapping,
                    replay_exact=True,
                    gain_checked=True,
                    sha256=hashes[0],
                )
            )
    return rows


def adversarial(executable, count):
    """Each malformed packet is isolated because the protocol intentionally fails closed."""
    import subprocess

    rng = random.Random(2457)
    outcomes = {"accepted": 0, "rejected": 0}
    h = hashlib.sha256()
    for i in range(count):
        packet = [rng.randrange(256) for _ in range(i % 97)]
        text = json.dumps(dict(op="decoder", rate=48000, channels=2)) + "\n"
        text += json.dumps(dict(op="decode", frame_size=5760, packet=packet)) + "\n"
        result = subprocess.run(
            [str(executable)], input=text, text=True, capture_output=True, timeout=15
        )
        assert result.returncode in (0, 1), f"oracle crashed: {result.returncode}"
        lines = result.stdout.splitlines()
        assert lines and json.loads(lines[0]) == {"ok": True}
        if result.returncode == 0:
            output = json.loads(lines[1])
            assert 0 <= output["samples"] <= 5760
            assert len(output["pcm"]) == output["samples"] * 2
            outcomes["accepted"] += 1
        else:
            outcomes["rejected"] += 1
        h.update(bytes(packet))
        h.update(result.stdout.encode())
    return dict(seed=2457, cases=count, sha256=h.hexdigest(), **outcomes)


def quality(executable, filename, label):
    """Reference encode/decode with explicit codec delay and bounded waveform metrics."""
    with wave.open(str(filename), "rb") as wav:
        rate, channels = wav.getframerate(), wav.getnchannels()
        assert wav.getsampwidth() == 2 and wav.getcomptype() == "NONE"
        assert rate in (8000, 12000, 16000, 24000, 48000) and channels in (1, 2)
        raw = wav.readframes(rate * 30)
    pcm = list(struct.unpack("<" + "h" * (len(raw) // 2), raw))
    size = rate // 50
    assert len(pcm) >= size * channels * 10, "need at least 200 ms of real media"
    rows = []
    for bitrate in (16000 * channels, 32000 * channels, 64000 * channels):
        decoded = []
        packet_bytes = 0
        with Oracle(executable) as oracle:
            oracle.request("encoder", rate=rate, channels=channels)
            oracle.request("decoder", rate=rate, channels=channels)
            oracle.request("configure", controls=[[4002, bitrate]])
            # Public Opus default audio lookahead is 6.5 ms; append drain frames.
            delay = rate * 65 // 10000
            for offset in range(0, len(pcm) + size * channels, size * channels):
                block = pcm[offset : offset + size * channels]
                block += [0] * (size * channels - len(block))
                packet = oracle.request("encode", frame_size=size, pcm=block)["packet"]
                packet_bytes += len(packet)
                decoded.extend(
                    oracle.request("decode", frame_size=size, packet=packet)["pcm"]
                )
        aligned = decoded[delay * channels : delay * channels + len(pcm)]
        signal = sum(x * x for x in pcm)
        error = sum((x - y) ** 2 for x, y in zip(pcm, aligned))
        assert signal > 0, "silent media is not a quality measurement"
        snr = 10 * math.log10(signal / max(error, 1))
        rows.append(
            dict(
                bitrate=bitrate,
                delay_samples=delay,
                snr_db=snr,
                decoded_clipped_samples=sum(abs(x) >= 32767 for x in aligned),
                packet_bytes=packet_bytes,
                pcm_sha256=digest(pcm_bytes(aligned)),
            )
        )
    return dict(
        label=label,
        source_sha256=digest(raw),
        samples=len(pcm) // channels,
        rate=rate,
        channels=channels,
        metrics=rows,
        limitation="waveform SNR only; no PESQ/POLQA/listening or native encoder claim",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=12)
    parser.add_argument("--adversarial-cases", type=int, default=64)
    parser.add_argument("--speech-wav", type=Path)
    parser.add_argument("--music-wav", type=Path)
    args = parser.parse_args()
    assert 1 <= args.frames <= 3000 and 0 <= args.adversarial_cases <= 4096
    executable = args.executable.resolve(strict=True)
    with Oracle(executable) as oracle:
        manifest = oracle.manifest
    report = dict(
        schema="moosicbox.opus.expanded-reference.v1",
        executable_sha256=digest(executable.read_bytes()),
        script_sha256=digest(Path(__file__).read_bytes()),
        reference=manifest,
        platform=platform.platform(),
        machine=platform.machine(),
        frozen=frozen(executable),
        matrix=matrix(executable, args.frames),
        multistream=multistream(executable),
        adversarial=adversarial(executable, args.adversarial_cases),
        real_media=[],
        limitation="reference-only; not native conformance or perceptual acceptance",
    )
    for label in ("speech", "music"):
        path = getattr(args, label + "_wav")
        if path:
            report["real_media"].append(quality(executable, path, label))
    payload = json.dumps(report, sort_keys=True, indent=2) + "\n"
    args.output.write_text(payload)
    print(
        json.dumps(
            dict(
                report=str(args.output),
                sha256=digest(payload.encode()),
                frozen=report["frozen"],
                matrix_cases=len(report["matrix"]),
                multistream_cases=len(report["multistream"]),
                adversarial=report["adversarial"],
                real_media_cases=len(report["real_media"]),
            ),
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
