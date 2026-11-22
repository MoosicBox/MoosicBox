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

    def test_decoder_gain_nonzero_pcm_and_reset(self):
        with Oracle(EXECUTABLE) as oracle:
            oracle.request("encoder", rate=48000, channels=1)
            pcm = [
                int(8000 * math.sin(2 * math.pi * 440 * i / 48000)) for i in range(960)
            ]
            packets = [
                oracle.request("encode", frame_size=960, pcm=pcm)["packet"]
                for _ in range(3)
            ]
            outputs = []
            for gain in (0, -1536):
                oracle.request("decoder", rate=48000, channels=1)
                oracle.request("decoder_configure", controls=[[4034, gain]])
                decoded = []
                for packet in packets:
                    decoded.extend(
                        oracle.request("decode", frame_size=960, packet=packet)["pcm"]
                    )
                oracle.request("reset", state="decoder")
                replay = []
                for packet in packets:
                    replay.extend(
                        oracle.request("decode", frame_size=960, packet=packet)["pcm"]
                    )
                self.assertEqual(decoded, replay)
                outputs.append(sum(x * x for x in decoded))
            self.assertGreater(outputs[0], outputs[1])
            self.assertGreater(outputs[1], 0)

    def test_multistream_layouts_replay_loss_and_gain(self):
        from qualification_matrix import multistream

        rows = multistream(EXECUTABLE)
        self.assertEqual(len(rows), 4)
        self.assertTrue(
            all(row["replay_exact"] and row["gain_checked"] for row in rows)
        )

    def test_multistream_rejects_invalid_layouts_and_gain(self):
        valid = dict(
            op="multistream_decoder",
            rate=48000,
            channels=2,
            streams=1,
            coupled_streams=1,
            mapping=[0, 1],
        )
        for changes in (
            dict(channels=0),
            dict(mapping=[0]),
            dict(streams=0),
            dict(coupled_streams=2),
            dict(mapping=[0, 2]),
            dict(rate=44100),
        ):
            result = subprocess.run(
                [EXECUTABLE],
                input=json.dumps(dict(valid, **changes)) + "\n",
                text=True,
                capture_output=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
        result = subprocess.run(
            [EXECUTABLE],
            input=json.dumps(valid)
            + "\n"
            + json.dumps(dict(op="multistream_gain", gain_q8=32768))
            + "\n",
            text=True,
            capture_output=True,
            timeout=15,
        )
        self.assertEqual(result.returncode, 1)

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
