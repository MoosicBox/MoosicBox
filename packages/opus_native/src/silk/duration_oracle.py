"""Public persistent SILK oracle using an explicitly selected reference runner.

Usage: MOOSICBOX_OPUS_REFERENCE_ORACLE=/path/reference_oracle python3 -B duration_oracle.py RATE CHANNELS [--rates|--channels|--recover|--fec-long]
No automatic build, cache discovery, shared-library loading or fixture writes.
"""

import math
import os
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

executable = os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]
rate, channels = map(int, sys.argv[1:3])
recover = "--recover" in sys.argv[3:]
long = "--fec-long" in sys.argv[3:]
fec = "--fec-packets" in sys.argv[3:] or recover or long
rates = "--rates" in sys.argv[3:]
channel_changes = "--channels" in sys.argv[3:]
encode_rate = 48000 if rates else rate


def checked(oracle, operation, **fields):
    result = oracle.request(operation, **fields)
    assert "error" not in result, result
    return result


with Oracle(executable) as oracle:
    checked(oracle, "encoder", rate=encode_rate, channels=channels, application=2048)
    maximum = (
        1103
        if rates or channel_changes
        else {8000: 1101, 12000: 1102, 16000: 1103, 48000: 1103}[rate]
    )
    checked(
        oracle,
        "configure",
        controls=[
            [11002, 1000],
            [4022, channels],
            [4002, 16000 * channels],
            [4006, 1],
            [4004, maximum],
        ],
    )
    if fec:
        checked(
            oracle,
            "configure",
            controls=[[4012, 1], [4014, 30], [4002, 32000 * channels], [4006, 0]],
        )
    for reset in range(2):
        records = []
        position = 0
        durations = (
            ([40, 60] * 5 if long else [20] * 10)
            if fec
            else [20, 20, 10, 10, 40, 60, 20, 10, 20, 20]
        )
        for index, ms in enumerate(durations):
            if rates or channel_changes:
                bandwidth = (
                    [1101, 1101, 1102, 1102, 1103, 1103, 1102, 1101, 1103, 1101][index]
                    if rates
                    else 1103
                )
                coded = (
                    [1, 1, 2, 2, 1, 2, 1, 2, 2, 1][index]
                    if channel_changes
                    else channels
                )
                checked(
                    oracle,
                    "encoder",
                    rate=encode_rate,
                    channels=channels,
                    application=2048,
                )
                checked(
                    oracle,
                    "configure",
                    controls=[
                        [11002, 1000],
                        [4022, coded],
                        [4002, 32000 * channels],
                        [4006, 1],
                        [4004, bandwidth],
                        [4008, bandwidth],
                    ],
                )
            n = encode_rate * ms // 1000
            pcm = []
            for i in range(n):
                t = (position + i) / encode_rate
                for ch in range(channels):
                    pcm.append(
                        round(
                            9000 * math.sin(2 * math.pi * (173 + ch * 67) * t)
                            + 2300 * math.sin(2 * math.pi * 417 * t)
                        )
                    )
            position += n
            packet = checked(oracle, "encode", frame_size=n, pcm=pcm)["packet"]
            assert len(packet) > 1 and packet[0] >> 3 < 12, packet[:1]
            if rates:
                assert packet[0] >> 5 == bandwidth - 1101, packet[0]
            if channel_changes:
                assert 1 + ((packet[0] >> 2) & 1) == coded, packet[0]
            records.append((ms, packet))
        if recover:
            records = records[:3] + [records[4]] + records[4:]
        checked(oracle, "decoder", rate=rate, channels=channels)
        for index, (ms, packet) in enumerate(records):
            count = rate * ms // 1000
            output = checked(
                oracle,
                "decode",
                packet=packet,
                frame_size=count,
                fec=recover and index == 3,
            )
            assert output["samples"] == count
            sys.stdout.buffer.write(struct.pack("<IIII", reset, ms, len(packet), count))
            sys.stdout.buffer.write(bytes(packet))
            sys.stdout.buffer.write(
                struct.pack("<" + "h" * len(output["pcm"]), *output["pcm"])
            )
