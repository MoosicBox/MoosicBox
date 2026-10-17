"""Persistent public-C mode-transition oracle; explicit qualified runner only."""

import math
import os
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

channels = int(sys.argv[1])
rate = int(sys.argv[2])
with Oracle(os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]) as oracle:

    def checked(op, **fields):
        result = oracle.request(op, **fields)
        assert "error" not in result, result
        return result

    checked("encoder", rate=48000, channels=channels, application=2049)
    checked("decoder", rate=rate, channels=channels)
    checked("configure", controls=[[4010, 0]])
    expected = ["SSSSSS", "HHHHHH", "HCCCCC", "HHHHHH", "SSSSSS", "HCCCCC", "SSSSSS"]
    for cycle in range(2):
        if cycle:
            checked("reset", state="encoder")
            checked("reset", state="decoder")
        for stage, mode in enumerate([1000, 1001, 1002, 1001, 1000, 1002, 1000]):
            if len(sys.argv) > 3 and sys.argv[3] == "abrupt":
                checked("encoder", rate=48000, channels=channels, application=2049)
                checked("configure", controls=[[4010, 0]])
            bandwidth = 1103 if mode == 1000 else 1105
            bitrate = {1000: 24000, 1001: 48000, 1002: 96000}[mode]
            checked(
                "configure",
                controls=[
                    [11002, mode],
                    [4004, bandwidth],
                    [4008, bandwidth],
                    [4002, bitrate],
                ],
            )
            actual = ""
            for frame in range(6):
                time = stage * 6 + frame
                pcm = [
                    int(
                        7000
                        * math.sin(
                            2 * math.pi * (220 + ch * 113) * (time * 960 + i) / 48000
                        )
                        + 1900 * math.sin(2 * math.pi * 731 * (time * 960 + i) / 48000)
                    )
                    for i in range(960)
                    for ch in range(channels)
                ]
                packet = checked("encode", frame_size=960, pcm=pcm)["packet"]
                toc = packet[0] >> 3
                actual += "S" if toc < 12 else "H" if toc < 16 else "C"
                output = checked("decode", packet=packet, frame_size=rate // 50)
                assert output["samples"] == rate // 50
                sys.stdout.buffer.write(
                    struct.pack("<II", len(packet), len(output["pcm"])) + bytes(packet)
                )
                sys.stdout.buffer.write(
                    struct.pack("<" + "h" * len(output["pcm"]), *output["pcm"])
                )
            assert actual == (
                (("S" if mode == 1000 else "H" if mode == 1001 else "C") * 6)
                if len(sys.argv) > 3
                else expected[stage]
            ), (stage, actual, expected[stage])
