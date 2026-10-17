"""Missing SILK LBRR must synthesize missing audio, then decode current packet normally."""

import math
import os
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

coded, channels, rate, missing = map(int, sys.argv[1:5])
with Oracle(os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]) as oracle:

    def checked(op, **fields):
        value = oracle.request(op, **fields)
        assert "error" not in value, value
        return value

    checked("encoder", rate=48000, channels=coded, application=2048)
    checked("decoder", rate=rate, channels=channels)
    checked(
        "configure",
        controls=[
            [11002, 1000],
            [4004, 1103],
            [4008, 1103],
            [4002, 32000],
            [4010, 0],
            [4012, 0],
        ],
    )

    def emit(packet, fec, samples):
        value = checked("decode", packet=packet, frame_size=samples, fec=fec)
        assert value["samples"] == samples
        sys.stdout.buffer.write(
            struct.pack("<III", int(fec), len(packet), len(value["pcm"]))
            + bytes(packet)
        )
        sys.stdout.buffer.write(
            struct.pack("<" + "h" * len(value["pcm"]), *value["pcm"])
        )

    for cycle in range(2):
        if cycle:
            checked("reset", state="encoder")
            checked("reset", state="decoder")
        for step in range(8):
            pcm = [
                int(
                    7000
                    * math.sin(
                        2 * math.pi * (220 + ch * 113) * (step * 960 + i) / 48000
                    )
                )
                for i in range(960)
                for ch in range(coded)
            ]
            packet = checked("encode", frame_size=960, pcm=pcm)["packet"]
            assert packet[0] >> 3 < 12 and packet[0] & 3 == 0
            if step in [0, 4]:
                emit(packet, True, rate * missing // 1000)
            emit(packet, False, rate // 50)
