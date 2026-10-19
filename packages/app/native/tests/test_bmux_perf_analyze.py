"""Tiny offline analyzer regressions: no executable, Cargo, PTY or HTTP calls."""

import base64
import tempfile
from pathlib import Path
import unittest

from bmux_perf_analyze import KittyAnalyzer, replay_profile, stage_metrics


def apc(header, data=b""):
    return b"\x1b_G" + header + b";" + base64.b64encode(data) + b"\x1b\\"


class KittyTests(unittest.TestCase):
    def analyze(self, data, block=1):
        analyzer = KittyAnalyzer()
        for begin in range(0, len(data), block):
            analyzer.feed(data[begin : begin + block])
        return analyzer.finish()

    def test_chunk_count_is_not_upload_count(self):
        data = (
            apc(b"a=t,i=1,f=100,s=4,v=4,m=1", b"abc")
            + apc(b"m=0", b"def")
            + apc(b"a=p,i=1")
            + apc(b"a=d,i=1")
        )
        result = self.analyze(data)
        self.assertEqual(result["transmit_chunks"], 2)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 1)
        self.assertEqual(result["placements"], 1)
        self.assertEqual(result["deletions"], 1)
        self.assertFalse(result["incomplete_command"])
        self.assertFalse(result["incomplete_upload"])
        self.assertEqual(result, self.analyze(data, 65536))

    def test_repeat_hash_ignores_id_and_chunk_boundaries_not_geometry(self):
        data = (
            apc(b"a=t,i=1,f=100,s=4,v=4,m=1", b"abc")
            + apc(b"a=t,i=1,m=0", b"def")
            + apc(b"a=t,i=9,f=100,s=4,v=4", b"abcdef")
            + apc(b"a=t,i=10,f=100,s=5,v=4", b"abcdef")
        )
        result = self.analyze(data)
        self.assertEqual(result["completed_uploads"], 3)
        self.assertEqual(result["unique_source_hashes"], 2)
        self.assertEqual(result["repeat_uploads"], 1)
        self.assertEqual(result["repeat_encoded_payload_bytes"], 8)
        self.assertEqual(result["repeat_decoded_transport_bytes"], 6)

    def test_truncated_command_and_upload_are_not_complete(self):
        result = self.analyze(apc(b"a=t,i=1,m=1", b"abc") + b"\x1b_Gm=0;YWJj")
        self.assertEqual(result["completed_uploads"], 0)
        self.assertTrue(result["incomplete_command"])
        self.assertTrue(result["incomplete_upload"])

    def test_bad_base64_and_conflicting_id_not_hashed(self):
        data = b"\x1b_Ga=t,i=1,m=1;!!!!\x1b\\" + apc(b"a=t,i=2,m=0", b"abc")
        result = self.analyze(data)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["invalid_base64_chunks"], 1)
        self.assertEqual(result["conflicting_chunk_ids"], 1)
        self.assertEqual(result["hashed_uploads"], 0)

    def test_oversized_command_resynchronizes(self):
        analyzer = KittyAnalyzer(max_command_bytes=40)
        for data in (
            b"\x1b_G" + b"x" * 100,
            b"x" * 100,
            b"\x1b\\" + apc(b"a=t,i=2", b"abc"),
        ):
            analyzer.feed(data)
        result = analyzer.finish()
        self.assertEqual(result["oversized_commands"], 1)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 1)

    def test_default_action_and_single_read_oversize(self):
        result = self.analyze(apc(b"i=1,f=100", b"abc"))
        self.assertEqual(result["completed_uploads"], 1)
        analyzer = KittyAnalyzer(max_command_bytes=40)
        analyzer.feed(b"\x1b_G" + b"x" * 100 + b"\x1b\\" + apc(b"a=p,i=1"))
        self.assertEqual(analyzer.finish()["oversized_commands"], 1)
        self.assertEqual(analyzer.finish()["placements"], 1)

    def test_malformed_header_is_reported(self):
        result = self.analyze(b"\x1b_Ga=t,bad;YWJj\x1b\\")
        self.assertEqual(result["malformed_commands"], 1)
        self.assertEqual(result["completed_uploads"], 0)

    def test_non_direct_upload_not_hashed(self):
        result = self.analyze(apc(b"a=t,t=f,i=1", b"/tmp/source.png"))
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["unsupported_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 0)


class ReplayTests(unittest.TestCase):
    def test_identical_fixed_bytes_replayed_separately(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "terminal.bin"
            path.write_bytes(b"\x1b[2J\x1b[1;1HFixture text")
            result = replay_profile(path)
        first, second = result["stream_analyzer"], result["acceptance_screen"]
        self.assertEqual(first["sha256"], second["sha256"])
        self.assertEqual(first["bytes"], second["bytes"])
        self.assertGreater(first["feed_ns"], 0)
        self.assertGreater(second["feed_ns"], 0)


class StageTests(unittest.TestCase):
    def test_missing_stages_explicit_and_separate(self):
        result = stage_metrics(
            [
                "DEBUG bmux prepare elapsed_us=1000 layout_us=900",
                "DEBUG bmux prepare scroll-reuse elapsed_us=200",
                "DEBUG bmux presenter output present_us=3000 write_us=100 flush_us=1",
            ]
        )
        self.assertEqual(result["prepare"]["p95_ms"], 1)
        self.assertEqual(result["scroll_reuse"]["samples"], 1)
        self.assertEqual(result["write"]["p95_ms"], 0.1)
        self.assertEqual(result["resource_only"]["samples"], 0)
        self.assertIsNone(result["resource_only"]["p95_ms"])

    def test_nearest_rank_percentiles(self):
        result = stage_metrics(
            f"bmux prepare elapsed_us={i * 1000}" for i in range(1, 21)
        )
        self.assertEqual(
            result["prepare"], dict(samples=20, p50_ms=10, p95_ms=19, max_ms=20)
        )


if __name__ == "__main__":
    unittest.main()
