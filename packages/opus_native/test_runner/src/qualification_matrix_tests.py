"""Silence matrix coverage tests use fake codec processes, not acceptance evidence."""

import unittest
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import PurePosixPath, PureWindowsPath
from unittest.mock import patch

import qualification_matrix as matrix


class SourceIdentityTests(unittest.TestCase):
    def test_paths_use_same_case_sensitive_order_and_bytes_on_all_hosts(self):
        for root in (PurePosixPath('/repo'), PureWindowsPath('C:/repo')):
            keys = sorted(matrix._source_key(root / name, root)
                          for name in ('pkg/src/A.rs', 'pkg/Cargo.toml', 'pkg/a.rs'))
            self.assertEqual(['/'.join(key) for key in keys],
                             ['pkg/Cargo.toml', 'pkg/a.rs', 'pkg/src/A.rs'])


class Codec:
    peak = 0
    wrong_duration = False

    def __init__(self, unused):
        self.mode = 1002
        self.channels = 1

    def __enter__(self):
        return self

    def __exit__(self, *unused):
        return False

    def request(self, operation, **fields):
        if operation in ("encoder", "decoder"):
            self.channels = fields["channels"]
            return {"ok": True}
        if operation == "configure":
            for control, value in fields["controls"]:
                if control == 11002:
                    self.mode = value
            return {"ok": True}
        if operation == "encode":
            return {"packet": [self.mode]}
        if operation == "decode":
            size = fields["frame_size"]
            return dict(
                samples=size - int(self.wrong_duration),
                pcm=[self.peak] * (size * self.channels),
            )
        raise AssertionError(operation)


class SilenceMatrixTests(unittest.TestCase):
    def collect(self):
        with patch.object(matrix, "Native", Codec), patch.object(
            matrix, "Oracle", Codec
        ), patch.object(matrix, "packet_mode", side_effect=lambda packet: packet[0]):
            return matrix.silence_matrix("unused", "unused")

    def tearDown(self):
        Codec.peak = 0
        Codec.wrong_duration = False

    def test_all_auto_and_explicit_silence_cases_run(self):
        rows = self.collect()
        self.assertEqual(len(rows), 36)
        self.assertTrue(all(row["status"] == "pass" for row in rows))
        self.assertEqual({row["mode"] for row in rows}, {0, 1000, 1001, 1002})
        self.assertEqual({row["channels"] for row in rows}, {1, 2})
        self.assertTrue(all(row["frames"] == 12 for row in rows))

    def test_one_lsb_passes_two_lsb_silence_fails(self):
        Codec.peak = 1
        self.assertTrue(all(row["status"] == "pass" for row in self.collect()))
        Codec.peak = 2
        self.assertTrue(
            all(row["status"] == "silence_above_1lsb" for row in self.collect())
        )

    def test_duration_mismatch_is_failure_not_skipped(self):
        Codec.wrong_duration = True
        self.assertTrue(all(row["status"] == "codec_failure" for row in self.collect()))


class NativeTransportTests(unittest.TestCase):
    def client(self, program):
        # Exercise actual cross-platform Python pipes, without Rust/C or selectors.
        client = matrix.Native.__new__(matrix.Native)
        client.errors = tempfile.TemporaryFile()
        client.process = subprocess.Popen(
            [sys.executable, "-u", "-c", program],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=client.errors,
        )
        client.REQUEST_TIMEOUT = 0.3
        self.addCleanup(client.__exit__, None, None, None)
        return client

    def assert_stopped(self, client):
        self.assertIsNotNone(client.process.poll())
        self.assertFalse(
            any(
                thread.name == "opus-native-exchange" and thread.is_alive()
                for thread in threading.enumerate()
            )
        )
        with self.assertRaises(RuntimeError):
            client.request("again")

    def test_persistent_roundtrips_without_selectors(self):
        client = self.client(
            "import sys,json\nfor line in sys.stdin:\n print(json.dumps({'ok':json.loads(line)['op']}),flush=True)"
        )
        self.assertEqual(client.request("first"), {"ok": "first"})
        self.assertEqual(client.request("second"), {"ok": "second"})

    def test_timeout_bounds_partial_line_and_reaps_child(self):
        client = self.client(
            "import sys,time; sys.stdin.readline(); sys.stdout.write('{'); sys.stdout.flush(); time.sleep(30)"
        )
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            client.request("partial")
        self.assertLess(time.monotonic() - started, 5)
        self.assert_stopped(client)

    def test_timeout_bounds_blocked_stdin_write(self):
        client = self.client("import time; time.sleep(30)")
        with self.assertRaises(TimeoutError):
            client.request("large", payload="x" * 900000)
        self.assert_stopped(client)

    def test_eof_truncation_invalid_json_and_oversize_fail_closed(self):
        programs = (
            "import sys; sys.stdin.readline()",
            "import sys; sys.stdin.readline(); sys.stdout.write('{}'); sys.stdout.flush()",
            "import sys; sys.stdin.readline(); print('not-json',flush=True)",
            "import sys; sys.stdin.readline(); sys.stdout.write('x'*1048577); sys.stdout.flush()",
        )
        for program in programs:
            with self.subTest(program=program):
                client = self.client(program)
                with self.assertRaises((RuntimeError, ValueError)):
                    client.request("bad")
                self.assert_stopped(client)

    def test_oversize_request_rejected_before_exchange(self):
        client = self.client(
            "import sys,json\nfor line in sys.stdin:\n print('{}',flush=True)"
        )
        with self.assertRaises(ValueError):
            client.request("large", payload="x" * 1048576)
        self.assertEqual(client.request("still-usable"), {})


if __name__ == "__main__":
    unittest.main()
