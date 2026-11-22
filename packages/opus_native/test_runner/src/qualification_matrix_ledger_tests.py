"""Offline support inventory tests; no codec processes or acceptance claims."""

from pathlib import Path
import tempfile
import unittest

from qualification_matrix_ledger import build_ledger


class LedgerTests(unittest.TestCase):
    def test_complete_intended_inventory_never_claims_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            ledger = build_ledger(Path(directory))
        self.assertFalse(ledger["accepted"])
        self.assertEqual(ledger["silk_required_count"], 192)
        self.assertEqual(len(ledger["silk_internal_cells"]), 192)
        self.assertEqual(len(ledger["public_roundtrip_cells"]), 88)
        self.assertEqual(len(ledger["public_performance_cases"]), 16)
        self.assertEqual(len(ledger["real_media_cells"]), 31)
        self.assertEqual(len(ledger["silence_cells"]), 36)
        self.assertEqual(len(ledger["candidate_performance_cases"]), 0)
        self.assertEqual(set(ledger["missing_reviewed_media"]), {"speech", "music"})
        self.assertIn("quality", ledger["runner_metadata"])
        self.assertTrue(ledger["runner_metadata"]["quality"]["loops"])
        self.assertTrue(
            all(
                row["interface"] == "internal_test_only_candidate"
                for row in ledger["silk_internal_cells"]
            )
        )


if __name__ == "__main__":
    unittest.main()
