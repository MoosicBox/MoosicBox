#!/usr/bin/env python3
"""Explicit pinned-C multistream interoperability fixture helper (test only).

MOOSICBOX_OPUS_REFERENCE_ORACLE is mandatory. No discovery, build or fallback.
`fixture` emits a C packet and independently mapped C decoded PCM as CSV lines.
`decode` accepts JSON {packet:[...]} on stdin and emits decoded PCM as CSV.
The C multistream decode is checked against independently instantiated elementary
C decoders, including different channel signals, silent output, PLC and reset.
"""
import importlib.util
import json
import os
from pathlib import Path
import sys

SOURCE = Path(__file__).resolve().parents[1] / "libopus/src/oracle.py"
spec = importlib.util.spec_from_file_location("opus_reference", SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
LAYOUT = dict(
    rate=48000, channels=4, streams=2, coupled_streams=1, mapping=[2, 0, 1, 255]
)


def request(oracle, op, **fields):
    result = oracle.request(op, **fields)
    if "error" in result:
        raise RuntimeError(result["error"])
    return result


def size(data, offset):
    first = data[offset]
    offset += 1
    if first >= 252:
        first += 4 * data[offset]
        offset += 1
    return first, offset


def split_first(data):
    """Independent Appendix-B reader; rebuild ordinary code-3 VBR."""
    toc = data[0]
    offset = 1
    padding = 0
    code = toc & 3
    count, vbr = (1, False) if code == 0 else (2, code == 2)
    if code == 3:
        control = data[offset]
        offset += 1
        count, vbr = control & 63, bool(control & 128)
        assert 1 <= count <= 48
        if control & 64:
            while True:
                byte = data[offset]
                offset += 1
                padding += 254 if byte == 255 else byte
                if byte != 255:
                    break
    sizes = []
    if vbr:
        for _ in range(count - 1):
            length, offset = size(data, offset)
            sizes.append(length)
    length, offset = size(data, offset)
    sizes = sizes + [length] if vbr else [length] * count
    packet = [(toc & 252) | 3, 128 | count]
    for length in sizes[:-1]:
        if length < 252:
            packet.append(length)
        else:
            first = 252 + (length & 3)
            packet.extend([first, (length - first) // 4])
    end = offset + sum(sizes)
    assert end + padding <= len(data)
    packet.extend(data[offset:end])
    return packet, data[end + padding :]


def checked_decode(oracle, packet):
    request(oracle, "multistream_decoder", **LAYOUT)
    result = request(oracle, "multistream_decode", packet=packet, frame_size=960)
    first, second = split_first(packet)
    channels = []
    for width, elementary in [(2, first), (1, second)]:
        request(oracle, "decoder", rate=48000, channels=width)
        channels.append(
            request(oracle, "decode", packet=elementary, frame_size=960)["pcm"]
        )
    stereo, mono = channels
    expected = [
        x for i in range(960) for x in (mono[i], stereo[2 * i], stereo[2 * i + 1], 0)
    ]
    assert result["samples"] == 960
    assert (
        result["pcm"] == expected
    ), "C multistream channel order differs from independent streams"
    assert any(mono), "mono channel unexpectedly silent"
    assert any(
        stereo[2 * i] != stereo[2 * i + 1] for i in range(960)
    ), "coupled channels duplicated"
    loss = request(oracle, "multistream_decode", packet=None, frame_size=960)
    assert loss["samples"] == 960 and all(x == 0 for x in loss["pcm"][3::4])
    request(oracle, "multistream_reset", state="decoder")
    again = request(oracle, "multistream_decode", packet=packet, frame_size=960)
    assert again == result, "multistream reset did not restore initial decode"
    request(oracle, "multistream_gain", gain_q8=-1536)
    request(oracle, "multistream_reset", state="decoder")
    gained = request(oracle, "multistream_decode", packet=packet, frame_size=960)["pcm"]
    assert all(x == 0 for x in gained[3::4])
    assert sum(abs(x) for x in gained) < sum(abs(x) for x in expected)
    return result["pcm"], loss["pcm"], again["pcm"], gained


def main():
    executable = os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]
    with module.Oracle(executable) as oracle:
        if sys.argv[1:] == ["fixture"]:
            request(oracle, "multistream_encoder", **LAYOUT)
            pcm = [
                x
                for i in range(960)
                for x in (
                    6000 if i % 51 < 25 else -6000,
                    9000 if i % 37 < 18 else -9000,
                    3000 if i % 23 < 11 else -3000,
                    24000,
                )
            ]
            packet = request(oracle, "multistream_encode", pcm=pcm, frame_size=960)[
                "packet"
            ]
            decoded = checked_decode(oracle, packet)
            print(",".join(map(str, packet)))
            for state_pcm in decoded:
                print(",".join(map(str, state_pcm)))
        elif sys.argv[1:] == ["decode"]:
            decoded = checked_decode(oracle, json.load(sys.stdin)["packet"])
            print(",".join(map(str, decoded[0])))
        else:
            raise SystemExit("usage: multistream_oracle.py fixture|decode")


if __name__ == "__main__":
    main()
