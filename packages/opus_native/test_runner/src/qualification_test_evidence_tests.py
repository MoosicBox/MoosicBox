"""Writer regressions using actual subprocesses; no qualification receipts are mocked."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

import qualification_finish_bundle as bundle
import qualification_test_evidence as writer


class EvidenceTests(unittest.TestCase):
    def test_failed_command_retains_bytes_and_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "test.log"
            with self.assertRaises(RuntimeError):
                writer.run([sys.executable, "-c", "print('before exit');exit(7)"],
                           log, os.environ, 30, root)
            self.assertEqual(log.read_text().strip(), "before exit")
            status = json.loads(log.with_suffix(".command.json").read_bytes())
            self.assertEqual(status["exit_code"], 7)
            self.assertFalse(status["timed_out"])

    def test_timeout_not_reported_as_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "timeout.log"
            with self.assertRaises(RuntimeError):
                writer.run([sys.executable, "-c", "import time;time.sleep(30)"],
                           log, os.environ, 0.1, root)
            status = json.loads(log.with_suffix(".command.json").read_bytes())
            self.assertTrue(status["timed_out"])
            self.assertIsNone(status["exit_code"])

    def test_index_is_not_acceptance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            platform = root / "platform"
            platform.mkdir()
            report = root / "report.json"
            report.write_text('{"source_before_sha256":"stale fixture"}')
            entries = dict.fromkeys(("performance", "pipeline", "unit_shards", "public_silk"), report.name)
            with self.assertRaises(ValueError):
                bundle.assemble(root, "platform", **entries)
            (platform / "ci.json").write_text("{}")
            bundle.assemble(root, "platform", **entries)
            path = root / "finish-bundle.json"
            self.assertEqual(json.loads(path.read_bytes())["pipeline"]["sha256"],
                             hashlib.sha256(report.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, "not current"):
                writer.verify.aggregate(path)
            with self.assertRaises(FileExistsError):
                bundle.assemble(root, "platform", **entries)


if __name__ == "__main__":
    unittest.main()
