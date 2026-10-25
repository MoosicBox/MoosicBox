"""Portable selection tests: no Cargo, codec executable, network or baseline writes."""

import argparse
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
import wave

import qualification_ci as ci


class SelectionTests(unittest.TestCase):
    def test_current_ignored_inventory_is_explicitly_classified(self):
        required = ci.acceptance_inventory()
        self.assertGreater(len(required), 40)
        self.assertIn(
            "encoder::silk::public::public_floor_synthetic_matrix",
            [r[2] for r in required],
        )
        self.assertIn(
            "encoder::silk::public::public_floor_real_speech_matrix",
            [r[2] for r in required],
        )
        self.assertNotIn(
            "internal_practical_floor_media_matrix", [r[2] for r in required]
        )
        self.assertIn("internal_practical_floor_media_matrix", ci.DEVELOPMENT_ONLY)
        self.assertIn(
            "encoder::adaptive::tests::reference_public_practical_rd_quality",
            [r[2] for r in required],
        )
        self.assertNotIn("reference_public_practical_rd_quality", ci.DEVELOPMENT_ONLY)
        self.assertIn(
            "reference_unredundant_closed_loop_energy_proof", ci.DEVELOPMENT_ONLY
        )
        self.assertNotIn(
            "reference_unredundant_closed_loop_energy_proof",
            [r[2].rsplit("::", 1)[-1] for r in required],
        )
        self.assertIn(
            "encoder::adaptive::tests::reference_practical_unredundant_transition_quality",
            [r[2] for r in required],
        )
        self.assertIn(
            "encoder::adaptive::tests::reference_practical_auto_steady_quality",
            [r[2] for r in required],
        )

    def test_required_exact_tests_execute_with_oracle_and_media(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            execution.run.return_value = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
            env = {
                "MOOSICBOX_OPUS_REFERENCE_ORACLE": "explicit-oracle",
                "MOOSICBOX_OPUS_HYBRID_REAL_PCM": "reviewed.pcm",
                "MOOSICBOX_OPUS_SPEECH_WAV": "reviewed.wav",
            }
            ci.live_acceptance(execution, "release", env)
            required = ci.acceptance_inventory()
            self.assertEqual(len(execution.run.call_args_list), len(required))
            for call, (_, target, name) in zip(execution.run.call_args_list, required):
                argv = call.args[0]
                self.assertIn(name, argv)
                self.assertIn("--exact", argv)
                self.assertIn("--ignored", argv)
                self.assertIn("--release", argv)
                self.assertIn("--offline", argv)
                self.assertEqual(call.kwargs["env"], env)
                if target:
                    self.assertEqual(argv[argv.index("--test") + 1], target)
                self.assertNotIn("internal_practical_floor_media_matrix", argv)
            self.assertTrue((Path(directory) / "acceptance-inventory.json").exists())

    def test_zero_selected_test_is_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            execution.run.return_value = (
                "test result: ok. 0 passed; 0 failed; 0 ignored;\n"
            )
            with self.assertRaisesRegex(RuntimeError, "did not execute once"):
                ci.live_acceptance(execution, "release", {})
            self.assertEqual(execution.run.call_count, 1)

    def test_required_failure_stops_instead_of_skipping(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            execution.run.side_effect = RuntimeError("required test failed")
            with self.assertRaisesRegex(RuntimeError, "required test failed"):
                ci.live_acceptance(execution, "release", {})
            self.assertEqual(execution.run.call_count, 1)

    def test_native_regression_remains_unfiltered(self):
        execution = Mock()
        ci.native_tests(execution, "release")
        argv = execution.run.call_args.args[0]
        self.assertIn("--lib", argv)
        self.assertIn("--tests", argv)
        self.assertNotIn("--skip", argv)
        self.assertNotIn("--ignored", argv)
        self.assertNotIn("--exclude", argv)

    def test_pcm_transform_is_exact_and_identity_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            music, speech = root / "music.wav", root / "speech.wav"
            pcm = b"\x01\x00\xff\x7f" * (32 * 960)
            with wave.open(str(music), "wb") as output:
                output.setnchannels(2)
                output.setsampwidth(2)
                output.setframerate(48000)
                output.writeframes(pcm)
            speech.write_bytes(b"mock-reviewed-speech")
            manifest = {"synthetic_test_fixture": True}
            with patch(
                "qualification_inputs.verify_inputs",
                return_value=(manifest, {"music": music, "speech": speech}),
            ):
                env = ci.media_environment(
                    root, root, {"retained": "yes"}, Path("oracle")
                )
            self.assertEqual(
                Path(env["MOOSICBOX_OPUS_HYBRID_REAL_PCM"]).read_bytes(), pcm
            )
            self.assertEqual(env["MOOSICBOX_OPUS_SPEECH_WAV"], str(speech))
            self.assertEqual(env["retained"], "yes")
            import json

            receipt = json.loads((root / "media-transform.json").read_text())
            self.assertEqual(
                receipt["output_sha256"],
                ci.digest(Path(env["MOOSICBOX_OPUS_HYBRID_REAL_PCM"])),
            )
            self.assertEqual(receipt["output_bytes"], len(pcm))

    def test_unreviewed_media_rejected_before_transform(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch(
                "qualification_inputs.verify_inputs",
                side_effect=ValueError("checksum mismatch"),
            ):
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    ci.media_environment(root, root, {}, Path("oracle"))
            self.assertFalse((root / "media-transform.json").exists())

    def test_strict_matrix_required_argv_and_acceptance(self):
        import json

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            execution = ci.Execution(root)

            def fake_child(argv, **kwargs):
                self.assertIn("--media-directory", argv)
                self.assertIn("--build-receipt", argv)
                self.assertNotIn("--diagnostic", argv)
                self.assertIn(str(ci.HERE / "qualification_matrix.py"), argv)
                self.assertEqual(kwargs["timeout"], 1800)
                (root / "quality.json").write_text(
                    json.dumps({"accepted": True, "failures": 0})
                )
                return Mock(returncode=0)

            with patch.object(ci.subprocess, "run", side_effect=fake_child):
                ci.strict_matrix(execution, Path("native"), Path("reference"))
            (root / "quality.json").write_text(
                json.dumps({"accepted": False, "failures": 1})
            )
            with patch.object(execution, "run"):
                with self.assertRaisesRegex(RuntimeError, "not accepted"):
                    ci.strict_matrix(execution, Path("native"), Path("reference"))

    def test_real_subprocess_arguments_mocked(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = ci.Execution(Path(directory))
            with patch.object(ci.subprocess, "run") as child:
                child.return_value.returncode = 0
                ci.native_tests(execution, "release")
                self.assertEqual(child.call_args.kwargs["timeout"], 1800)
                self.assertEqual(child.call_args.kwargs["env"]["CARGO_BUILD_JOBS"], "2")
                self.assertEqual(
                    child.call_args.kwargs["env"]["CARGO_NET_OFFLINE"], "true"
                )


if __name__ == "__main__":
    unittest.main()
