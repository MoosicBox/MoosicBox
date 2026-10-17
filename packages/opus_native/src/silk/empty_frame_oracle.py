"""Legal empty constituent and recovery, using an explicit public C runner."""

import math
import os
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

channels, rate = map(int, sys.argv[1:3])
with Oracle(os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]) as oracle:

    def checked(op, **fields):
        value = oracle.request(op, **fields)
        assert "error" not in value, value
        return value

    checked("encoder", rate=48000, channels=channels, application=2051)
    checked("decoder", rate=rate, channels=channels)
    checked("configure", controls=[[4002, 96000], [4010, 0]])
    for step in range(8):
        pcm = [
            int(
                7000
                * math.sin(2 * math.pi * (220 + ch * 113) * (step * 960 + i) / 48000)
            )
            for i in range(960)
            for ch in range(channels)
        ]
        packet = checked("encode", frame_size=960, pcm=pcm)["packet"]
        assert packet[0] >> 3 >= 16 and packet[0] & 3 == 0
        if step in [2, 5]:
            size = len(packet) - 1
            length = (
                [size]
                if size < 252
                else [252 + (size & 3), (size - (252 + (size & 3))) // 4]
            )
            packet = [(packet[0] & 252) | 2] + length + packet[1:]
        samples = rate // 25 if step in [2, 5] else rate // 50
        value = checked("decode", packet=packet, frame_size=samples)
        assert value["samples"] == samples
        sys.stdout.buffer.write(
            struct.pack("<II", len(packet), len(value["pcm"])) + bytes(packet)
        )
        sys.stdout.buffer.write(
            struct.pack("<" + "h" * len(value["pcm"]), *value["pcm"])
        )
