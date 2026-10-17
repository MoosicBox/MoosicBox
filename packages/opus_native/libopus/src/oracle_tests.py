"""Explicit integration test: python3 -B oracle_tests.py /absolute/reference_oracle."""

import json
import math
from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from oracle import Oracle

EXECUTABLE = sys.argv.pop(1)


class ReferenceProtocolTests(unittest.TestCase):
    def test_nonzero_modes_rates_channels_and_losses(self):
        for rate in (8000, 12000, 16000, 24000, 48000):
            for channels in (1, 2):
                for mode, bandwidth in ((1000, 1103), (1001, 1105), (1002, 1105)):
                    if mode == 1001 and rate < 24000:
                        continue
                    with self.subTest(rate=rate, channels=channels, mode=mode):
                        with Oracle(EXECUTABLE) as oracle:
                            self.assertEqual(
                                oracle.manifest["revision"],
                                "34bba701ae97c913de719b1f7c10686f62cddb15",
                            )
                            oracle.request("encoder", rate=rate, channels=channels)
                            oracle.request("decoder", rate=rate, channels=channels)
                            oracle.request(
                                "configure",
                                controls=[
                                    [11002, mode],
                                    [4002, 64000],
                                    [4004, bandwidth],
                                    [4006, 0],
                                ],
                            )
                            n = rate // 50
                            pcm = [
                                int(12000 * math.sin(2 * math.pi * 440 * i / rate))
                                for i in range(n)
                                for _ in range(channels)
                            ]
                            energy = 0
                            for _ in range(4):
                                packet = oracle.request(
                                    "encode", frame_size=n, pcm=pcm
                                )["packet"]
                                output = oracle.request(
                                    "decode", frame_size=n, packet=packet
                                )
                                self.assertEqual(output["samples"], n)
                                self.assertEqual(len(output["pcm"]), n * channels)
                                energy += sum(x * x for x in output["pcm"])
                            self.assertGreater(energy, 0)
                            output = oracle.request("decode", frame_size=n, packet=None)
                            self.assertEqual(output["samples"], n)
                            self.assertTrue(any(output["pcm"]))
                            oracle.request("reset", state="decoder")
                            self.assertFalse(
                                any(
                                    oracle.request("decode", frame_size=n, packet=None)[
                                        "pcm"
                                    ]
                                )
                            )
                            oracle.request("reset", state="encoder")

    def test_rejects_bad_requests(self):
        for request in (
            {"op": "unknown"},
            {"op": "decode", "frame_size": 5761},
            {"op": "encoder", "rate": 44100, "channels": 1},
            {"op": "encoder", "rate": 48000, "channels": 3},
        ):
            result = subprocess.run(
                [EXECUTABLE],
                input=json.dumps(request) + "\n",
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
        result = subprocess.run(
            [EXECUTABLE], input="x" * 1048577, text=True, capture_output=True
        )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
