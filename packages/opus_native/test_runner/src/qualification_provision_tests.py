"""Provisioning policy tests; all network responses mocked, no downloads."""

from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from qualification_provision import download


class ProvisionTests(unittest.TestCase):
    def test_http_is_rejected_before_network(self):
        with patch("qualification_provision.urlopen") as request:
            with self.assertRaises(ValueError):
                download("http://invalid.example/fixture", Path("unused"), 4)
            request.assert_not_called()

    def test_redirect_downgrade_and_wrong_size_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture"
            for url, payload in (
                ("http://invalid.example/file", b"1234"),
                ("https://invalid.example/file", b"12345"),
            ):
                response = MagicMock()
                response.geturl.return_value = url
                response.read.return_value = payload
                with patch("qualification_provision.urlopen") as request:
                    request.return_value.__enter__.return_value = response
                    with self.assertRaises(ValueError):
                        download("https://invalid.example/file", path, 4)
                self.assertFalse(path.exists())

    def test_exact_bytes_use_exclusive_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture"
            response = MagicMock()
            response.geturl.return_value = "https://invalid.example/file"
            response.read.return_value = b"1234"
            with patch("qualification_provision.urlopen") as request:
                request.return_value.__enter__.return_value = response
                download("https://invalid.example/file", path, 4)
                self.assertEqual(path.read_bytes(), b"1234")
                with self.assertRaises(FileExistsError):
                    download("https://invalid.example/file", path, 4)


if __name__ == "__main__":
    unittest.main()
