"""Generate one actual changing-mode file's packets and persistent public-C PCM.

Requires an explicit qualified Cargo runner. No builds, fixture writes or cache search.
"""

import importlib.util
import math
import os
from pathlib import Path
import struct
import sys

client = Path(__file__).resolve().parents[2] / "opus_native/libopus/src/oracle.py"
spec = importlib.util.spec_from_file_location("moosicbox_reference_client", client)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
channels = int(sys.argv[1])
executable = os.environ.get("MOOSICBOX_OPUS_REFERENCE_ORACLE")
if not executable:
    raise RuntimeError(
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to the qualified Cargo reference_oracle executable"
    )
with module.Oracle(executable) as oracle:

    def checked(op, **fields):
        response = oracle.request(op, **fields)
        assert "error" not in response, response
        return response

    checked("encoder", rate=48000, channels=channels, application=2049)
    checked("decoder", rate=48000, channels=channels)
    checked("configure", controls=[[4010, 0]])
    expected = ["SSSSSS", "HHHHHH", "HCCCCC", "HHHHHH", "SSSSSS", "HCCCCC", "SSSSSS"]
    for stage, mode in enumerate([1000, 1001, 1002, 1001, 1000, 1002, 1000]):
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
            position = (stage * 6 + frame) * 960
            pcm = [
                int(
                    7000
                    * math.sin(2 * math.pi * (220 + ch * 113) * (position + i) / 48000)
                    + 1900 * math.sin(2 * math.pi * 731 * (position + i) / 48000)
                )
                for i in range(960)
                for ch in range(channels)
            ]
            packet = checked("encode", frame_size=960, pcm=pcm)["packet"]
            toc = packet[0] >> 3
            actual += "S" if toc < 12 else "H" if toc < 16 else "C"
            decoded = checked("decode", frame_size=960, packet=packet)
            assert decoded["samples"] == 960
            assert len(decoded["pcm"]) == 960 * channels
            sys.stdout.buffer.write(
                struct.pack("<II", len(packet), len(decoded["pcm"])) + bytes(packet)
            )
            sys.stdout.buffer.write(
                struct.pack("<" + "h" * len(decoded["pcm"]), *decoded["pcm"])
            )
        assert actual == expected[stage], (stage, actual, expected[stage])
