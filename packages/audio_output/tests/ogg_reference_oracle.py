"""Optional manual qualification: decode family-zero Ogg with a selected oracle.

Ordinary Rust tests do not invoke this script or require its environment variable.
Their native-only pipeline runs in process; the existing `opus` feature enables
in-process C cross-decoding. Frozen C PCM expectations cover exact native parity.
No PATH codec, ctypes, dynamic library lookup, or C compilation is permitted here.
"""

import json
import os
import struct
import subprocess
import sys


def main():
    executable = os.environ.get("MOOSICBOX_OPUS_REFERENCE_ORACLE")
    if not executable or not os.path.isabs(executable):
        raise RuntimeError(
            "set MOOSICBOX_OPUS_REFERENCE_ORACLE to the qualified absolute executable"
        )
    data = sys.stdin.buffer.read()
    packets = []
    partial = bytearray()
    offset = 0
    final_granule = None
    while offset < len(data):
        if data[offset : offset + 4] != b"OggS":
            raise RuntimeError("invalid Ogg capture pattern")
        flags = data[offset + 5]
        granule = struct.unpack_from("<Q", data, offset + 6)[0]
        count = data[offset + 26]
        laces = data[offset + 27 : offset + 27 + count]
        body = offset + 27 + count
        for size in laces:
            partial.extend(data[body : body + size])
            body += size
            if size < 255:
                packets.append(bytes(partial))
                partial.clear()
        if flags & 4:
            final_granule = granule
        offset = body
    if partial or final_granule is None or len(packets) < 3:
        raise RuntimeError("incomplete Ogg Opus stream")
    head = packets[0]
    if head[:8] != b"OpusHead" or head[18] != 0:
        raise RuntimeError("this test helper requires family-zero OpusHead")
    channels = head[9]
    pre_skip = struct.unpack_from("<H", head, 10)[0]
    gain = struct.unpack_from("<h", head, 16)[0]
    with subprocess.Popen(
        [executable], stdin=subprocess.PIPE, stdout=subprocess.PIPE
    ) as oracle:

        def request(value):
            oracle.stdin.write((json.dumps(value) + "\n").encode())
            oracle.stdin.flush()
            reply = oracle.stdout.readline()
            if not reply:
                raise RuntimeError("oracle terminated without a reply")
            return json.loads(reply)

        assert request({"op": "decoder", "rate": 48000, "channels": channels})["ok"]
        if gain:
            assert request({"op": "decoder_configure", "controls": [[4034, gain]]})[
                "ok"
            ]
        pcm = []
        for packet in packets[2:]:
            reply = request(
                {"op": "decode", "packet": list(packet), "frame_size": 5760}
            )
            if len(reply["pcm"]) != reply["samples"] * channels:
                raise RuntimeError("oracle returned inconsistent frame length")
            pcm.extend(reply["pcm"])
        oracle.stdin.close()
        if oracle.wait() != 0:
            raise RuntimeError("oracle failed")
    if final_granule * channels > len(pcm) or final_granule < pre_skip:
        raise RuntimeError("invalid final granule")
    pcm = pcm[pre_skip * channels : final_granule * channels]
    sys.stdout.buffer.write(
        struct.pack("<" + "f" * len(pcm), *(sample / 32768.0 for sample in pcm))
    )


if __name__ == "__main__":
    main()
